use godot::classes::INode3D;
use godot::classes::Input;
use godot::classes::InputEvent;
use godot::classes::InputEventKey;
use godot::classes::InputEventMouseButton;
use godot::classes::Node3D;
use godot::classes::input::MouseMode;
use godot::global::{Key, MouseButton};
use godot::prelude::*;

use crate::BallRack;
use crate::Ui;
use crate::ball_markers::BallMarkers;
use crate::bot_player::BotPlayer;
use crate::camera_rig::CameraRig;
use crate::exit_prompt::ExitPrompt;
use crate::main_menu::MainMenu;
use crate::trajectory_preview::TrajectoryPreview;
use crate::ball::{Ball, BallType};
use crate::player::Player;
use crate::pocket::Pocket;
use crate::pool_cue::PoolCue;

/// Longest a shot may keep the turn. Normal shots settle well inside this; it
/// only stops a ball stuck somewhere odd from hanging the match.
const RESOLVE_TIMEOUT_SECONDS: f64 = 30.0;

#[derive(Default, PartialEq, Clone, Copy, Debug)]
enum GamePhase {
    #[default]
    Menu,
    /// A player (human or bot) is lining up a shot.
    Aiming,
    /// The shot is rolling; waiting for the table to settle.
    Resolving,
    GameOver,
}

/// What the menu asked for. Kept so R can restart the same match.
#[derive(Clone, Default)]
struct MatchConfig {
    name1: String,
    name2: String,
    vs_bot: bool,
    difficulty: i64,
}

#[derive(GodotClass)]
#[class(init, base=Node3D)]
pub struct GameManager {
    #[init(node = "BallRack")]
    ball_rack: OnReady<Gd<BallRack>>,

    #[init(node = "Pockets")]
    pockets: OnReady<Gd<Node3D>>,
    #[init(node = "PoolCue")]
    pool_cue: OnReady<Gd<PoolCue>>,

    cue_ball: Option<Gd<Ball>>,
    #[init(node = "Ui")]
    ui: OnReady<Gd<Ui>>,
    #[init(node = "CameraRig")]
    camera: OnReady<Gd<CameraRig>>,
    #[init(node = "MainMenu")]
    menu: OnReady<Gd<MainMenu>>,
    #[init(node = "TrajectoryPreview")]
    trajectory: OnReady<Gd<TrajectoryPreview>>,
    #[init(node = "BotPlayer")]
    bot: OnReady<Gd<BotPlayer>>,
    #[init(node = "BallMarkers")]
    markers: OnReady<Gd<BallMarkers>>,
    #[init(node = "ExitPrompt")]
    exit_prompt: OnReady<Gd<ExitPrompt>>,

    base: Base<Node3D>,

    player_one: Player,
    player_two: Player,

    /// 1 or 2.
    #[init(val = 1)]
    current_player: u8,

    /// The current shot sank a ball of the shooter's own type.
    shot_potted_own: bool,
    /// The cue ball fell during the current shot.
    foul: bool,
    phase: GamePhase,
    config: MatchConfig,
    resolving_seconds: f64,
}

#[godot_api]
impl INode3D for GameManager {
    fn ready(&mut self) {
        let cue_ball = self.ball_rack.get_node_as::<Ball>("CueBall");
        self.cue_ball = Some(cue_ball);

        self.menu
            .signals()
            .start_requested()
            .connect_other(&*self, Self::on_start_requested);
        self.pool_cue
            .signals()
            .shot_taken()
            .connect_other(&*self, Self::on_shot_taken);
        self.exit_prompt
            .signals()
            .confirmed()
            .connect_other(&*self, Self::on_leave_confirmed);
        self.exit_prompt
            .signals()
            .cancelled()
            .connect_other(&*self, Self::on_leave_cancelled);

        self.enter_menu();
    }

