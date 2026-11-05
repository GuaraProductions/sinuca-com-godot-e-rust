use godot::classes::Button;
use godot::classes::CanvasLayer;
use godot::classes::CenterContainer;
use godot::classes::HBoxContainer;
use godot::classes::ICanvasLayer;
use godot::classes::Label;
use godot::classes::LineEdit;
use godot::classes::MarginContainer;
use godot::classes::OptionButton;
use godot::classes::PanelContainer;
use godot::classes::VBoxContainer;
use godot::prelude::*;

const NAME_MAX_LEN: i32 = 16;
const PANEL_PADDING: i32 = 14;

/// (Control, what it does), shown beside the form.
const COMMANDS: [(&str, &str); 8] = [
    ("Mover o mouse", "Mirar o taco"),
    ("Botão esquerdo (segurar)", "Preparar a tacada"),
    ("Puxar o mouse para baixo", "Aumentar a força"),
    ("Soltar o botão esquerdo", "Dar a tacada"),
    ("Botão direito (segurar)", "Visão livre: o mouse gira a câmera"),
    ("Roda do mouse / W A S D", "Na visão livre: zoom / mover a câmera"),
    ("R", "Reiniciar a partida"),
    ("Esc", "Voltar ao menu"),
];

/// Start screen shown over the orbiting table. Built in code, so there is no
/// scene to keep in sync with the node lookups.
#[derive(GodotClass)]
#[class(init, base=CanvasLayer)]
pub struct MainMenu {
    base: Base<CanvasLayer>,

    name1: Option<Gd<LineEdit>>,
    name2: Option<Gd<LineEdit>>,
    mode: Option<Gd<OptionButton>>,
    difficulty: Option<Gd<OptionButton>>,
    difficulty_label: Option<Gd<Label>>,
}

#[godot_api]
impl ICanvasLayer for MainMenu {
    fn ready(&mut self) {
        let mut center = CenterContainer::new_alloc();
        center.set_anchors_preset(godot::classes::control::LayoutPreset::FULL_RECT);

        let mut panel = PanelContainer::new_alloc();
        panel.set_custom_minimum_size(Vector2::new(360.0, 0.0));
        let mut column = VBoxContainer::new_alloc();
        column.add_theme_constant_override("separation", 10);
        panel.add_child(&Self::padded(&column));
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 16);
        row.add_child(&panel);
        row.add_child(&Self::commands_panel());
        center.add_child(&row);

        let mut title = Label::new_alloc();
        title.set_text("SINUCA");
        title.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        title.add_theme_font_size_override("font_size", 36);
        column.add_child(&title);

        let mut name1 = Self::name_field("Jogador 1");
        let mut name2 = Self::name_field("Jogador 2");
        column.add_child(&Self::caption("Nome do jogador 1"));
        column.add_child(&name1);
        column.add_child(&Self::caption("Nome do jogador 2"));
        column.add_child(&name2);

        let mut mode = OptionButton::new_alloc();
        mode.add_item("2 jogadores");
        mode.add_item("Contra o bot");
        column.add_child(&Self::caption("Modo"));
        column.add_child(&mode);

        let mut difficulty = OptionButton::new_alloc();
        difficulty.add_item("Fácil");
        difficulty.add_item("Médio");
        difficulty.add_item("Difícil");
        difficulty.select(1);
        let difficulty_label = Self::caption("Dificuldade do bot");
        column.add_child(&difficulty_label);
        column.add_child(&difficulty);

        let mut play = Button::new_alloc();
        play.set_text("Jogar");
        let mut quit = Button::new_alloc();
        quit.set_text("Sair");
        column.add_child(&play);
        column.add_child(&quit);

        play.signals().pressed().connect_other(&*self, Self::on_play);
        quit.signals().pressed().connect_other(&*self, Self::on_quit);
        mode.signals()
            .item_selected()
            .connect_other(&*self, Self::on_mode_changed);

        self.base_mut().add_child(&center);

        // Reuse the handles for reading back and toggling later.
        name1.set_text("");
        name2.set_text("");
        self.name1 = Some(name1);
        self.name2 = Some(name2);
        self.mode = Some(mode);
        self.difficulty = Some(difficulty);
        self.difficulty_label = Some(difficulty_label);
        self.refresh_difficulty_visibility();
    }
}

#[godot_api]
impl MainMenu {
    /// `difficulty`: 0 = fácil, 1 = médio, 2 = difícil.
    #[signal]
    pub fn start_requested(name1: GString, name2: GString, vs_bot: bool, difficulty: i64);

    /// Keeps a panel's contents off its edges.
    fn padded(content: &Gd<VBoxContainer>) -> Gd<MarginContainer> {
        let mut margin = MarginContainer::new_alloc();
        for side in ["margin_left", "margin_top", "margin_right", "margin_bottom"] {
            margin.add_theme_constant_override(side, PANEL_PADDING);
        }
        margin.add_child(content);
        margin
    }

    fn commands_panel() -> Gd<PanelContainer> {
        let mut panel = PanelContainer::new_alloc();
        let mut column = VBoxContainer::new_alloc();
        column.add_theme_constant_override("separation", 8);
        panel.add_child(&Self::padded(&column));

        let mut heading = Label::new_alloc();
        heading.set_text("Comandos");
        heading.add_theme_font_size_override("font_size", 24);
        column.add_child(&heading);

        for (control, action) in COMMANDS {
            let mut key = Label::new_alloc();
            key.set_text(control);
            key.set_modulate(Color::from_rgb(1.0, 0.82, 0.45));
            column.add_child(&key);
            let mut description = Label::new_alloc();
            description.set_text(action);
            column.add_child(&description);
        }
        panel
    }

    fn name_field(placeholder: &str) -> Gd<LineEdit> {
        let mut field = LineEdit::new_alloc();
        field.set_placeholder(placeholder);
        field.set_max_length(NAME_MAX_LEN);
        field
    }

    fn caption(text: &str) -> Gd<Label> {
        let mut label = Label::new_alloc();
        label.set_text(text);
        label
    }

    fn vs_bot(&self) -> bool {
        self.mode.as_ref().is_some_and(|mode| mode.get_selected() == 1)
    }

    fn refresh_difficulty_visibility(&mut self) {
        let show = self.vs_bot();
        if let Some(difficulty) = self.difficulty.as_mut() {
            difficulty.set_visible(show);
        }
        if let Some(label) = self.difficulty_label.as_mut() {
            label.set_visible(show);
        }
        if let Some(name2) = self.name2.as_mut() {
            name2.set_placeholder(if show { "Bot" } else { "Jogador 2" });
        }
    }

    fn on_mode_changed(&mut self, _index: i64) {
        self.refresh_difficulty_visibility();
    }

    fn on_play(&mut self) {
        let name1 = self.name1.as_ref().map(|f| f.get_text()).unwrap_or_default();
        let name2 = self.name2.as_ref().map(|f| f.get_text()).unwrap_or_default();
        let difficulty = self
            .difficulty
            .as_ref()
            .map(|d| d.get_selected() as i64)
            .unwrap_or(1);
        let vs_bot = self.vs_bot();
        self.signals()
            .start_requested()
            .emit(&name1, &name2, vs_bot, difficulty);
    }

    fn on_quit(&mut self) {
        self.base().get_tree().quit();
    }
}
