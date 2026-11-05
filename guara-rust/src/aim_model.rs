//! Pure 2D (table-plane) shot geometry shared by the trajectory preview and
//! the bot. No engine types, so it can be unit tested without Godot.
//!
//! Points and directions are `[x, z]` pairs in world units.

pub type V2 = [f32; 2];

pub fn sub(a: V2, b: V2) -> V2 {
    [a[0] - b[0], a[1] - b[1]]
}
fn add(a: V2, b: V2) -> V2 {
    [a[0] + b[0], a[1] + b[1]]
}
fn scale(a: V2, k: f32) -> V2 {
    [a[0] * k, a[1] * k]
}
pub fn dot(a: V2, b: V2) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}
fn len(a: V2) -> f32 {
    dot(a, a).sqrt()
}
pub fn dist(a: V2, b: V2) -> f32 {
    len(sub(a, b))
}
pub fn normalize(a: V2) -> V2 {
    let l = len(a);
    if l <= f32::EPSILON { [0.0, 0.0] } else { scale(a, 1.0 / l) }
}

/// Distance along the unit ray `dir` from `origin` to the first hit with the
/// circle (`center`, `radius`), or `None` if it misses or lies behind.
pub fn ray_circle(origin: V2, dir: V2, center: V2, radius: f32) -> Option<f32> {
    let to_center = sub(center, origin);
    let along = dot(to_center, dir);
    let perp_sq = dot(to_center, to_center) - along * along;
    let r_sq = radius * radius;
    if perp_sq > r_sq {
        return None;
    }
    let t = along - (r_sq - perp_sq).sqrt();
    // Already overlapping or behind the origin: not a forward hit.
    if t < 0.0 { None } else { Some(t) }
}

/// Outcome of a cue ball striking an object ball.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contact {
    /// Cue ball centre at the moment of contact (the "ghost ball").
    pub ghost: V2,
    /// Unit direction the object ball leaves in.
    pub object_dir: V2,
    /// Unit direction the cue ball leaves in (zero for a dead-on hit).
    pub cue_dir: V2,
    /// Cosine of the cut angle: 1 is straight on, 0 is a graze.
    pub cut_cos: f32,
}

/// Cue ball from `origin` along unit `dir` against an object ball at `object`;
/// both have radius `ball_radius`.
pub fn ball_contact(origin: V2, dir: V2, object: V2, ball_radius: f32) -> Option<Contact> {
    let t = ray_circle(origin, dir, object, ball_radius * 2.0)?;
    let ghost = add(origin, scale(dir, t));
    let object_dir = normalize(sub(object, ghost));
    let cut_cos = dot(dir, object_dir).max(0.0);
    // The cue ball keeps the component of its motion along the tangent.
    let tangent = sub(dir, scale(object_dir, dot(dir, object_dir)));
    Some(Contact { ghost, object_dir, cue_dir: normalize(tangent), cut_cos })
}

/// Mirror `dir` about a surface with unit `normal`.
pub fn reflect(dir: V2, normal: V2) -> V2 {
    sub(dir, scale(normal, 2.0 * dot(dir, normal)))
}

/// Direction after a cushion: mirrored, with the along-the-rail part reduced.
pub fn bounce(dir: V2, normal: V2) -> V2 {
    let reflected = reflect(dir, normal);
    let into = scale(normal, dot(reflected, normal));
    let along = sub(reflected, into);
    normalize(add(into, scale(along, CUSHION_TANGENT_KEEP)))
}

/// Ghost-ball position that sends an object ball at `object` toward `pocket`.
pub fn ghost_for_pocket(object: V2, pocket: V2, ball_radius: f32) -> V2 {
    let out = normalize(sub(pocket, object));
    sub(object, scale(out, ball_radius * 2.0))
}

/// How far a ball travels before stopping when launched at `v0`, under
/// exponential damping `k` (1/s) plus constant deceleration `a` (units/s^2).
///
/// With v' = -k v - a: d = v0/k - (a / k^2) * ln(1 + k v0 / a).
pub fn stopping_distance(v0: f32, k: f32, a: f32) -> f32 {
    if v0 <= 0.0 {
        return 0.0;
    }
    if k <= 0.0 {
        return if a > 0.0 { v0 * v0 / (2.0 * a) } else { f32::INFINITY };
    }
    if a <= 0.0 {
        return v0 / k;
    }
    v0 / k - (a / (k * k)) * (1.0 + k * v0 / a).ln()
}

