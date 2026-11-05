use godot::classes::base_material_3d::{CullMode, Flags, ShadingMode, Transparency};
use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::mesh::PrimitiveType;
use godot::classes::{
    ImmediateMesh, INode3D, MeshInstance3D, Node3D, SphereMesh, StandardMaterial3D,
};
use godot::prelude::*;

use crate::aim_model::{ROLL_SPEED_FACTOR, V2, normalize, stopping_distance, sub, trace_shot};
use crate::ball::Ball;
use crate::ball_rack::BallRack;
use crate::pool_cue::PoolCue;
use crate::table_query::{
    BALL_RADIUS, CushionCaster, FELT_Y, HALF_X, HALF_Z, POCKET_CAPTURE_RADIUS, cue_ball,
    object_balls, pockets, to_v2,
};

const RIBBON_HALF_WIDTH: f32 = 0.12;
const RIBBON_HEIGHT: f32 = FELT_Y + 0.06;
/// Length of the object ball's line on a dead-on hit.
const OBJECT_LINE: f32 = 14.0;
/// Length of the cue ball's line after a contact, at a 90 degree split.
const CUE_LINE: f32 = 8.0;

const PATH_COLOR: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.8);
const OBJECT_COLOR: Color = Color::from_rgba(1.0, 0.82, 0.25, 0.9);
const CUE_AFTER_COLOR: Color = Color::from_rgba(0.45, 0.8, 1.0, 0.8);

/// Draws where the cue ball is going while a human aims: its path with cushion
/// bounces, a ghost ball at first contact, and where both balls head next.
#[derive(GodotClass)]
#[class(init, base=Node3D)]
pub struct TrajectoryPreview {
    base: Base<Node3D>,

    #[init(node = "../PoolCue")]
    pool_cue: OnReady<Gd<PoolCue>>,
    #[init(node = "../BallRack")]
    ball_rack: OnReady<Gd<BallRack>>,
    #[init(node = "../Pockets")]
    pockets: OnReady<Gd<Node3D>>,

    active: bool,
    mesh: Option<Gd<ImmediateMesh>>,
    ghost: Option<Gd<MeshInstance3D>>,
}

#[godot_api]
impl INode3D for TrajectoryPreview {
    fn ready(&mut self) {
        let mesh = ImmediateMesh::new_gd();

        let mut material = StandardMaterial3D::new_gd();
        material.set_shading_mode(ShadingMode::UNSHADED);
        material.set_flag(Flags::ALBEDO_FROM_VERTEX_COLOR, true);
        material.set_transparency(Transparency::ALPHA);
        material.set_cull_mode(CullMode::DISABLED);

        let mut ribbon = MeshInstance3D::new_alloc();
        ribbon.set_mesh(&mesh);
        ribbon.set_material_override(&material);
        ribbon.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        self.base_mut().add_child(&ribbon);

        let mut sphere = SphereMesh::new_gd();
        sphere.set_radius(BALL_RADIUS);
        sphere.set_height(BALL_RADIUS * 2.0);
        let mut ghost_material = StandardMaterial3D::new_gd();
        ghost_material.set_shading_mode(ShadingMode::UNSHADED);
        ghost_material.set_transparency(Transparency::ALPHA);
        ghost_material.set_albedo(Color::from_rgba(1.0, 1.0, 1.0, 0.35));

        let mut ghost = MeshInstance3D::new_alloc();
        ghost.set_mesh(&sphere);
        ghost.set_material_override(&ghost_material);
        ghost.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        ghost.set_visible(false);
        self.base_mut().add_child(&ghost);

        self.mesh = Some(mesh);
        self.ghost = Some(ghost);
    }

    /// Runs on the physics tick because the cushion casts need the direct
    /// space state, which is only valid there.
    fn physics_process(&mut self, _delta: f64) {
        if self.active {
            self.redraw();
        }
    }
}

