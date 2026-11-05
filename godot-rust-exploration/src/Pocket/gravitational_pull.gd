extends Area3D

## Helps a ball that is already over the pocket opening drop in, instead of
## flying across it and bouncing back out off the far edge.
##
## Horizontal speed has to be steered toward the center, not removed: the opening
## is narrow compared to this area, so a ball stopped anywhere else just sits on
## the felt with nothing moving it into the hole.

## Horizontal distance from the center at which a ball is captured. Must stay
## inside the opening (radius 1.64 on side pockets, 1.77 on corners), so that
## balls still on the felt are left alone.
@export var capture_radius := 1.6
## Horizontal speed toward the center, per unit of distance from it.
@export var centering_rate := 6.0
## Fraction of the way the horizontal velocity moves toward the steered velocity
## each physics step. Lower keeps more of the ball's incoming momentum.
@export_range(0.0, 1.0) var steering := 0.35
## Minimum downward speed of a captured ball.
@export var fall_speed := 6.0


func _physics_process(_delta: float) -> void:
	for body in get_overlapping_bodies():
		if body is not RigidBody3D:
			continue

		var to_center: Vector3 = global_position - body.global_position
		to_center.y = 0.0
		if to_center.length() > capture_radius:
			continue

		var velocity: Vector3 = body.linear_velocity
		var horizontal := Vector3(velocity.x, 0.0, velocity.z).lerp(to_center * centering_rate, steering)
		body.linear_velocity = Vector3(horizontal.x, minf(velocity.y, -fall_speed), horizontal.z)
