use godot::classes::{INode, Node, Node3D};
use godot::global::{randf_range, randi_range};
use godot::prelude::*;

use crate::aim_model::{ROLL_SPEED_FACTOR, V2};
use crate::ball::BallType;
use crate::ball_rack::BallRack;
use crate::pool_cue::PoolCue;
use crate::shot_planner::{Shot, Table};
use crate::table_query::{
    BALL_RADIUS, HALF_X, HALF_Z, belongs_to, cue_ball, object_balls, pockets, to_v2,
};

const THINK_SECONDS: f32 = 0.8;
const ROTATE_SECONDS: f32 = 0.7;
const CHARGE_SECONDS: f32 = 0.7;
/// Numbered balls in a full rack.
const FULL_RACK: usize = 15;
/// Power of the last-resort shot when no ball can be reached at all.
const BLIND_POWER: f32 = 0.6;

#[derive(Default, Clone, Copy, PartialEq)]
enum BotState {
    #[default]
    Idle,
    Thinking,
    Rotating,
    Charging,
}

/// Plays the bot's turn: picks a shot, swings the cue round to it, draws back
/// and fires, at a pace a human can follow.
#[derive(GodotClass)]
#[class(init, base=Node)]
pub struct BotPlayer {
    base: Base<Node>,

    #[init(node = "../PoolCue")]
    pool_cue: OnReady<Gd<PoolCue>>,
    #[init(node = "../BallRack")]
    ball_rack: OnReady<Gd<BallRack>>,
    #[init(node = "../Pockets")]
    pockets: OnReady<Gd<Node3D>>,

    /// Largest aim error per difficulty, in degrees. At this table's scale,
    /// 1 degree over 40 units misses by about 0.7, nearly a ball radius.
    #[export]
    #[init(val = 4.0)]
    easy_aim_error: f32,
    #[export]
    #[init(val = 3.0)]
    medium_aim_error: f32,
    #[export]
    #[init(val = 0.3)]
    hard_aim_error: f32,

    state: BotState,
    group: BallType,
    difficulty: i64,
    elapsed: f32,
    from_yaw: f32,
    to_yaw: f32,
    power: f32,
}

#[godot_api]
impl INode for BotPlayer {
    fn physics_process(&mut self, delta: f64) {
        self.elapsed += delta as f32;

        match self.state {
            BotState::Idle => {}
            BotState::Thinking => {
                if self.elapsed >= THINK_SECONDS {
                    self.choose_shot();
                }
            }
            BotState::Rotating => {
                let t = (self.elapsed / ROTATE_SECONDS).min(1.0);
                let eased = t * t * (3.0 - 2.0 * t);
                let turn = shortest_turn(self.from_yaw, self.to_yaw);
                self.pool_cue.bind_mut().set_yaw(self.from_yaw + turn * eased);
                if t >= 1.0 {
                    self.pool_cue.bind_mut().begin_bot_charge();
                    self.enter(BotState::Charging);
                }
            }
            BotState::Charging => {
                let t = (self.elapsed / CHARGE_SECONDS).min(1.0);
                self.pool_cue.bind_mut().set_pullback_power(self.power * t);
                if t >= 1.0 {
                    let power = self.power;
                    self.enter(BotState::Idle);
                    self.pool_cue.bind_mut().shoot(power);
                }
            }
        }
    }
}

impl BotPlayer {
    /// `group` is what the bot must pot; `Unidentified` means anything goes.
    pub fn begin_turn(&mut self, group: BallType, difficulty: i64) {
        self.group = group;
        self.difficulty = difficulty;
        self.enter(BotState::Thinking);
    }

    pub fn cancel(&mut self) {
        self.enter(BotState::Idle);
    }

    fn enter(&mut self, state: BotState) {
        self.state = state;
        self.elapsed = 0.0;
    }

    /// (Aim error in degrees, how many of the best shots to pick among.)
    fn skill(&self) -> (f32, i32) {
        match self.difficulty {
            0 => (self.easy_aim_error, 4),
            1 => (self.medium_aim_error, 2),
            _ => (self.hard_aim_error, 1),
        }
    }

    fn choose_shot(&mut self) {
        let (error_degrees, top) = self.skill();
        let shot = self.plan(top);

        let (dir, power) = match shot {
            Some(shot) => (shot.dir, shot.power),
            None => {
                // Never stall: with nothing reachable, any legal shot will do.
                let angle = randf_range(0.0, std::f64::consts::TAU) as f32;
                ([angle.sin(), angle.cos()], BLIND_POWER)
            }
        };

        let error = (randf_range(-error_degrees as f64, error_degrees as f64) as f32).to_radians();
        // The cue points along (-sin yaw, -cos yaw).
        let yaw = (-dir[0]).atan2(-dir[1]) + error;

        self.from_yaw = self.pool_cue.bind().yaw();
        self.to_yaw = yaw;
        self.power = power;
        godot_print!("bot: yaw {:.3} power {:.2} group {}", yaw, power, self.group);
        self.enter(BotState::Rotating);
    }

    fn plan(&self, top: i32) -> Option<Shot> {
        let cue = cue_ball(&self.ball_rack)?;
        let in_play = object_balls(&self.ball_rack);

        let balls: Vec<V2> = in_play.iter().map(|b| to_v2(b.get_global_position())).collect();
        let targets: Vec<V2> = in_play
            .iter()
            .filter(|b| belongs_to(b, self.group))
            .map(|b| to_v2(b.get_global_position()))
            .collect();
        let pocket_list = pockets(&self.pockets);

        let table = Table {
            cue: to_v2(cue.get_global_position()),
            balls: &balls,
            pockets: &pocket_list,
            ball_radius: BALL_RADIUS,
            half_x: HALF_X,
            half_z: HALF_Z,
            damp: cue.get_linear_damp(),
            decel: cue.bind().rolling_decel(),
            full_speed: self.pool_cue.bind().max_impulse() / cue.get_mass() * ROLL_SPEED_FACTOR,
        };

        if table.is_unbroken_rack(FULL_RACK) {
            return table.break_shot();
        }

        let candidates = table.pot_candidates(&targets);
        if candidates.is_empty() {
            return table.fallback(&targets);
        }

        let last = (candidates.len() as i64).min(top as i64) - 1;
        candidates.get(randi_range(0, last) as usize).copied()
    }
}

/// Signed angle from `from` to `to` going the short way round.
fn shortest_turn(from: f32, to: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (to - from + PI).rem_euclid(TAU) - PI
}
