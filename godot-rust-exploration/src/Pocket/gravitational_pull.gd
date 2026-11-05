extends Area3D

func _physics_process(_delta: float) -> void:
	var physics_bodies = get_overlapping_bodies()
	if physics_bodies.size() <= 0: return
	
	for body in physics_bodies:
		
		if body is not RigidBody3D:
			continue
		
		var v = body.linear_velocity
		body.linear_velocity = Vector3(v.x * 0.2, -6, v.z * 0.2)