    fn input(&mut self, event: Gd<InputEvent>) {
        if self.phase != GamePhase::Menu {
            if event.is_action_pressed("ui_cancel") && !event.is_echo() {
                self.request_leave();
                return;
            }
            if event.is_action_pressed("reset_game") && !event.is_echo() {
                let config = self.config.clone();
                self.start_match(config);
                return;
            }
            if let Ok(key) = event.clone().try_cast::<InputEventKey>() {
                if key.is_pressed() && !key.is_echo() && key.get_physical_keycode() == Key::N {
                    self.markers.bind_mut().toggle_numbers();
                    return;
                }
            }
        }

        if let Ok(button) = event.try_cast::<InputEventMouseButton>() {
            if button.get_button_index() == MouseButton::RIGHT {
                if button.is_pressed() {
                    self.begin_free_look();
                } else {
                    self.end_free_look();
                }
            }
        }
    }

    fn physics_process(&mut self, delta: f64) {
        if matches!(self.phase, GamePhase::Menu | GamePhase::GameOver)
            || !self.pockets.is_instance_valid()
        {
            return;
        }

        let children = self.pockets.get_children();
        for child in children.iter_shared() {
            let Ok(pocket) = child.try_cast::<Pocket>() else {
                continue;
            };

            self.process_balls(pocket);
        }

        if self.phase == GamePhase::GameOver {
            return;
        }

        if self.phase != GamePhase::Resolving {
            self.resolving_seconds = 0.0;
            return;
        }

        self.resolving_seconds += delta;
        let cue_disabled = self.pool_cue.bind_mut().is_disabled();
        let timed_out = self.resolving_seconds > RESOLVE_TIMEOUT_SECONDS;
        if cue_disabled && (timed_out || self.all_balls_at_rest()) {
            self.end_turn();
        }
    }
}

#[godot_api]
impl GameManager {
    fn set_mouse_mode(mode: MouseMode) {
        Input::singleton().set_mouse_mode(mode);
    }

    fn enter_menu(&mut self) {
        self.phase = GamePhase::Menu;
        self.bot.bind_mut().cancel();
        self.trajectory.bind_mut().set_active(false);
        self.markers.set_visible(false);
        self.camera.bind_mut().enter_menu();
        self.menu.set_visible(true);
        self.ui.bind_mut().set_hud_visible(false);
        {
            let mut cue = self.pool_cue.bind_mut();
            cue.set_input_locked(true);
            cue.set_disabled(true);
        }
        Self::set_mouse_mode(MouseMode::VISIBLE);
    }

    /// First Esc: pause and ask. A finished match has nothing left to lose.
    fn request_leave(&mut self) {
        if self.phase == GamePhase::GameOver {
            self.enter_menu();
            return;
        }

        // Their button releases would be missed while paused, leaving the camera
        // in free look or the cue half drawn when play resumes.
        self.end_free_look();
        if !self.current_ref().is_bot() {
            self.pool_cue.bind_mut().set_input_locked(true);
        }

        self.base().get_tree().set_pause(true);
        Self::set_mouse_mode(MouseMode::VISIBLE);
        self.exit_prompt.bind_mut().open();

        if let Some(mut viewport) = self.base().get_viewport() {
            viewport.set_input_as_handled();
        }
    }

    fn on_leave_confirmed(&mut self) {
        self.base().get_tree().set_pause(false);
        self.enter_menu();
    }

    fn on_leave_cancelled(&mut self) {
        self.base().get_tree().set_pause(false);
        Self::set_mouse_mode(MouseMode::CAPTURED);
        if self.phase == GamePhase::Aiming && !self.current_ref().is_bot() {
            self.pool_cue.bind_mut().set_input_locked(false);
        }
    }

    fn on_start_requested(&mut self, name1: GString, name2: GString, vs_bot: bool, difficulty: i64) {
        let config = MatchConfig {
            name1: name1.to_string(),
            name2: name2.to_string(),
            vs_bot,
            difficulty,
        };
        self.start_match(config);
    }

    /// Runs inside `PoolCue::shoot`, possibly called from the bot, so this must
    /// not bind the cue or the bot.
    fn on_shot_taken(&mut self, _power: f32) {
        self.phase = GamePhase::Resolving;
        self.trajectory.bind_mut().set_active(false);
        self.markers.bind_mut().set_highlight(BallType::Unidentified);
    }

