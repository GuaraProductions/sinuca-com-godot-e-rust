use godot::prelude::*;
use godot::classes::Timer;
use godot::classes::Area3D;
use godot::classes::Node3D;
use godot::classes::INode3D;
use godot::classes::{InputEvent, InputEventMouseMotion};
use std::fmt;

use crate::BallRack;
use crate::Ball;

/// Sensitivity of the yaw rotation, in radians per pixel of mouse motion.
const AIM_SENSITIVITY: f32 = 0.005;

/// Sensitivity of the pullback drag, in world units per pixel of mouse
/// motion. The scene stays at the original 30x scale (ball physics behaved
/// worse at the real-world 1x target), so this tracks `max_pullback` instead
/// of meters.
const PULLBACK_SENSITIVITY: f32 = 0.06;

/// Aiming and free to shoot, drawing the cue back, or locked out by the
/// GameManager while a shot is resolving.
#[derive(Default, PartialEq)]
enum CueState {
    #[default]
    Aiming,
    Charging,
    Disabled,
    Launched,
}

impl fmt::Display for CueState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CueState::Aiming => write!(f, "Aiming"),
            CueState::Charging => write!(f, "Charging"),
            CueState::Disabled => write!(f, "Disabled"),
            CueState::Launched => write!(f, "Launched"),
        }
    }
}

#[derive(GodotClass)]
#[class(init, base=Node3D)]
pub struct PoolCue {
    /// `PoolCue` and `BallRack` are siblings under the `Billard` root; resolving
    /// this by path instead of an inspector-wired export means there is no
    /// manual wiring step to forget.
    #[init(node = "../BallRack")]
    ball_rack: OnReady<Gd<BallRack>>,

    /// Ballpark for a hard break: a 0.17 kg ball leaving at ~8 m/s. Impulse is
    /// mass * velocity, so it does not scale with the scene's visual/world
    /// unit size -- only `max_pullback` and the mesh dimensions do.
    #[export]
    #[init(val = 1.4)]
    max_impulse: f32,

    /// How far the cue can be drawn back, in world units (the scene stays at
    /// 30x scale, so this is 30x the 0.3 m the plan assumed at 1x).
    #[export]
    #[init(val = 9.0)]
    max_pullback: f32,

    #[init(node = "Area3D")]
    cue_handler: OnReady<Gd<Area3D>>,
    #[init(node = "Timer")]
    launched_timer: OnReady<Gd<Timer>>,

    /// Resolved once in `ready`; the rack never frees this node afterwards.
    cue_ball: Option<Gd<Ball>>,

    /// Where `cue_handler` sits in the scene with no pullback: the gap between
    /// the cue tip and the ball's center. Captured in `ready` so the pullback
    /// is added on top of it instead of replacing it.
    rest_offset: Vector3,

    state: CueState,
    /// Set by the GameManager while a menu, free-look camera or the bot owns
    /// the cue. Independent of `state` so unlocking never revives a cue that
    /// is mid-shot.
    input_locked: bool,
    yaw: f32,
    pullback: f32,
    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for PoolCue {

    fn ready(&mut self) {
        self.rest_offset = self.cue_handler.get_position();

        let cue_ball = self.ball_rack.get_node_as::<Ball>("CueBall");

        self.snap_to(cue_ball.get_global_position());
        self.cue_ball = Some(cue_ball);

        self.launched_timer.signals().timeout().connect_other(self, Self::after_release);
    }

    /// Follows the cue ball while aiming, so a reset elsewhere in the scene
    /// never leaves the cue pointing at an empty spot on the felt.
    fn physics_process(&mut self, _delta: f64) {

        if matches!(self.state, CueState::Aiming | CueState::Charging) {
            let position = match &self.cue_ball {
                Some(ball) => ball.get_global_position() ,
                None => return,
            };

            self.snap_to(position);
        }
    }

    fn input(&mut self, event: Gd<InputEvent>) {
        if self.state == CueState::Disabled || self.input_locked {
            return;
        }

        if event.is_action_pressed("shoot") {
            self.start_charge();
        } else if event.is_action_released("shoot") {
            self.release_shot();
        }

        if let Ok(motion) = event.try_cast::<InputEventMouseMotion>() {
            self.on_mouse_motion(motion.get_relative());
        }
    }
}

#[godot_api]
impl PoolCue {

    #[signal]
    pub fn shot_taken(power: f32);

    /// Lets the GameManager hide and lock the cue while a shot resolves, and
    /// hand it back once the table settles.
    #[func]
    pub fn set_disabled(&mut self, disabled: bool) {
        self.cancel_launch();
        self.state = if disabled { CueState::Disabled } else { CueState::Aiming };
        self.base_mut().set_visible(!disabled);
    }

    /// Moves the cue to `position`, rebuilding the basis from `yaw` alone. That
    /// keeps the aim (the yaw lives on this node) while discarding any
    /// scale/rotation baked into how the instance was placed in the scene.
    fn snap_to(&mut self, position: Vector3) {
        let basis = Basis::from_axis_angle(Vector3::UP, self.yaw);
        self.base_mut().set_global_transform(Transform3D::new(basis, position));
    }

