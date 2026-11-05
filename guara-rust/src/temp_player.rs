use godot::prelude::*;
use godot::classes::CharacterBody2D;
use godot::classes::ICharacterBody2D;
use godot::classes::Input;
use godot::global::move_toward;

const JUMP_VELOCITY : f32 = -400.0;

#[derive(GodotClass)]
#[class(base=CharacterBody2D)]
struct Player {
    #[export]
    speed: f64,
    #[export(file)]
    variavel: GString,
    base: Base<CharacterBody2D>,

}


#[godot_api]
impl Player {

    #[signal]
    fn jumped(velocity: Vector2);

    #[func]
    pub fn teste(&mut self, value: i64) {
        godot_print!("ola: {}", value);
    }

    fn funcao_depois_do_sinal(&mut self, velocity: Vector2) {
        godot_print!("cheguei aqui {}", velocity)
    }
}

#[godot_api]
impl ICharacterBody2D for Player {

    fn init(base: Base<CharacterBody2D>) -> Self {
        
        Self {
            speed : 400.0,
            base,
            variavel : GString::default()
        }
    }


    fn ready(&mut self) {

        self.signals().jumped().connect_self(Self::funcao_depois_do_sinal);

        //let mut array : VarArray = VarArray::new();
        //let mut array : Array<i32> = Array::new();

        let mut dict : Dictionary<GString, i64> = Dictionary::new();
        dict.set("guará", 32);
        godot_print!("dicionario: {}", dict)
    }

    fn physics_process(&mut self, delta: f64) {
        let speed = self.speed as f32;

        let (mut velocity, on_floor, gravity) = {
            let instance = self.base_mut();
            (instance.get_velocity(), instance.is_on_floor(), instance.get_gravity())
        };

        if !on_floor{
            velocity += gravity * (delta as f32);
        }

        let input = Input::singleton();
        if input.is_action_just_pressed("ui_accept") && on_floor {
            velocity.y = JUMP_VELOCITY;
            self.signals().jumped().emit(velocity);
        }

        let direction = input.get_axis("ui_left", "ui_right");

        if direction != 0.0 {
            velocity.x = direction * speed;
        } else {
            velocity.x = move_toward(velocity.x as f64, 0.0, speed as f64) as f32;
        }

        let mut instance = self.base_mut();
        instance.set_velocity(velocity);
        instance.move_and_slide();

    }
}

