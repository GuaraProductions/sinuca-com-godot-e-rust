use godot::classes::IRigidBody3D;
use godot::classes::PhysicsDirectBodyState3D;
use godot::classes::RigidBody3D;
use godot::prelude::*;
use std::fmt;

use crate::ball::BallType::Unidentified;

#[derive(Default, PartialEq, Clone, Copy, Debug)]
pub enum BallType {
    Odd,
    Even,
    #[default]
    Unidentified,
}

impl fmt::Display for BallType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BallType::Odd => write!(f, "Odd"),
            BallType::Even => write!(f, "Even"),
            BallType::Unidentified => write!(f, "Unidentified"),
        }
    }
}

/// A single pool ball. Owns its spawn state, so a reset never has to consult
/// the rack or the scene file.
///
/// `pot()` and `reset()` are exact mirrors of each other: whatever one takes
/// away, the other restores. Neither may be called from a physics signal
/// callback -- both touch `freeze` and the collision layers, which the physics
/// server is still iterating over at that point.
#[derive(GodotClass)]
#[class(tool, base=RigidBody3D)]
pub struct Ball {
    /// `0` = cue ball, `1..=15` = numbered. Assigned by the `BallRack` from the
    /// source mesh name.
    #[export]
    #[var(pub)]
    number: i32,
    enumeration: BallType,

    /// Constant rolling deceleration, in real-world m/s^2 (scaled to the scene
    /// in `integrate_forces`). The body's `linear_damp` alone is exponential,
    /// so a ball would crawl along for seconds at the end of every roll; this
    /// gives it the near-linear stop of real cloth.
    #[export]
    rolling_resistance: f32,

    /// Spawn state, captured in `ready`. The single source of truth for resets.
    home: Transform3D,
    home_layer: u32,
    home_mask: u32,

    /// World units per real-world meter, read from the spawn transform. The
    /// rack is instanced at 30x, and every real-world threshold below has to be
    /// scaled by it.
    world_scale: f32,

    /// Guards against scoring twice when two pocket areas overlap.
    potted: bool,

    /// `reset` was called and the physics server still has to apply it.
    teleport_pending: bool,

    base: Base<RigidBody3D>,
}

/// Rest thresholds, in real-world units (a ball is 2.8 cm across). The linear
/// one is scaled by `world_scale` before use; the angular one is in rad/s,
/// which no scale changes.
///
/// The angular limit is the linear one converted through `w = v / r`: a ball
/// rolling without slipping at 2 cm/s spins at roughly 0.7 rad/s.
const LINEAR_REST_SPEED_SQ: f32 = 0.02 * 0.02;
const ANGULAR_REST_SPEED_SQ: f32 = 0.7 * 0.7;

#[godot_api]
impl IRigidBody3D for Ball {
    fn init(base: Base<RigidBody3D>) -> Self {
        Self {
            base,
            number: 0,
            rolling_resistance: 0.1,
            world_scale: 1.0,
            potted: false,
            teleport_pending: false,
            home: Transform3D::IDENTITY,
            home_layer: 0,
            home_mask: 0,
            enumeration: Unidentified,
        }
    }

    fn ready(&mut self) {
        let (home, home_layer, home_mask) = {
            let base = self.base();
            (
                base.get_global_transform(),
                base.get_collision_layer(),
                base.get_collision_mask(),
            )
        };

        self.home = home;
        self.home_layer = home_layer;
        self.home_mask = home_mask;
        self.world_scale = home.basis.get_scale().x;

        //godot_print!("Ball {} -> number {}", self.base().get_name(), self.number);
    }

    /// Slows the rolling (horizontal) motion by a constant amount per step,
    /// scaling the spin by the same factor so table friction does not turn the
    /// leftover spin back into speed. Vertical velocity is left alone, so a
    /// ball crawling over a pocket lip still falls in.
    fn integrate_forces(&mut self, state: Option<Gd<PhysicsDirectBodyState3D>>) {
        let Some(mut state) = state else {
            return;
        };

        // A transform written from outside is overwritten when the body is
        // moving, so a restart mid-shot has to teleport from in here.
        if self.teleport_pending {
            self.teleport_pending = false;
            state.set_transform(self.home);
            state.set_linear_velocity(Vector3::ZERO);
            state.set_angular_velocity(Vector3::ZERO);
            return;
        }

        if self.potted {
            return;
        }

        let velocity = state.get_linear_velocity();
        let speed = Vector3::new(velocity.x, 0.0, velocity.z).length();
        if speed <= 0.0 {
            return;
        }

        let decel = self.rolling_resistance * self.world_scale * state.get_step();
        let factor = (speed - decel).max(0.0) / speed;

        state.set_linear_velocity(Vector3::new(
            velocity.x * factor,
            velocity.y,
            velocity.z * factor,
        ));
        let angular = state.get_angular_velocity();
        state.set_angular_velocity(angular * factor);
    }
}

#[godot_api]
impl Ball {
    /// Takes the ball out of play: no motion, no collisions, not drawn. The
    /// node stays in the tree so the rack keeps holding a valid `Gd<Ball>`.
    #[func]
    pub fn pot(&mut self) {
        if self.potted {
            return;
        }
        self.potted = true;

        let mut base = self.base_mut();
        base.set_freeze_enabled(true);
        base.set_linear_velocity(Vector3::ZERO);
        base.set_angular_velocity(Vector3::ZERO);
        base.set_collision_layer(0);
        base.set_collision_mask(0);
        base.set_visible(false);
    }

    /// Returns the ball to its spawn state.
    ///
    /// The freeze around the teleport is not optional: the physics server
    /// routinely ignores a transform written to a body it is still simulating.
    #[func]
    pub fn reset(&mut self) {
        self.potted = false;
        self.teleport_pending = true;

        let (home, home_layer, home_mask) = (self.home, self.home_layer, self.home_mask);

        let mut base = self.base_mut();
        base.set_freeze_enabled(true);
        base.set_global_transform(home);
        base.set_linear_velocity(Vector3::ZERO);
        base.set_angular_velocity(Vector3::ZERO);
        base.set_collision_layer(home_layer);
        base.set_collision_mask(home_mask);
        base.set_visible(true);
        base.set_freeze_enabled(false);
        // `integrate_forces` does not run for a sleeping body.
        base.set_sleeping(false);
    }

    /// Half of the "table has settled" test. A potted ball counts as at rest so
    /// it cannot hold the turn open forever.
    #[func]
    pub fn is_at_rest(&self) -> bool {
        if self.potted {
            return true;
        }

        let base = self.base();
        let linear_limit_sq = LINEAR_REST_SPEED_SQ * self.world_scale * self.world_scale;

        base.is_sleeping()
            || (base.get_linear_velocity().length_squared() < linear_limit_sq
                && base.get_angular_velocity().length_squared() < ANGULAR_REST_SPEED_SQ)
    }
}

impl Ball {
    pub fn is_potted(&self) -> bool {
        self.potted
    }

    /// Constant rolling deceleration in world units per second squared.
    pub fn rolling_decel(&self) -> f32 {
        self.rolling_resistance * self.world_scale
    }

    pub fn get_enumeration(&self) -> BallType {
        self.enumeration
    }

    pub fn set_enumeration(&mut self, number: i32) {
        self.enumeration = if number % 2 == 0 {
            BallType::Even
        } else {
            BallType::Odd
        }
    }
}