    fn start_charge(&mut self) {
        if self.state != CueState::Aiming {
            return;
        }

        self.state = CueState::Charging;
        self.pullback = 0.0;
    }

    pub fn reset_player(&mut self) {
        self.cancel_launch();
        self.state = CueState::Aiming;
        self.base_mut().set_visible(true);
        self.pullback = 0.0;
        let rest_offset = self.rest_offset;
        self.cue_handler.set_position(rest_offset);

        let position = match &self.cue_ball {
            Some(ball) => ball.get_global_position() ,
            None => return,
        };

        self.snap_to(position);
    }
    /// Stops a pending `after_release`, which would otherwise disable a cue
    /// that a restart has just handed back to a player.
    fn cancel_launch(&mut self) {
        self.launched_timer.stop();
        self.base_mut().set_physics_process(true);
    }

    /// Applies the impulse along the cue's current aim and snaps it back to
    /// the resting (non-pulled-back) position.
    fn release_shot(&mut self) {
        if self.state != CueState::Charging {
            return;
        }

        let power = self.pullback / self.max_pullback;
        self.shoot(power);
    }

    /// Fires the cue ball with `power` in 0..=1 along the current aim. Used by
    /// both the human release and the bot.
    pub fn shoot(&mut self, power: f32) {
        let power = power.clamp(0.0, 1.0);
        self.pullback = 0.0;
        let rest_offset = self.rest_offset;
        self.cue_handler.set_position(rest_offset);

        if let Some(cue_ball) = self.cue_ball.as_mut() {
            let direction = self.cue_handler.get_global_transform().basis * Vector3::FORWARD;
            cue_ball.apply_central_impulse(direction * power * self.max_impulse);
        }

        self.signals().shot_taken().emit(power);
        self.state = CueState::Launched;
        self.base_mut().set_visible(false);
        self.base_mut().set_physics_process(false);
        self.launched_timer.start();
    }

    /// Locks or unlocks mouse control of the cue (menu, free look, bot turn).
    pub fn set_input_locked(&mut self, locked: bool) {
        self.input_locked = locked;
        if locked && self.state == CueState::Charging {
            // Abort a half-drawn shot instead of leaving the cue pulled back.
            self.state = CueState::Aiming;
            self.pullback = 0.0;
            let rest_offset = self.rest_offset;
            self.cue_handler.set_position(rest_offset);
        }
    }

    pub fn is_charging(&self) -> bool {
        self.state == CueState::Charging
    }

    pub fn charge_power(&self) -> f32 {
        if self.max_pullback > 0.0 { self.pullback / self.max_pullback } else { 0.0 }
    }

    pub fn max_impulse(&self) -> f32 {
        self.max_impulse
    }

    pub fn yaw(&self) -> f32 {
        self.yaw
    }

    /// Turns the cue around the ball to `yaw` (radians around +Y).
    pub fn set_yaw(&mut self, yaw: f32) {
        self.yaw = yaw;
        self.base_mut().set_rotation(Vector3::new(0.0, yaw, 0.0));
    }

    /// Horizontal unit vector the cue ball would be shot along.
    pub fn aim_direction(&self) -> Vector3 {
        Basis::from_axis_angle(Vector3::UP, self.yaw) * Vector3::FORWARD
    }

    /// Pull-back used by the bot's animation; mirrors the mouse drag.
    pub fn set_pullback_power(&mut self, power: f32) {
        let pullback = power.clamp(0.0, 1.0) * self.max_pullback;
        self.pullback = pullback;
        let offset = self.rest_offset + Vector3::new(0.0, 0.0, pullback);
        self.cue_handler.set_position(offset);
    }

    pub fn begin_bot_charge(&mut self) {
        if self.state == CueState::Aiming {
            self.state = CueState::Charging;
            self.pullback = 0.0;
        }
    }

    pub fn is_disabled(&mut self) -> bool {
        return self.state == CueState::Disabled;
    }

    fn after_release(&mut self) {
        self.state = CueState::Disabled;
        self.base_mut().set_physics_process(true);
    }

    /// Yaw while aiming turns the cue around the ball; drag while charging
    /// pulls it back along its own local +Z.
    fn on_mouse_motion(&mut self, relative: Vector2) {
        match self.state {
            CueState::Aiming => {
                let yaw: f32 = self.yaw + relative.x * AIM_SENSITIVITY;
                self.base_mut().set_rotation(Vector3::new(0.0, yaw, 0.0));
                self.yaw = yaw;
            }
            CueState::Charging => {
                let pullback = (self.pullback + relative.y * PULLBACK_SENSITIVITY)
                    .clamp(0.0, self.max_pullback);
                let offset = self.rest_offset + Vector3::new(0.0, 0.0, pullback);
                self.cue_handler.set_position(offset);
                self.pullback = pullback;
            }
            CueState::Disabled => {}
            CueState::Launched => {}
        }
    }
}
