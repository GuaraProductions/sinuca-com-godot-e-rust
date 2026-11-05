use godot::prelude::*;

struct MyExtension;

#[gdextension]
unsafe impl ExtensionLibrary for MyExtension {}

mod aim_model;
mod ball;
mod ball_markers;
mod ball_rack;
mod bot_player;
mod camera_rig;
mod game_manager;
mod main_menu;
mod player;
mod pocket;
mod pool_cue;
mod shot_planner;
mod table_query;
mod trajectory_preview;
mod ui;

pub use ball::Ball;
pub use ball_rack::BallRack;
pub use player::Player;
pub use ui::Ui;
