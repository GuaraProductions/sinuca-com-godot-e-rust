//! Chooses shots for the bot. Pure geometry over `aim_model`, so every rule
//! here can be unit tested without the engine.

use crate::aim_model::{
    V2, dist, dot, ghost_for_pocket, normalize, ray_circle, speed_for_distance, stopping_distance,
    sub,
};

/// Thinnest cut worth trying: cos(75 degrees).
const MIN_CUT_COS: f32 = 0.26;
/// Side pockets face the x axis; a ball arriving flatter than this rattles out.
const MIN_SIDE_ENTRY: f32 = 0.35;
/// Fraction of cue ball speed the object ball receives on a full hit, from the
/// balls' 0.9 bounce: (1 + e) / 2.
const TRANSFER: f32 = 0.95;
/// Extra speed on top of the ideal, so a ball arrives rather than just reaching.
const POWER_MARGIN: f32 = 1.1;
/// A rack this tight around its centre has not been broken yet.
const RACK_RADIUS: f32 = 7.0;
/// How far past the target a fallback shot should carry.
const FALLBACK_CARRY: f32 = 25.0;
/// A cue ball heading into a pocket within this distance risks a scratch.
const SCRATCH_RANGE: f32 = 30.0;
const SCRATCH_RADIUS: f32 = 1.7;

#[derive(Debug, Clone, Copy)]
pub struct PocketInfo {
    pub pos: V2,
    /// Side pockets only accept balls arriving across the table.
    pub side: bool,
}

pub struct Table<'a> {
    pub cue: V2,
    /// Every object ball still in play, targets included.
    pub balls: &'a [V2],
    pub pockets: &'a [PocketInfo],
    pub ball_radius: f32,
    /// Distance from the table centre to the inner rail faces.
    pub half_x: f32,
    pub half_z: f32,
    /// Linear damping and rolling deceleration of a ball.
    pub damp: f32,
    pub decel: f32,
    /// Effective launch speed at full power (see `ROLL_SPEED_FACTOR`).
    pub full_speed: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct Shot {
    pub dir: V2,
    pub power: f32,
    pub score: f32,
}

/// Shortest distance from `p` to the segment `a..b`.
fn segment_distance(a: V2, b: V2, p: V2) -> f32 {
    let ab = sub(b, a);
    let len_sq = dot(ab, ab);
    if len_sq <= f32::EPSILON {
        return dist(a, p);
    }
    let t = (dot(sub(p, a), ab) / len_sq).clamp(0.0, 1.0);
    dist([a[0] + ab[0] * t, a[1] + ab[1] * t], p)
}