/// Launch speed needed to roll `distance` before stopping (inverse of
/// [`stopping_distance`], found by bisection since it has no closed form).
pub fn speed_for_distance(distance: f32, k: f32, a: f32) -> f32 {
    if distance <= 0.0 {
        return 0.0;
    }
    let mut hi = 1.0f32;
    while stopping_distance(hi, k, a) < distance && hi < 1.0e6 {
        hi *= 2.0;
    }
    let mut lo = 0.0f32;
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if stopping_distance(mid, k, a) < distance {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Measured in the engine: a ball rolls as far as `stopping_distance` predicts
/// for 87% of its launch speed, because it first slides and loses speed to
/// spin. Fits powers 0.1 to 0.6 to within about 5%.
pub const ROLL_SPEED_FACTOR: f32 = 0.87;
/// Fraction of the remaining travel kept after bouncing off a cushion.
const CUSHION_KEEP: f32 = 0.75;
/// Cushion friction leaves a ball this much of its speed along the rail, so it
/// leaves at a flatter angle than a mirror would. Measured in the engine.
const CUSHION_TANGENT_KEEP: f32 = 0.8;
/// Cushion bounces the preview follows before giving up.
const MAX_SEGMENTS: usize = 3;

/// What the cue ball is predicted to do. `points` is its path; it ends at
/// `contact.ghost` when a ball is hit first.
#[derive(Debug, Clone, Default)]
pub struct Trace {
    pub points: Vec<V2>,
    pub contact: Option<Contact>,
    /// Where the object ball starts, when there is a contact.
    pub object: Option<V2>,
    pub pocketed: bool,
}

/// Follows the cue ball from `origin` along unit `dir` for at most `budget`
/// units. `cast_cushion(from, dir, max)` returns the distance and unit normal
/// of the first cushion hit.
pub fn trace_shot(
    origin: V2,
    dir: V2,
    budget: f32,
    balls: &[V2],
    pockets: &[V2],
    ball_radius: f32,
    pocket_radius: f32,
    mut cast_cushion: impl FnMut(V2, V2, f32) -> Option<(f32, V2)>,
) -> Trace {
    let mut trace = Trace { points: vec![origin], ..Trace::default() };
    let (mut from, mut dir, mut budget) = (origin, dir, budget);

    for _ in 0..MAX_SEGMENTS {
        if budget <= 0.0 {
            break;
        }

        let ball = balls
            .iter()
            .filter_map(|&b| {
                let t = ray_circle(from, dir, b, ball_radius * 2.0)?;
                Some((t, b))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .filter(|&(t, _)| t <= budget);
        let pocket = pockets
            .iter()
            .filter_map(|&p| ray_circle(from, dir, p, pocket_radius))
            .min_by(f32::total_cmp)
            .filter(|&t| t <= budget);
        let cushion = cast_cushion(from, dir, budget);

        let ball_t = ball.map_or(f32::INFINITY, |(t, _)| t);
        let pocket_t = pocket.unwrap_or(f32::INFINITY);
        let cushion_t = cushion.map_or(f32::INFINITY, |(t, _)| t);

        if ball_t <= pocket_t && ball_t <= cushion_t {
            if let Some((_, object)) = ball {
                let contact = ball_contact(from, dir, object, ball_radius);
                if let Some(contact) = contact {
                    trace.points.push(contact.ghost);
                    trace.contact = Some(contact);
                    trace.object = Some(object);
                    return trace;
                }
            }
        }

        if pocket_t <= cushion_t && pocket.is_some() {
            trace.points.push(add(from, scale(dir, pocket_t)));
            trace.pocketed = true;
            return trace;
        }

        let Some((t, normal)) = cushion else {
            trace.points.push(add(from, scale(dir, budget)));
            return trace;
        };

        from = add(from, scale(dir, t));
        trace.points.push(from);
        dir = bounce(dir, normal);
        budget = (budget - t) * CUSHION_KEEP;
    }

    trace
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: f32 = 0.84;

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1.0e-4
    }

    #[test]
    fn ray_hits_and_misses() {
        assert!(near(ray_circle([0.0, 0.0], [1.0, 0.0], [10.0, 0.0], 1.0).unwrap(), 9.0));
        assert!(ray_circle([0.0, 0.0], [1.0, 0.0], [10.0, 2.0], 1.0).is_none());
        assert!(ray_circle([0.0, 0.0], [1.0, 0.0], [-10.0, 0.0], 1.0).is_none());
    }

    /// Rails at x = +-10 for the tests below.
    fn walls(from: V2, dir: V2, max: f32) -> Option<(f32, V2)> {
        if dir[0] == 0.0 {
            return None;
        }
        let plane = 10.0 * dir[0].signum();
        let t = (plane - from[0]) / dir[0];
        (t >= 0.0 && t <= max).then_some((t, [-dir[0].signum(), 0.0]))
    }

    #[test]
    fn trace_stops_when_the_budget_runs_out() {
        let t = trace_shot([0.0, 0.0], [1.0, 0.0], 5.0, &[], &[], R, 1.6, walls);
        assert_eq!(t.points.len(), 2);
        assert!(near(t.points[1][0], 5.0));
        assert!(t.contact.is_none());
    }

    #[test]
    fn trace_ends_at_the_first_ball() {
        let balls = [[6.0, 0.0], [3.0, 0.0]];
        let t = trace_shot([0.0, 0.0], [1.0, 0.0], 50.0, &balls, &[], R, 1.6, walls);
        assert_eq!(t.object, Some([3.0, 0.0]));
        assert!(near(t.contact.unwrap().ghost[0], 3.0 - 2.0 * R));
    }

    #[test]
    fn weak_shot_short_of_the_ball_has_no_contact() {
        let balls = [[8.0, 0.0]];
        let t = trace_shot([0.0, 0.0], [1.0, 0.0], 3.0, &balls, &[], R, 1.6, walls);
        assert!(t.contact.is_none());
    }

    #[test]
    fn trace_bounces_off_a_cushion_with_less_budget() {
        let d = normalize([1.0, 1.0]);
        let t = trace_shot([0.0, 0.0], d, 16.0, &[], &[], R, 1.6, walls);
        assert_eq!(t.points.len(), 3);
        assert!(near(t.points[1][0], 10.0));
        // Second leg heads back from the rail.
        assert!(t.points[2][0] < 10.0 && t.points[2][1] > t.points[1][1]);
        let second = dist(t.points[1], t.points[2]);
        let expected = (16.0 - dist([0.0, 0.0], t.points[1])) * CUSHION_KEEP;
        assert!(near(second, expected), "{second} vs {expected}");
    }

    #[test]
    fn trace_stops_in_a_pocket_before_the_rail() {
        let t = trace_shot([0.0, 0.0], [1.0, 0.0], 50.0, &[], &[[10.0, 0.0]], R, 1.6, walls);
        assert!(t.pocketed);
        assert!(near(t.points[1][0], 8.4));
    }

    #[test]
    fn straight_hit_stops_the_cue_ball() {
        let c = ball_contact([0.0, 0.0], [1.0, 0.0], [10.0, 0.0], R).unwrap();
        assert!(near(c.ghost[0], 10.0 - 2.0 * R));
        assert!(near(c.object_dir[0], 1.0) && near(c.object_dir[1], 0.0));
        assert!(near(c.cut_cos, 1.0));
        assert_eq!(c.cue_dir, [0.0, 0.0]);
    }

    #[test]
    fn cut_shot_splits_at_ninety_degrees() {
        // Object offset by one ball radius: sin(cut) = 0.5, a 30 degree cut.
        let c = ball_contact([0.0, 0.0], [1.0, 0.0], [10.0, R], R).unwrap();
        assert!(near(c.cut_cos, 30f32.to_radians().cos()));
        let d = dot(c.object_dir, c.cue_dir);
        assert!(d.abs() < 1.0e-4, "object and cue paths must be perpendicular, got {d}");
        assert!(c.object_dir[1] > 0.0 && c.cue_dir[1] < 0.0);
    }

    #[test]
    fn sixty_degree_cut() {
        // Offset 1.5 radii: sin(cut) = 0.75.
        let c = ball_contact([0.0, 0.0], [1.0, 0.0], [10.0, 1.5 * R], R).unwrap();
        assert!(near(c.cut_cos, (1.0f32 - 0.75 * 0.75).sqrt()));
    }

    #[test]
    fn reflection_flips_the_normal_component() {
        let d = reflect([1.0, 1.0], [0.0, -1.0]);
        assert!(near(d[0], 1.0) && near(d[1], -1.0));
    }

    #[test]
    fn cushion_flattens_the_angle() {
        let d = bounce(normalize([1.0, 1.0]), [-1.0, 0.0]);
        assert!(d[0] < 0.0 && d[1] > 0.0);
        assert!(near(d[1] / -d[0], CUSHION_TANGENT_KEEP));
    }

    #[test]
    fn ghost_for_pocket_sits_behind_the_object() {
        let g = ghost_for_pocket([10.0, 0.0], [20.0, 0.0], R);
        assert!(near(g[0], 10.0 - 2.0 * R) && near(g[1], 0.0));
    }

    #[test]
    fn stopping_distance_matches_the_scene_values() {
        // linear_damp 0.4, rolling resistance 0.1 * 30 = 3, full power:
        // 14 * 1.0 / 0.25 = 56 units/s.
        let d = stopping_distance(56.0, 0.4, 3.0);
        assert!(d > 90.0 && d < 110.0, "full power should roll about 100 units, got {d}");
        assert_eq!(stopping_distance(0.0, 0.4, 3.0), 0.0);
        assert!(stopping_distance(10.0, 0.4, 3.0) < stopping_distance(20.0, 0.4, 3.0));
    }

    #[test]
    fn speed_for_distance_round_trips() {
        for &v in &[5.0f32, 20.0, 56.0] {
            let d = stopping_distance(v, 0.4, 3.0);
            assert!((speed_for_distance(d, 0.4, 3.0) - v).abs() < 0.01);
        }
    }
}