    /// Resets everything a previous match could have left behind.
    fn start_match(&mut self, config: MatchConfig) {
        self.config = config.clone();
        self.bot.bind_mut().cancel();

        self.ball_rack.bind_mut().reset_rack();
        let children = self.pockets.get_children();
        for child in children.iter_shared() {
            if let Ok(mut pocket) = child.try_cast::<Pocket>() {
                pocket.bind_mut().take_potted();
            }
        }

        self.player_one = Player::new(1, BallType::Unidentified)
            .with_name(&config.name1, "Jogador 1");
        let default2 = if config.vs_bot { "Bot" } else { "Jogador 2" };
        self.player_two = Player::new(2, BallType::Unidentified)
            .with_name(&config.name2, default2)
            .with_bot(config.vs_bot);

        self.current_player = 1;
        self.shot_potted_own = false;
        self.foul = false;

        {
            let name1 = self.player_one.name().to_string();
            let name2 = self.player_two.name().to_string();
            let mut ui = self.ui.bind_mut();
            ui.reset_match();
            ui.set_hud_visible(true);
            ui.set_player1_name(&name1);
            ui.set_player2_name(&name2);
            ui.set_current_player(&name1);
        }

        self.menu.set_visible(false);
        self.markers.set_visible(true);
        self.camera.bind_mut().enter_table();
        Self::set_mouse_mode(MouseMode::CAPTURED);

        self.pool_cue.bind_mut().set_disabled(false);
        self.begin_turn();
    }

    /// Hands the cue to whoever is up: unlocked for a human, locked for a bot.
    fn begin_turn(&mut self) {
        self.phase = GamePhase::Aiming;
        let is_bot = self.current_ref().is_bot();
        let group = self.current_ref().ball_numeration;
        let current = self.current_player;

        {
            let mut cue = self.pool_cue.bind_mut();
            cue.reset_player();
            cue.set_input_locked(is_bot);
        }

        self.trajectory.bind_mut().set_active(!is_bot);
        self.markers.bind_mut().set_highlight(group);
        self.ui.bind_mut().set_active_player(current);
        self.refresh_ball_chips();

        if is_bot {
            let difficulty = self.config.difficulty;
            self.bot.bind_mut().begin_turn(group, difficulty);
        }
    }

    /// Shows each player's numbers in the HUD, dimming the ones already potted.
    fn refresh_ball_chips(&mut self) {
        let mut potted = [false; 16];
        let balls = self.ball_rack.bind().get_balls();
        for ball in balls.iter_shared() {
            let ball = ball.bind();
            if let Some(slot) = potted.get_mut(ball.get_number() as usize) {
                *slot = ball.is_potted();
            }
        }
        let (type1, type2) = (self.player_one.ball_numeration, self.player_two.ball_numeration);
        self.ui.bind_mut().update_ball_chips(type1, type2, &potted);
    }

    fn current_ref(&self) -> &Player {
        if self.current_player == 1 {
            &self.player_one
        } else {
            &self.player_two
        }
    }

    fn begin_free_look(&mut self) {
        if self.phase != GamePhase::Aiming || self.current_ref().is_bot() {
            return;
        }
        if self.pool_cue.bind().is_charging() {
            return;
        }
        self.pool_cue.bind_mut().set_input_locked(true);
        self.camera.bind_mut().enter_free_look();
    }

    fn end_free_look(&mut self) {
        if !self.camera.bind().is_free_look() {
            return;
        }
        self.camera.bind_mut().enter_table();
        if self.phase == GamePhase::Aiming && !self.current_ref().is_bot() {
            self.pool_cue.bind_mut().set_input_locked(false);
        }
    }

    fn current_mut(&mut self) -> &mut Player {
        if self.current_player == 1 {
            &mut self.player_one
        } else {
            &mut self.player_two
        }
    }

    fn other_mut(&mut self) -> &mut Player {
        if self.current_player == 1 {
            &mut self.player_two
        } else {
            &mut self.player_one
        }
    }

    fn other_player_id(&self) -> u8 {
        if self.current_player == 1 { 2 } else { 1 }
    }

    fn all_balls_at_rest(&self) -> bool {
        let balls = self.ball_rack.bind().get_balls();
        balls.iter_shared().all(|ball| ball.bind().is_at_rest())
    }

