use godot::classes::control::{LayoutPreset, SizeFlags};
use godot::classes::node::ProcessMode;
use godot::classes::{
    Button, CanvasLayer, CenterContainer, ColorRect, HBoxContainer, ICanvasLayer, InputEvent,
    Label, MarginContainer, PanelContainer, VBoxContainer,
};
use godot::global::HorizontalAlignment;
use godot::prelude::*;

const PANEL_PADDING: i32 = 20;
const DIM_COLOR: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.55);

/// Asks before abandoning a match. The GameManager pauses the tree while this
/// is open, so this node alone keeps running and receiving input.
#[derive(GodotClass)]
#[class(init, base=CanvasLayer)]
pub struct ExitPrompt {
    base: Base<CanvasLayer>,
    continue_button: Option<Gd<Button>>,
}

#[godot_api]
impl ICanvasLayer for ExitPrompt {
    fn ready(&mut self) {
        self.base_mut().set_process_mode(ProcessMode::ALWAYS);
        // Above the HUD and the menu.
        self.base_mut().set_layer(10);

        let mut dim = ColorRect::new_alloc();
        dim.set_color(DIM_COLOR);
        dim.set_anchors_preset(LayoutPreset::FULL_RECT);
        self.base_mut().add_child(&dim);

        let mut center = CenterContainer::new_alloc();
        center.set_anchors_preset(LayoutPreset::FULL_RECT);

        let mut column = VBoxContainer::new_alloc();
        column.add_theme_constant_override("separation", 12);

        column.add_child(&Self::centered_label("Voltar ao menu principal?", 24));
        column.add_child(&Self::centered_label("A partida atual será perdida.", 16));

        let mut buttons = HBoxContainer::new_alloc();
        buttons.add_theme_constant_override("separation", 12);
        let mut continue_button = Self::wide_button("Continuar");
        let mut leave_button = Self::wide_button("Voltar ao menu");
        buttons.add_child(&continue_button);
        buttons.add_child(&leave_button);
        column.add_child(&buttons);

        column.add_child(&Self::centered_label("Esc: confirmar", 14));

        let mut margin = MarginContainer::new_alloc();
        for side in ["margin_left", "margin_top", "margin_right", "margin_bottom"] {
            margin.add_theme_constant_override(side, PANEL_PADDING);
        }
        margin.add_child(&column);

        let mut panel = PanelContainer::new_alloc();
        panel.add_child(&margin);
        center.add_child(&panel);
        self.base_mut().add_child(&center);

        continue_button
            .signals()
            .pressed()
            .connect_other(&*self, Self::on_continue);
        leave_button
            .signals()
            .pressed()
            .connect_other(&*self, Self::on_leave);

        self.continue_button = Some(continue_button);
        self.base_mut().set_visible(false);
    }

    fn input(&mut self, event: Gd<InputEvent>) {
        if !self.base().is_visible() {
            return;
        }
        if event.is_action_pressed("ui_cancel") && !event.is_echo() {
            // Consumed here so the same key press cannot reach anything else.
            if let Some(mut viewport) = self.base().get_viewport() {
                viewport.set_input_as_handled();
            }
            self.on_leave();
        }
    }
}

#[godot_api]
impl ExitPrompt {
    #[signal]
    pub fn confirmed();

    #[signal]
    pub fn cancelled();

    /// Shows the prompt with "Continuar" focused, so Enter is the safe choice.
    pub fn open(&mut self) {
        self.base_mut().set_visible(true);
        if let Some(button) = self.continue_button.as_mut() {
            button.grab_focus();
        }
    }

    fn on_continue(&mut self) {
        self.base_mut().set_visible(false);
        self.signals().cancelled().emit();
    }

    fn on_leave(&mut self) {
        self.base_mut().set_visible(false);
        self.signals().confirmed().emit();
    }

    fn centered_label(text: &str, font_size: i32) -> Gd<Label> {
        let mut label = Label::new_alloc();
        label.set_text(text);
        label.set_horizontal_alignment(HorizontalAlignment::CENTER);
        label.add_theme_font_size_override("font_size", font_size);
        label
    }

    fn wide_button(text: &str) -> Gd<Button> {
        let mut button = Button::new_alloc();
        button.set_text(text);
        button.set_custom_minimum_size(Vector2::new(160.0, 0.0));
        button.set_h_size_flags(SizeFlags::EXPAND_FILL);
        button
    }
}
