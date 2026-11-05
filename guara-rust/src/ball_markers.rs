use godot::classes::base_material_3d::{BillboardMode, ShadingMode};
use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::label_3d::DrawFlags;
use godot::classes::{INode3D, Label3D, MeshInstance3D, Node3D, StandardMaterial3D, TorusMesh};
use godot::prelude::*;

use crate::ball::{Ball, BallType};
use crate::ball_rack::BallRack;
use crate::table_query::{BALL_RADIUS, FELT_Y};

const LABEL_HEIGHT: f32 = BALL_RADIUS + 0.9;
const RING_HEIGHT: f32 = FELT_Y + 0.08;
const RING_COLOR: Color = Color::from_rgb(1.0, 0.82, 0.2);
const ODD_COLOR: Color = Color::from_rgb(1.0, 0.72, 0.35);
const EVEN_COLOR: Color = Color::from_rgb(0.55, 0.8, 1.0);

struct Marker {
    ball: Gd<Ball>,
    label: Gd<Label3D>,
    ring: Gd<MeshInstance3D>,
}

/// A floating number per ball and a ring under the ones the current player must
/// pot. The balls share 8 colours between 15 numbers and the groups are odd and
/// even, so nothing on the ball itself says whose it is.
///
/// These nodes follow the balls instead of being their children: a ball is
/// scaled 30x and spins as it rolls, which would take any child with it.
#[derive(GodotClass)]
#[class(init, base=Node3D)]
pub struct BallMarkers {
    base: Base<Node3D>,

    #[init(node = "../BallRack")]
    ball_rack: OnReady<Gd<BallRack>>,

    markers: Vec<Marker>,
    highlight: BallType,
    /// The player's choice, kept across matches. Rings do not depend on it.
    numbers_visible: bool,
}

#[godot_api]
impl INode3D for BallMarkers {
    fn ready(&mut self) {
        let balls = self.ball_rack.bind().get_balls();

        let mut ring_material = StandardMaterial3D::new_gd();
        ring_material.set_shading_mode(ShadingMode::UNSHADED);
        ring_material.set_albedo(RING_COLOR);

        let mut ring_mesh = TorusMesh::new_gd();
        ring_mesh.set_inner_radius(BALL_RADIUS + 0.1);
        ring_mesh.set_outer_radius(BALL_RADIUS + 0.3);

        for ball in balls.iter_shared() {
            let number = ball.bind().get_number();
            if number == 0 {
                continue;
            }

            let mut label = Label3D::new_alloc();
            label.set_text(&number.to_string());
            label.set_billboard_mode(BillboardMode::ENABLED);
            label.set_draw_flag(DrawFlags::DISABLE_DEPTH_TEST, true);
            label.set_font_size(48);
            label.set_outline_size(14);
            label.set_pixel_size(0.035);
            label.set_modulate(if number % 2 == 0 { EVEN_COLOR } else { ODD_COLOR });
            label.set_outline_modulate(Color::from_rgb(0.05, 0.05, 0.05));
            self.base_mut().add_child(&label);

            let mut ring = MeshInstance3D::new_alloc();
            ring.set_mesh(&ring_mesh);
            ring.set_material_override(&ring_material);
            ring.set_cast_shadows_setting(ShadowCastingSetting::OFF);
            ring.set_visible(false);
            self.base_mut().add_child(&ring);

            self.markers.push(Marker { ball, label, ring });
        }
    }

    fn process(&mut self, _delta: f64) {
        let highlight = self.highlight;
        let numbers_visible = self.numbers_visible;
        for marker in &mut self.markers {
            let position = marker.ball.get_global_position();
            let (potted, own) = {
                let ball = marker.ball.bind();
                (
                    ball.is_potted(),
                    highlight != BallType::Unidentified && ball.get_enumeration() == highlight,
                )
            };
            let in_play = !potted && position.y > FELT_Y - 1.0;

            marker.label.set_visible(in_play && numbers_visible);
            marker.ring.set_visible(in_play && own);
            if in_play {
                marker.label.set_global_position(position + Vector3::new(0.0, LABEL_HEIGHT, 0.0));
                marker.ring.set_global_position(Vector3::new(position.x, RING_HEIGHT, position.z));
            }
        }
    }
}

impl BallMarkers {
    pub fn toggle_numbers(&mut self) {
        self.numbers_visible = !self.numbers_visible;
    }

    /// Rings go under balls of `group`; `Unidentified` shows numbers only.
    pub fn set_highlight(&mut self, group: BallType) {
        self.highlight = group;
    }
}
