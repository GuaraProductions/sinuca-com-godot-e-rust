//! What the preview and the bot need to know about the live table: where the
//! balls and pockets are, and where a rolling ball would meet a cushion.

use godot::classes::{
    Node3D, PhysicsDirectSpaceState3D, PhysicsShapeQueryParameters3D, SphereShape3D,
};
use godot::prelude::*;

use crate::aim_model::{V2, normalize};
use crate::ball::{Ball, BallType};
use crate::ball_rack::BallRack;
use crate::pocket::Pocket;
use crate::shot_planner::PocketInfo;

/// Ball radius in world units: 0.028 m at the scene's 30x scale.
pub const BALL_RADIUS: f32 = 0.84;
/// Inner rail faces, from Rail_*-col in billard-table.glb at 30x.
pub const HALF_X: f32 = 16.8;
pub const HALF_Z: f32 = 33.6;
/// Top of the felt collider.
pub const FELT_Y: f32 = 22.5;
/// Matches `capture_radius` in gravitational_pull.gd.
pub const POCKET_CAPTURE_RADIUS: f32 = 1.6;

/// Physical table colliders live on this layer; balls are on layer 2.
const TABLE_LAYER: u32 = 1;
/// A touch under the real radius and lifted off the felt, so a cast along the
/// table does not count the felt the ball is resting on as a hit.
const CAST_RADIUS: f32 = 0.82;
const CAST_LIFT: f32 = 0.08;
/// How far to step forward when a cast starts in contact with a rail.
const UNSTICK_STEP: f32 = 0.3;

pub fn to_v2(p: Vector3) -> V2 {
    [p.x, p.z]
}

/// A ball knocked off the table is out of reach, so nothing should plan on it.
fn on_the_table(ball: &Gd<Ball>) -> bool {
    ball.get_global_position().y > FELT_Y - 1.0
}

/// Object balls still in play.
pub fn object_balls(rack: &Gd<BallRack>) -> Vec<Gd<Ball>> {
    rack.bind()
        .get_balls()
        .iter_shared()
        .filter(|ball| {
            let bound = ball.bind();
            bound.get_number() != 0 && !bound.is_potted()
        })
        .filter(on_the_table)
        .collect()
}

pub fn cue_ball(rack: &Gd<BallRack>) -> Option<Gd<Ball>> {
    rack.bind().get_balls().iter_shared().find(|ball| ball.bind().get_number() == 0)
}

pub fn belongs_to(ball: &Gd<Ball>, group: BallType) -> bool {
    group == BallType::Unidentified || ball.bind().get_enumeration() == group
}

pub fn pockets(parent: &Gd<Node3D>) -> Vec<PocketInfo> {
    parent
        .get_children()
        .iter_shared()
        .filter_map(|child| child.try_cast::<Pocket>().ok())
        .map(|pocket| {
            let pos = to_v2(pocket.get_global_position());
            PocketInfo { pos, side: pos[1].abs() < 1.0 }
        })
        .collect()
}

/// Sphere casts against the real table colliders. Only valid during a physics
/// frame, because the direct space state is.
pub struct CushionCaster {
    space: Gd<PhysicsDirectSpaceState3D>,
    params: Gd<PhysicsShapeQueryParameters3D>,
    height: f32,
}

impl CushionCaster {
    pub fn new(space: Gd<PhysicsDirectSpaceState3D>, ball_height: f32) -> Self {
        let mut shape = SphereShape3D::new_gd();
        shape.set_radius(CAST_RADIUS);

        let mut params = PhysicsShapeQueryParameters3D::new_gd();
        params.set_shape(&shape);
        params.set_collision_mask(TABLE_LAYER);
        params.set_collide_with_areas(false);
        params.set_collide_with_bodies(true);

        Self { space, params, height: ball_height + CAST_LIFT }
    }

    fn at(&self, p: V2) -> Transform3D {
        Transform3D::new(Basis::IDENTITY, Vector3::new(p[0], self.height, p[1]))
    }

    /// Inward-facing horizontal normal of the surface met at `p`. Falls back to
    /// the nearest rail when the contact has no usable horizontal part.
    fn normal_at(&mut self, p: V2, dir: V2) -> V2 {
        let transform = self.at(p);
        self.params.set_transform(transform);
        self.params.set_motion(Vector3::ZERO);
        let info = self.space.get_rest_info(&self.params);

        let contact = info
            .get("normal")
            .and_then(|value| value.try_to::<Vector3>().ok())
            .map(|n| [n.x, n.z])
            .filter(|n| n[0] * n[0] + n[1] * n[1] > 0.01);

        let normal = contact.map(normalize).unwrap_or_else(|| {
            if p[0].abs() / HALF_X > p[1].abs() / HALF_Z {
                [-p[0].signum(), 0.0]
            } else {
                [0.0, -p[1].signum()]
            }
        });

        // A normal must oppose the motion, whichever way the query reported it.
        if normal[0] * dir[0] + normal[1] * dir[1] > 0.0 {
            [-normal[0], -normal[1]]
        } else {
            normal
        }
    }

    fn cast_once(&mut self, from: V2, dir: V2, max: f32) -> Option<f32> {
        let transform = self.at(from);
        self.params.set_transform(transform);
        self.params.set_motion(Vector3::new(dir[0] * max, 0.0, dir[1] * max));
        let result = self.space.cast_motion(&self.params);
        let safe = result.get(0).unwrap_or(1.0);
        (safe < 1.0).then_some(safe * max)
    }

    /// Distance and normal of the first cushion on the way, if any.
    pub fn cast(&mut self, from: V2, dir: V2, max: f32) -> Option<(f32, V2)> {
        let mut start = from;
        let mut travelled = 0.0;
        let mut hit = self.cast_once(start, dir, max);

        // Starting against a rail reports an immediate hit even when moving
        // away from it, so step clear and look again.
        if hit.is_some_and(|d| d < 0.001) && max > UNSTICK_STEP {
            travelled = UNSTICK_STEP;
            start = [from[0] + dir[0] * UNSTICK_STEP, from[1] + dir[1] * UNSTICK_STEP];
            hit = self.cast_once(start, dir, max - UNSTICK_STEP);
        }

        let distance = hit?;
        let point = [start[0] + dir[0] * distance, start[1] + dir[1] * distance];
        // Probe just past the safe point, where the sphere actually touches.
        let probe = [point[0] + dir[0] * 0.05, point[1] + dir[1] * 0.05];
        Some((travelled + distance, self.normal_at(probe, dir)))
    }
}
