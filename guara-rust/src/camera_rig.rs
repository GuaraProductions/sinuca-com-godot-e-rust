use godot::classes::Camera3D;
use godot::classes::ICamera3D;
use godot::classes::Input;
use godot::classes::InputEvent;
use godot::classes::InputEventMouseButton;
use godot::classes::InputEventMouseMotion;
use godot::global::{Key, MouseButton};
use godot::prelude::*;

/// Middle of the felt, in world units (the scene is at 30x scale).
const TABLE_CENTER: Vector3 = Vector3::new(0.0, 22.5, 0.0);

const ORBIT_RADIUS: f32 = 70.0;
const ORBIT_HEIGHT: f32 = 45.0;
const ORBIT_SPEED: f32 = 0.15;

const BLEND_SECONDS: f32 = 1.0;

const LOOK_SENSITIVITY: f32 = 0.005;
const PITCH_MIN: f32 = 10.0 * std::f32::consts::PI / 180.0;
const PITCH_MAX: f32 = 85.0 * std::f32::consts::PI / 180.0;
const DISTANCE_MIN: f32 = 15.0;
const DISTANCE_MAX: f32 = 75.0;
const ZOOM_STEP: f32 = 3.0;
const PAN_SPEED: f32 = 30.0;
/// Camera and focus limits keep the view inside the room and over the table.
const FOCUS_LIMIT_X: f32 = 17.0;
const FOCUS_LIMIT_Z: f32 = 34.0;
const CAMERA_LIMIT_X: f32 = 80.0;
const CAMERA_LIMIT_Z: f32 = 120.0;
const CAMERA_MIN_Y: f32 = 3.0;

#[derive(Default, PartialEq, Clone, Copy)]
enum RigMode {
    Menu,
    #[default]
    Table,
    FreeLook,
}

/// The scene camera. Orbits the table behind the menu, sits at the original
/// authored pose during play, and lets the player look around on their turn.
#[derive(GodotClass)]
#[class(init, base=Camera3D)]
pub struct CameraRig {
    base: Base<Camera3D>,

    mode: RigMode,
    /// The pose authored in Main.tscn, captured in `ready`.
    table_pose: Transform3D,
    orbit_angle: f32,

    blend_from: Transform3D,
    blend_t: f32,

    focus: Vector3,
    yaw: f32,
    pitch: f32,
    distance: f32,
}

#[godot_api]
impl ICamera3D for CameraRig {
    fn ready(&mut self) {
        self.table_pose = self.base().get_global_transform();
        self.blend_t = 1.0;
    }

    fn process(&mut self, delta: f64) {
        let delta = delta as f32;

        match self.mode {
            RigMode::Menu => self.orbit_angle += ORBIT_SPEED * delta,
            RigMode::FreeLook => self.pan(delta),
            RigMode::Table => {}
        }

        let target = self.target_transform();
        let pose = if self.blend_t < 1.0 {
            self.blend_t = (self.blend_t + delta / BLEND_SECONDS).min(1.0);
            let t = self.blend_t * self.blend_t * (3.0 - 2.0 * self.blend_t);
            self.blend_from.interpolate_with(&target, t)
        } else {
            target
        };
        self.base_mut().set_global_transform(pose);
    }

    fn input(&mut self, event: Gd<InputEvent>) {
        if self.mode != RigMode::FreeLook {
            return;
        }

        if let Ok(motion) = event.clone().try_cast::<InputEventMouseMotion>() {
            let relative = motion.get_relative();
            self.yaw -= relative.x * LOOK_SENSITIVITY;
            self.pitch = (self.pitch + relative.y * LOOK_SENSITIVITY).clamp(PITCH_MIN, PITCH_MAX);
        } else if let Ok(button) = event.try_cast::<InputEventMouseButton>() {
            if !button.is_pressed() {
                return;
            }
            match button.get_button_index() {
                MouseButton::WHEEL_UP => {
                    self.distance = (self.distance - ZOOM_STEP).clamp(DISTANCE_MIN, DISTANCE_MAX)
                }
                MouseButton::WHEEL_DOWN => {
                    self.distance = (self.distance + ZOOM_STEP).clamp(DISTANCE_MIN, DISTANCE_MAX)
                }
                _ => {}
            }
        }
    }
}

