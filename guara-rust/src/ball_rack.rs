use godot::classes::INode3D;
use godot::classes::Mesh;
use godot::classes::MeshInstance3D;
use godot::classes::Node3D;
use godot::prelude::*;

use crate::ball::Ball;

/// Spawns one `Ball.tscn` per MeshInstance3D found in the imported `balls.blend`,
/// handing each instance the mesh (and starting transform) of its source node.
///
/// Runs as a tool script, so the rack is built in the editor as well as at
/// runtime. The spawned balls are given no owner, which keeps them out of the
/// scene dock and out of the saved `.tscn`.
#[derive(GodotClass)]
#[class(tool, init, base=Node3D)]
pub struct BallRack {
    base: Base<Node3D>,

    /// Scene instanced once per ball. Its root must be a `Ball` with a
    /// MeshInstance3D child named `MeshInstance3D`.
    #[export(file = "*.tscn")]
    #[var(set = set_ball_scene)]
    #[init(val = "res://src/Ball.tscn".into())]
    ball_scene: GString,

    /// Imported model supplying the meshes and the rack layout.
    #[export(file)]
    #[var(set = set_ball_source)]
    #[init(val = "res://assets/balls.blend".into())]
    ball_source: GString,

    /// Inspector checkbox that rebuilds the rack and unticks itself again, to
    /// pick up a re-imported source model without touching the exported paths.
    ///
    /// A `#[export_tool_button]` would read nicer, but its Callable captures the
    /// instance it was read from -- and Godot reads it once on a throwaway
    /// instance while caching property defaults, so pressing the button can end
    /// up on a freed instance. A setter always runs on the edited node.
    #[export]
    #[var(set = set_rebuild, usage_flags = [EDITOR])]
    rebuild: bool,

    /// Balls spawned by the last build, discarded before the next one.
    balls: Vec<Gd<Ball>>,
}

#[godot_api]
impl INode3D for BallRack {
    fn ready(&mut self) {
        self.rebuild();
    }
}

#[godot_api]
impl BallRack {
    #[func]
    fn set_ball_scene(&mut self, path: GString) {
        self.ball_scene = path;
        self.rebuild_if_ready();
    }

    #[func]
    fn set_ball_source(&mut self, path: GString) {
        self.ball_source = path;
        self.rebuild_if_ready();
    }

    #[func]
    fn set_rebuild(&mut self, _ticked: bool) {
        self.rebuild = false;
        self.rebuild_if_ready();
    }

    /// Property setters also run while the scene is still loading; the first
    /// build is left to `ready`.
    fn rebuild_if_ready(&mut self) {
        if self.base().is_node_ready() {
            self.rebuild();
        }
    }

    fn rebuild(&mut self) {
        self.clear();

        let ball_scene = match try_load::<PackedScene>(&self.ball_scene) {
            Ok(scene) => scene,
            Err(error) => {
                godot_error!(
                    "BallRack: could not load ball scene {}: {}",
                    self.ball_scene,
                    error
                );
                return;
            }
        };

        let source_scene = match try_load::<PackedScene>(&self.ball_source) {
            Ok(scene) => scene,
            Err(error) => {
                godot_error!(
                    "BallRack: could not load ball source {}: {}",
                    self.ball_source,
                    error
                );
                return;
            }
        };

        let source = source_scene
            .instantiate()
            .expect("ball source has no root node");

        for child in source.get_children().iter_shared() {
            let Ok(source_mesh) = child.try_cast::<MeshInstance3D>() else {
                continue;
            };

            let Some(mesh) = source_mesh.get_mesh() else {
                continue;
            };

            let name = source_mesh.get_name().to_string();

            let Some(number) = Self::ball_number_from_name(&name) else {
                godot_warn!("BallRack: skipping '{name}', its name identifies no ball");
                continue;
            };

            // `break`, not `return`: the failure is the same for every ball, and
            // `source` still has to be freed below.
            let Some(mut ball) = self.spawn_ball(&ball_scene, &mesh) else {
                break;
            };

            ball.bind_mut().set_number(number);
            ball.bind_mut().set_enumeration(number);
            ball.set_name(&name);
            ball.set_transform(source_mesh.get_transform());

            self.base_mut().add_child(&ball);
            self.balls.push(ball);
        }

        source.free();
    }

    /// Instantiates `ball_scene` and hands it `mesh`. Returns `None` after
    /// reporting a scene that cannot act as a ball, freeing whatever it built.
    fn spawn_ball(&self, ball_scene: &Gd<PackedScene>, mesh: &Gd<Mesh>) -> Option<Gd<Ball>> {
        let Some(node) = ball_scene.instantiate() else {
            godot_error!("BallRack: ball scene {} has no root node", self.ball_scene);
            return None;
        };

        let ball = match node.try_cast::<Ball>() {
            Ok(ball) => ball,
            Err(node) => {
                godot_error!("BallRack: the root of {} is not a Ball", self.ball_scene);
                node.free();
                return None;
            }
        };

        match ball.try_get_node_as::<MeshInstance3D>("MeshInstance3D") {
            Some(mut ball_mesh) => ball_mesh.set_mesh(mesh),
            None => {
                godot_error!("BallRack: {} has no MeshInstance3D child", self.ball_scene);
                ball.free();
                return None;
            }
        }

        Some(ball)
    }

    /// Removes the balls of the previous build, so a rebuild does not stack a
    /// second rack on top of the first one.
    fn clear(&mut self) {
        for mut ball in std::mem::take(&mut self.balls) {
            self.base_mut().remove_child(&ball);
            ball.queue_free();
        }
    }

    /// `"CueBall"` -> 0, `"Ball_07"` -> 7.
    ///
    /// The node name is the only reliable identity: `balls.blend` carries just
    /// 8 colour materials for 15 balls, so the material says nothing.
    fn ball_number_from_name(name: &str) -> Option<i32> {
        if name == "CueBall" {
            return Some(0);
        }

        name.rsplit_once('_')?.1.parse().ok()
    }

    #[func]
    pub fn reset_rack(&mut self) {
        for ball in &mut self.balls {
            ball.bind_mut().reset();
        }
    }

    /// The balls of the current build, for the GameManager to poll.
    #[func]
    pub fn get_balls(&self) -> Array<Gd<Ball>> {
        self.balls.iter().cloned().collect()
    }

    #[func]
    pub fn get_num_balls(&self) -> u8 {
        return self.balls.len() as u8;
    }
}
