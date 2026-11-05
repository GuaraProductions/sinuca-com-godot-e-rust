use godot::classes::CanvasLayer;
use godot::classes::HFlowContainer;
use godot::classes::ICanvasLayer;
use godot::classes::Label;
use godot::classes::control::LayoutPreset;
use godot::global::HorizontalAlignment;
use godot::prelude::*;

use crate::ball::BallType;

const HINT: &str = "Mouse: mirar   |   Botão esquerdo (segurar) + puxar o mouse para baixo: tacada   |   \
                    Botão direito (segurar): visão livre   |   N: números   |   R: reiniciar   |   Esc: menu";

#[derive(GodotClass)]
#[class(init, base=CanvasLayer)]
pub struct Ui {
    base: Base<CanvasLayer>,

    player1_points: u8,
    player2_points: u8,

    #[init(node = "%Name1")]
    player1: OnReady<Gd<Label>>,
    #[init(node = "%Name2")]
    player2: OnReady<Gd<Label>>,

    #[init(node = "%Team1")]
    type1: OnReady<Gd<Label>>,
    #[init(node = "%Team2")]
    type2: OnReady<Gd<Label>>,

    #[init(node = "%Points1")]
    score_player1: OnReady<Gd<Label>>,
    #[init(node = "%Points2")]
    score_player2: OnReady<Gd<Label>>,

    #[init(node = "%Resultado")]
    result: OnReady<Gd<Label>>,

    #[init(node = "%PlayerAtual")]
    current_player: OnReady<Gd<Label>>,

    #[init(node = "%Balls1")]
    balls1: OnReady<Gd<HFlowContainer>>,
    #[init(node = "%Balls2")]
    balls2: OnReady<Gd<HFlowContainer>>,

    name1: String,
    name2: String,
}

#[godot_api]
impl ICanvasLayer for Ui {
    fn ready(&mut self) {
        self.player1.set_visible(false);
        self.player2.set_visible(false);
        self.type1.set_visible(false);
        self.type2.set_visible(false);
        self.score_player1.set_visible(false);
        self.score_player2.set_visible(false);
        self.result.set_visible(false);

        self.player1_points = 0;
        self.player2_points = 0;

        let mut hint = Label::new_alloc();
        hint.set_text(HINT);
        hint.set_horizontal_alignment(HorizontalAlignment::CENTER);
        hint.add_theme_font_size_override("font_size", 14);
        self.base_mut().add_child(&hint);
        hint.set_anchors_and_offsets_preset(LayoutPreset::BOTTOM_WIDE);
        hint.set_offset(godot::builtin::Side::BOTTOM, -8.0);
        hint.set_offset(godot::builtin::Side::TOP, -32.0);
    }
}

#[godot_api]
impl Ui {
    /// Clears scores, types and the result banner for a fresh match.
    pub fn reset_match(&mut self) {
        self.player1_points = 0;
        self.player2_points = 0;
        self.type1.set_visible(false);
        self.type2.set_visible(false);
        self.score_player1.set_visible(false);
        self.score_player2.set_visible(false);
        self.result.set_visible(false);
        self.update_ball_chips(BallType::Unidentified, BallType::Unidentified, &[false; 16]);
    }

    /// Marks whose turn it is next to the player's name.
    pub fn set_active_player(&mut self, player: u8) {
        let (first, second) = (self.name1.clone(), self.name2.clone());
        let marked = |active: bool, name: &str| {
            if active { format!("\u{25B6} {name}") } else { name.to_string() }
        };
        self.player1.set_text(&marked(player == 1, &first));
        self.player2.set_text(&marked(player == 2, &second));
    }

    /// Lists the numbers each player has to pot; `potted` is indexed by number.
    pub fn update_ball_chips(&mut self, type1: BallType, type2: BallType, potted: &[bool; 16]) {
        Self::fill_chips(&mut self.balls1, type1, potted);
        Self::fill_chips(&mut self.balls2, type2, potted);
    }

    fn fill_chips(row: &mut Gd<HFlowContainer>, group: BallType, potted: &[bool; 16]) {
        for mut child in row.get_children().iter_shared() {
            row.remove_child(&child);
            child.queue_free();
        }

        let remainder = match group {
            BallType::Odd => 1,
            BallType::Even => 0,
            BallType::Unidentified => return,
        };
        for number in (1..=15).filter(|n: &usize| n % 2 == remainder) {
            let mut chip = Label::new_alloc();
            chip.set_text(&number.to_string());
            if potted[number] {
                chip.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, 0.25));
            }
            row.add_child(&chip);
        }
    }

    pub fn set_hud_visible(&mut self, visible: bool) {
        self.base_mut().set_visible(visible);
    }

    pub fn set_player1_name(&mut self, text: &str) {
        self.name1 = text.to_string();
        self.player1.set_text(text);
        self.player1.set_visible(true);
    }

    pub fn set_current_player(&mut self, text: &str) {
        self.current_player
            .set_text(&format!("Player atual: {}", text));
    }

    pub fn set_player1_type(&mut self, ball_type: BallType) {
        let ball_type_text = format!("Tipo: {}", ball_type);
        self.type1.set_text(&ball_type_text);
        self.type1.set_visible(true);
    }

    pub fn add_player1_points(&mut self) {
        self.player1_points += 1;

        let points_text = format!("Pontos: {}", self.player1_points);
        self.score_player1.set_text(&points_text);
        self.score_player1.set_visible(true);
    }

    pub fn set_player2_name(&mut self, text: &str) {
        self.name2 = text.to_string();
        self.player2.set_text(text);
        self.player2.set_visible(true);
    }

    pub fn set_player2_type(&mut self, ball_type: BallType) {
        let ball_type_text = format!("Tipo: {}", ball_type);
        self.type2.set_text(&ball_type_text);
        self.type2.set_visible(true);
    }

    pub fn add_player2_points(&mut self) {
        self.player2_points += 1;

        let points_text = format!("Pontos: {}", self.player2_points);
        self.score_player2.set_text(&points_text);
        self.score_player2.set_visible(true);
    }

    pub fn show_result(&mut self, text: &str) {
        self.result.set_text(text);
        self.result.set_visible(true);
    }
}