impl Table<'_> {
    /// No ball other than `skip` sits within two radii of the segment `a..b`.
    fn path_clear(&self, a: V2, b: V2, skip: Option<V2>, include_cue: bool) -> bool {
        let clearance = self.ball_radius * 2.0;
        let blocked = |p: V2| segment_distance(a, b, p) < clearance;
        let ball_blocks = self.balls.iter().any(|&p| Some(p) != skip && blocked(p));
        !ball_blocks && !(include_cue && blocked(self.cue))
    }

    fn on_table(&self, p: V2) -> bool {
        p[0].abs() <= self.half_x - self.ball_radius && p[1].abs() <= self.half_z - self.ball_radius
    }

    /// Power (may exceed 1) to reach `to_contact` units and still arrive with
    /// enough speed left to carry another `carry` units.
    fn power_to_carry(&self, to_contact: f32, carry: f32) -> f32 {
        let total = to_contact + carry;
        speed_for_distance(total, self.damp, self.decel) * POWER_MARGIN / self.full_speed
    }

    /// A pot attempt on the ball at `object` into `pocket`, if it is playable.
    fn pot_shot(&self, object: V2, pocket: &PocketInfo) -> Option<Shot> {
        let ghost = ghost_for_pocket(object, pocket.pos, self.ball_radius);
        if !self.on_table(ghost) {
            return None;
        }

        let to_ghost = sub(ghost, self.cue);
        let dir = normalize(to_ghost);
        let out = normalize(sub(pocket.pos, object));
        let cut_cos = dot(dir, out);
        if cut_cos < MIN_CUT_COS {
            return None;
        }

        let side_factor = if pocket.side {
            let entry = out[0].abs();
            if entry < MIN_SIDE_ENTRY {
                return None;
            }
            entry
        } else {
            1.0
        };

        if !self.path_clear(self.cue, ghost, Some(object), false)
            || !self.path_clear(object, pocket.pos, Some(object), true)
        {
            return None;
        }

        let d1 = dist(self.cue, ghost);
        let d2 = dist(object, pocket.pos);

        // Work backwards: object ball speed to reach the pocket, then the cue
        // speed at contact to give it that, then the launch speed to arrive.
        let object_speed = speed_for_distance(d2 * 1.25 + 2.0, self.damp, self.decel);
        let contact_speed = object_speed / (TRANSFER * cut_cos);
        let carry = stopping_distance(contact_speed, self.damp, self.decel);
        let power = self.power_to_carry(d1, carry);

        // The cue ball leaves along the tangent; penalise a likely scratch.
        let tangent = {
            let along = dot(dir, out);
            normalize([dir[0] - out[0] * along, dir[1] - out[1] * along])
        };
        let scratches = self.pockets.iter().any(|p| {
            ray_circle(ghost, tangent, p.pos, SCRATCH_RADIUS).is_some_and(|t| t < SCRATCH_RANGE)
        });
        let scratch_factor = if scratches { 0.4 } else { 1.0 };
        let reach_factor = if power > 1.05 { 0.5 } else { 1.0 };

        let score = cut_cos * cut_cos / (1.0 + (d1 + d2) / 50.0)
            * side_factor
            * scratch_factor
            * reach_factor;

        Some(Shot { dir, power: power.clamp(0.2, 1.0), score })
    }

    /// Every playable pot on `targets`, best first.
    pub fn pot_candidates(&self, targets: &[V2]) -> Vec<Shot> {
        let mut shots: Vec<Shot> = targets
            .iter()
            .flat_map(|&object| self.pockets.iter().filter_map(move |p| self.pot_shot(object, p)))
            .collect();
        shots.sort_by(|a, b| b.score.total_cmp(&a.score));
        shots
    }

    /// Nearest ball of `targets` with a straight, unobstructed path.
    fn direct_hit(&self, targets: &[V2]) -> Option<Shot> {
        targets
            .iter()
            .filter(|&&t| self.path_clear(self.cue, t, Some(t), false))
            .min_by(|&&a, &&b| dist(self.cue, a).total_cmp(&dist(self.cue, b)))
            .map(|&t| Shot {
                dir: normalize(sub(t, self.cue)),
                power: self.power_to_carry(dist(self.cue, t), FALLBACK_CARRY).clamp(0.35, 1.0),
                score: 0.0,
            })
    }

    /// Shortest clear one-cushion path to any of `targets`, found by mirroring
    /// the target across each rail.
    fn one_cushion(&self, targets: &[V2]) -> Option<Shot> {
        let x = self.half_x - self.ball_radius;
        let z = self.half_z - self.ball_radius;
        // (axis, plane coordinate): 0 is the x rails, 1 is the z rails.
        let rails = [(0usize, x), (0, -x), (1, z), (1, -z)];

        let mut best: Option<(f32, Shot)> = None;
        for &target in targets {
            for &(axis, plane) in &rails {
                let mut mirrored = target;
                mirrored[axis] = 2.0 * plane - target[axis];

                let span = mirrored[axis] - self.cue[axis];
                if span.abs() <= f32::EPSILON {
                    continue;
                }
                let t = (plane - self.cue[axis]) / span;
                if !(0.0..=1.0).contains(&t) {
                    continue;
                }
                let bounce = [
                    self.cue[0] + (mirrored[0] - self.cue[0]) * t,
                    self.cue[1] + (mirrored[1] - self.cue[1]) * t,
                ];
                let other = 1 - axis;
                let limit = if other == 0 { x } else { z };
                if bounce[other].abs() > limit
                    || !self.path_clear(self.cue, bounce, None, false)
                    || !self.path_clear(bounce, target, Some(target), false)
                {
                    continue;
                }

                let length = dist(self.cue, mirrored);
                if best.is_none_or(|(shortest, _)| length < shortest) {
                    let shot = Shot {
                        dir: normalize(sub(bounce, self.cue)),
                        power: self.power_to_carry(length, FALLBACK_CARRY).clamp(0.35, 1.0),
                        score: 0.0,
                    };
                    best = Some((length, shot));
                }
            }
        }
        best.map(|(_, shot)| shot)
    }

    /// True while the object balls still form the opening rack.
    pub fn is_unbroken_rack(&self, full_rack_size: usize) -> bool {
        if self.balls.len() < full_rack_size {
            return false;
        }
        let count = self.balls.len() as f32;
        let centre = self.balls.iter().fold([0.0, 0.0], |c, p| [c[0] + p[0], c[1] + p[1]]);
        let centre = [centre[0] / count, centre[1] / count];
        self.balls.iter().all(|&p| dist(p, centre) < RACK_RADIUS)
    }

    /// A full-power hit on the nearest ball, to open the rack.
    pub fn break_shot(&self) -> Option<Shot> {
        self.balls
            .iter()
            .min_by(|&&a, &&b| dist(self.cue, a).total_cmp(&dist(self.cue, b)))
            .map(|&nearest| Shot { dir: normalize(sub(nearest, self.cue)), power: 1.0, score: 0.0 })
    }

    /// What to do when no pot is on: touch an own ball, directly or off a rail,
    /// or failing that any ball at all.
    pub fn fallback(&self, targets: &[V2]) -> Option<Shot> {
        self.direct_hit(targets)
            .or_else(|| self.one_cushion(targets))
            .or_else(|| self.direct_hit(self.balls))
            .or_else(|| self.one_cushion(self.balls))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: f32 = 0.84;
    const POCKETS: [PocketInfo; 6] = [
        PocketInfo { pos: [16.8, 33.6], side: false },
        PocketInfo { pos: [-16.8, 33.6], side: false },
        PocketInfo { pos: [16.8, -33.6], side: false },
        PocketInfo { pos: [-16.8, -33.6], side: false },
        PocketInfo { pos: [16.8, 0.0], side: true },
        PocketInfo { pos: [-16.8, 0.0], side: true },
    ];

    fn table<'a>(cue: V2, balls: &'a [V2]) -> Table<'a> {
        Table {
            cue,
            balls,
            pockets: &POCKETS,
            ball_radius: R,
            half_x: 16.8,
            half_z: 33.6,
            damp: 0.4,
            decel: 3.0,
            full_speed: 56.0,
        }
    }

    #[test]
    fn straight_pot_aims_through_the_ball_into_the_pocket() {
        let balls = [[8.0, 0.0]];
        let shots = table([-8.0, 0.0], &balls).pot_candidates(&balls);
        let best = shots.first().expect("a straight side-pocket pot must be found");
        assert!((best.dir[0] - 1.0).abs() < 1e-3, "{:?}", best.dir);
        assert!(best.dir[1].abs() < 1e-3);
        assert!(best.power > 0.0 && best.power <= 1.0);
    }

    #[test]
    fn a_ball_in_the_way_rejects_the_shot() {
        let target = [8.0, 0.0];
        let blocker = [0.0, 0.0];
        let balls = [target, blocker];
        let shots = table([-8.0, 0.0], &balls).pot_candidates(&[target]);
        assert!(
            shots.iter().all(|s| s.dir[0] < 0.999),
            "the straight line through the blocker must not be offered: {shots:?}"
        );
    }

    #[test]
    fn impossible_cuts_are_rejected() {
        // The pocket is behind the object ball as seen from the cue ball.
        let cue = [10.0, 0.0];
        let object = [5.0, 0.0];
        let t = table(cue, std::slice::from_ref(&object));
        assert!(t.pot_shot(object, &POCKETS[5]).is_some(), "the far side pocket is straight on");
        assert!(t.pot_shot(object, &POCKETS[4]).is_none(), "the near side pocket is behind");
    }

    #[test]
    fn side_pockets_refuse_shots_along_the_rail() {
        let cue = [15.0, -20.0];
        let object = [15.0, -5.0];
        let t = table(cue, std::slice::from_ref(&object));
        assert!(t.pot_shot(object, &POCKETS[4]).is_none());
    }

    #[test]
    fn farther_power_is_higher_and_clamped() {
        let near = [[8.0, 0.0]];
        let far = [[8.0, 25.0]];
        let near_power = table([4.0, 0.0], &near).pot_candidates(&near)[0].power;
        let far_power = table([-14.0, -30.0], &far).pot_candidates(&far)[0].power;
        assert!(far_power > near_power);
        assert!(far_power <= 1.0);
    }

    #[test]
    fn unbroken_rack_is_detected() {
        let rack: Vec<V2> = (0..15).map(|i| [(i % 5) as f32 * 1.7 - 3.4, -18.0 - (i / 5) as f32 * 1.7]).collect();
        assert!(table([0.0, 17.0], &rack).is_unbroken_rack(15));

        let mut broken = rack.clone();
        broken[0] = [12.0, 25.0];
        assert!(!table([0.0, 17.0], &broken).is_unbroken_rack(15));
        assert!(!table([0.0, 17.0], &rack[..10]).is_unbroken_rack(15));
    }

    #[test]
    fn fallback_uses_a_cushion_when_the_direct_path_is_blocked() {
        let target = [0.0, -20.0];
        let blocker = [0.0, 0.0];
        let balls = [target, blocker];
        let t = table([0.0, 20.0], &balls);
        assert!(t.direct_hit(&[target]).is_none());
        let shot = t.fallback(&[target]).expect("a one-cushion route exists");
        assert!(shot.dir[0].abs() > 0.05, "must leave the blocked centre line: {:?}", shot.dir);
    }

    #[test]
    fn mirror_shot_really_reaches_the_target() {
        let cue = [0.0, 20.0];
        let target = [0.0, -20.0];
        let balls = [target, [0.0, 0.0]];
        let t = table(cue, &balls);
        let shot = t.one_cushion(&[target]).unwrap();

        // Walk to whichever x rail the shot aims at, reflect, then check the
        // new line passes through the target.
        let plane = (16.8 - R) * shot.dir[0].signum();
        let steps = (plane - cue[0]) / shot.dir[0];
        let bounce = [plane, cue[1] + shot.dir[1] * steps];
        let after = [-shot.dir[0], shot.dir[1]];
        let hit = ray_circle(bounce, after, target, 0.01);
        assert!(hit.is_some(), "reflected path misses the target: {bounce:?} {after:?}");
    }

    #[test]
    fn fallback_is_none_only_when_nothing_is_on_the_table() {
        let t = table([0.0, 0.0], &[]);
        assert!(t.fallback(&[]).is_none());
        assert!(t.break_shot().is_none());
    }
}
