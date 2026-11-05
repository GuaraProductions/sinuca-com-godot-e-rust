use godot::prelude::*;
use godot::classes::Area3D;
use godot::classes::IArea3D;

use crate::ball::Ball;

/// The mouth of one pocket. It observes; it never acts on the ball.
///
/// `body_entered` fires while the physics server is still flushing the frame,
/// and `Ball::pot()` writes `freeze` and the collision layers -- doing that from
/// inside the callback is undefined behaviour. So the pocket only records the
/// ball, and the GameManager drains the queue from its own `physics_process`,
/// where changing that state is safe.
#[derive(GodotClass)]
#[class(init, base=Area3D)]
pub struct Pocket {
    /// Balls that arrived since the last `take_potted()`. A ball is recorded at
    /// most once: both `Ball::is_potted()` and this queue are checked on entry,
    /// so overlapping mouths cannot report the same ball twice.
    potted: Vec<Gd<Ball>>,
    base: Base<Area3D>,
}

#[godot_api]
impl IArea3D for Pocket {

    fn ready(&mut self) {
        self.signals().body_entered().connect_self(Self::on_body_entered);
    }
}

#[godot_api]
impl Pocket {

    /// Announced the instant a ball crosses the mouth, for whatever wants to
    /// react right away (sound, HUD). Handlers must not touch the ball's
    /// physics state -- that is what `take_potted()` is for.
    #[signal]
    fn ball_potted(ball: Gd<Ball>);

    /// Hands over the balls collected since the last call and empties the queue.
    ///
    /// Balls freed in the meantime -- a rack rebuild calls `queue_free` on all
    /// of them -- are dropped here instead of being handed out as dangling
    /// references.
    #[func]
    pub fn take_potted(&mut self) -> Array<Gd<Ball>> {

        std::mem::take(&mut self.potted)
            .into_iter()
            .filter(|ball| ball.is_instance_valid())
            .collect()
    }

    fn on_body_entered(&mut self, body: Gd<Node3D>) {

        let Ok(ball) = body.try_cast::<Ball>() else {
            return;
        };

        if ball.bind().is_potted() || self.potted.contains(&ball) {
            return;
        }

        godot_print!("{}: ball {}", self.base().get_name(), ball.bind().get_number());

        self.potted.push(ball.clone());
        self.signals().ball_potted().emit(&ball);
    }
}