impl CameraRig {
    fn start_blend(&mut self, mode: RigMode) {
        self.blend_from = self.base().get_global_transform();
        self.blend_t = 0.0;
        self.mode = mode;
    }

    pub fn enter_menu(&mut self) {
        if self.mode != RigMode::Menu {
            let from = self.base().get_global_transform().origin - TABLE_CENTER;
            self.orbit_angle = from.x.atan2(from.z);
            self.start_blend(RigMode::Menu);
        }
    }

    pub fn enter_table(&mut self) {
        if self.mode != RigMode::Table {
            self.start_blend(RigMode::Table);
        }
    }

    /// Starts free look from wherever the camera currently is, so nothing jumps.
    pub fn enter_free_look(&mut self) {
        if self.mode == RigMode::FreeLook {
            return;
        }
        self.focus = Vector3::new(0.0, TABLE_CENTER.y, 0.0);
        let offset = self.base().get_global_transform().origin - self.focus;
        self.distance = offset.length().clamp(DISTANCE_MIN, DISTANCE_MAX);
        self.pitch = (offset.y / offset.length().max(0.001))
            .asin()
            .clamp(PITCH_MIN, PITCH_MAX);
        self.yaw = offset.x.atan2(offset.z);
        self.start_blend(RigMode::FreeLook);
    }

    pub fn is_free_look(&self) -> bool {
        self.mode == RigMode::FreeLook
    }

    fn pan(&mut self, delta: f32) {
        let input = Input::singleton();
        let forward = Vector3::new(-self.yaw.sin(), 0.0, -self.yaw.cos());
        let right = Vector3::new(forward.z * -1.0, 0.0, forward.x);
        let mut dir = Vector3::ZERO;
        if input.is_key_pressed(Key::W) {
            dir += forward;
        }
        if input.is_key_pressed(Key::S) {
            dir -= forward;
        }
        if input.is_key_pressed(Key::D) {
            dir += right;
        }
        if input.is_key_pressed(Key::A) {
            dir -= right;
        }
        if dir != Vector3::ZERO {
            self.focus += dir.normalized() * PAN_SPEED * delta;
            self.focus.x = self.focus.x.clamp(-FOCUS_LIMIT_X, FOCUS_LIMIT_X);
            self.focus.z = self.focus.z.clamp(-FOCUS_LIMIT_Z, FOCUS_LIMIT_Z);
        }
    }

    fn look_at(position: Vector3, target: Vector3) -> Transform3D {
        let basis = Basis::looking_at(target - position);
        Transform3D::new(basis, position)
    }

    fn target_transform(&self) -> Transform3D {
        match self.mode {
            RigMode::Table => self.table_pose,
            RigMode::Menu => {
                let position = TABLE_CENTER
                    + Vector3::new(
                        self.orbit_angle.sin() * ORBIT_RADIUS,
                        ORBIT_HEIGHT - TABLE_CENTER.y,
                        self.orbit_angle.cos() * ORBIT_RADIUS,
                    );
                Self::look_at(position, TABLE_CENTER)
            }
            RigMode::FreeLook => {
                let dir = Vector3::new(
                    self.yaw.sin() * self.pitch.cos(),
                    self.pitch.sin(),
                    self.yaw.cos() * self.pitch.cos(),
                );
                let mut position = self.focus + dir * self.distance;
                position.x = position.x.clamp(-CAMERA_LIMIT_X, CAMERA_LIMIT_X);
                position.z = position.z.clamp(-CAMERA_LIMIT_Z, CAMERA_LIMIT_Z);
                position.y = position.y.max(CAMERA_MIN_Y);
                Self::look_at(position, self.focus)
            }
        }
    }
}