    fn add_ui_point(&mut self, player: u8) {
        let mut ui = self.ui.bind_mut();
        if player == 1 {
            ui.add_player1_points();
        } else {
            ui.add_player2_points();
        }
    }

    fn set_ui_type(&mut self, player: u8, ball_type: BallType) {
        let mut ui = self.ui.bind_mut();
        if player == 1 {
            ui.set_player1_type(ball_type);
        } else {
            ui.set_player2_type(ball_type);
        }
    }

    fn end_turn(&mut self) {
        if self.foul {
            if let Some(mut cue_ball) = self.cue_ball.clone() {
                cue_ball.bind_mut().reset();
            }
        }

        if !(self.shot_potted_own && !self.foul) {
            self.current_player = self.other_player_id();
        }
        let name = self.current_ref().name().to_string();
        self.ui.bind_mut().set_current_player(&name);

        self.shot_potted_own = false;
        self.foul = false;

        // Free look is only allowed while aiming; drop it if a shot ended it.
        if self.camera.bind().is_free_look() {
            self.camera.bind_mut().enter_table();
        }
        self.begin_turn();
    }

    fn process_balls(&mut self, mut pocket: Gd<Pocket>) {
        let potted = pocket.bind_mut().take_potted();
        for mut ball in potted.iter_shared() {
            if self.cue_ball.as_ref() == Some(&ball) {
                self.foul = true;
                // Left to fall, it wedges in the pocket and never comes to rest,
                // which would hold the turn open forever. `end_turn` restores it.
                ball.bind_mut().pot();
                continue;
            }

            self.handle_potted_ball(ball);
            if self.phase == GamePhase::GameOver {
                return;
            }
        }
    }

    fn handle_potted_ball(&mut self, mut ball: Gd<Ball>) {
        let ball_type = ball.bind().get_enumeration();

        if !self.current_mut().has_enumeration() {
            let opposite = match ball_type {
                BallType::Odd => BallType::Even,
                BallType::Even => BallType::Odd,
                BallType::Unidentified => BallType::Unidentified,
            };
            let current = self.current_player;
            let other = self.other_player_id();

            self.current_mut().set_enumeration(ball_type);
            self.other_mut().set_enumeration(opposite);
            self.set_ui_type(current, ball_type);
            self.set_ui_type(other, opposite);
        }

        if self.current_mut().is_same_enumeration(ball_type) {
            let current = self.current_player;
            self.current_mut().add_points(1);
            self.add_ui_point(current);
            self.shot_potted_own = true;
        } else if self.other_mut().is_same_enumeration(ball_type) {
            let other = self.other_player_id();
            self.other_mut().add_points(1);
            self.add_ui_point(other);
        }

        ball.bind_mut().pot();
        self.refresh_ball_chips();
        self.check_winner();
    }

    fn check_winner(&mut self) {
        let (mut odd_left, mut even_left) = (0u32, 0u32);
        let balls = self.ball_rack.bind().get_balls();
        for ball in balls.iter_shared() {
            let ball = ball.bind();
            if ball.get_number() == 0 || ball.is_potted() {
                continue;
            }
            match ball.get_enumeration() {
                BallType::Odd => odd_left += 1,
                BallType::Even => even_left += 1,
                BallType::Unidentified => {}
            }
        }

        // The type a player owns is only known once the first ball is potted,
        // which has always happened by the time this runs.
        let winning_type = if odd_left == 0 {
            BallType::Odd
        } else if even_left == 0 {
            BallType::Even
        } else {
            return;
        };

        let winner = if self.player_one.is_same_enumeration(winning_type) {
            self.player_one.id()
        } else {
            self.player_two.id()
        };

        self.phase = GamePhase::GameOver;
        self.trajectory.bind_mut().set_active(false);
        self.markers.bind_mut().set_highlight(BallType::Unidentified);
        let winner_name = if winner == 1 {
            self.player_one.name().to_string()
        } else {
            self.player_two.name().to_string()
        };
        self.ui.bind_mut().show_result(&format!(
            "{} venceu!\nR: jogar de novo   Esc: menu",
            winner_name
        ));
        self.pool_cue.bind_mut().set_disabled(true);
    }
}