impl TrajectoryPreview {
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
        self.base_mut().set_visible(active);
        if !active {
            if let Some(mesh) = self.mesh.as_mut() {
                mesh.clear_surfaces();
            }
        }
    }

    fn redraw(&mut self) {
        let Some(cue) = cue_ball(&self.ball_rack) else {
            return;
        };
        let Some(world) = self.base().get_world_3d() else {
            return;
        };
        let Some(space) = world.get_direct_space_state() else {
            return;
        };

        let (dir, charging, power, max_impulse) = {
            let pool_cue = self.pool_cue.bind();
            (
                pool_cue.aim_direction(),
                pool_cue.is_charging(),
                pool_cue.charge_power(),
                pool_cue.max_impulse(),
            )
        };

        let budget = travel_budget(&cue, if charging { power } else { 1.0 }, max_impulse);
        let origin = cue.get_global_position();
        let balls: Vec<V2> = object_balls(&self.ball_rack)
            .iter()
            .map(|ball| to_v2(ball.get_global_position()))
            .collect();
        let pocket_points: Vec<V2> = pockets(&self.pockets).iter().map(|p| p.pos).collect();

        let mut caster = CushionCaster::new(space, origin.y);
        let trace = trace_shot(
            to_v2(origin),
            normalize([dir.x, dir.z]),
            budget,
            &balls,
            &pocket_points,
            BALL_RADIUS,
            POCKET_CAPTURE_RADIUS,
            |from, along, max| caster.cast(from, along, max),
        );

        let mut segments: Vec<(V2, V2, Color)> = trace
            .points
            .windows(2)
            .map(|pair| (pair[0], pair[1], PATH_COLOR))
            .collect();

        if let (Some(contact), Some(object)) = (trace.contact, trace.object) {
            let reach = OBJECT_LINE * contact.cut_cos;
            segments.push((object, end_on_table(object, contact.object_dir, reach), OBJECT_COLOR));
            let sin = (1.0 - contact.cut_cos * contact.cut_cos).max(0.0).sqrt();
            let cue_reach = CUE_LINE * sin;
            segments.push((
                contact.ghost,
                end_on_table(contact.ghost, contact.cue_dir, cue_reach),
                CUE_AFTER_COLOR,
            ));
        }

        if let Some(ghost) = self.ghost.as_mut() {
            match trace.contact {
                Some(contact) => {
                    ghost.set_global_position(Vector3::new(contact.ghost[0], origin.y, contact.ghost[1]));
                    ghost.set_visible(true);
                }
                None => ghost.set_visible(false),
            }
        }

        if let Some(mesh) = self.mesh.as_mut() {
            draw_ribbons(mesh, &segments);
        }
    }
}

/// Where a line of length `reach` ends, stopped at the rails so it never pokes
/// out past the cushion.
fn end_on_table(from: V2, dir: V2, reach: f32) -> V2 {
    let limit = |position: f32, along: f32, half: f32| {
        if along.abs() <= f32::EPSILON {
            f32::INFINITY
        } else {
            ((half * along.signum() - position) / along).max(0.0)
        }
    };
    let length = reach
        .min(limit(from[0], dir[0], HALF_X - BALL_RADIUS))
        .min(limit(from[1], dir[1], HALF_Z - BALL_RADIUS));
    [from[0] + dir[0] * length, from[1] + dir[1] * length]
}

/// How far the cue ball rolls at `power`, from the ball's own damping settings.
fn travel_budget(cue: &Gd<Ball>, power: f32, max_impulse: f32) -> f32 {
    let speed = power * max_impulse / cue.get_mass() * ROLL_SPEED_FACTOR;
    stopping_distance(speed, cue.get_linear_damp(), cue.bind().rolling_decel())
}

fn draw_ribbons(mesh: &mut Gd<ImmediateMesh>, segments: &[(V2, V2, Color)]) {
    mesh.clear_surfaces();

    let drawable: Vec<_> = segments
        .iter()
        .filter(|(a, b, _)| {
            let d = sub(*b, *a);
            d[0] * d[0] + d[1] * d[1] > 0.0001
        })
        .collect();
    if drawable.is_empty() {
        return;
    }

    mesh.surface_begin(PrimitiveType::TRIANGLES);
    for (a, b, color) in drawable {
        let along = normalize(sub(*b, *a));
        let side = [-along[1] * RIBBON_HALF_WIDTH, along[0] * RIBBON_HALF_WIDTH];
        let corner = |p: &V2, sign: f32| {
            Vector3::new(p[0] + side[0] * sign, RIBBON_HEIGHT, p[1] + side[1] * sign)
        };
        let quad = [
            corner(a, 1.0),
            corner(a, -1.0),
            corner(b, 1.0),
            corner(b, 1.0),
            corner(a, -1.0),
            corner(b, -1.0),
        ];
        for vertex in quad {
            mesh.surface_set_color(*color);
            mesh.surface_add_vertex(vertex);
        }
    }
    mesh.surface_end();
}
