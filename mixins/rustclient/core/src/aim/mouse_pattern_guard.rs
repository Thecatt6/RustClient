//! The steps that actually leave, read as a *sequence* instead of one at a time.
//!
//! The aiming core's quantiser already makes every single step look like a mouse: whole counts, a
//! hand's rounding, counts of noise on top. What none of that can fix is the shape the sequence
//! takes over time, because the sequence is not chosen by the noise — it is chosen by the control
//! loop underneath:
//!
//!  - an aim that has arrived is *on* the target, so every tick asks for the same correction, the
//!    quantiser rounds it to the same count and the packets carry `+1, +1, +1, ...`;
//!  - an aim that is one count off alternates: the correction pushes it over, the next one pulls it
//!    back, and the packets carry `+1, -1, +1, -1, ...` for as long as the target stands there.
//!
//! Both are cycles, and a cycle is exactly what a "pattern" check is built to find — it does not
//! need to know how the numbers were produced, only that they keep coming back. The assist is the
//! worse case of the two aiming modules: it holds the crosshair *on* the target for whole seconds
//! (that is its job), while the aimbot is usually in the middle of a flick, and when the player is
//! not moving the mouse at all the packets are the module's alone, with no hand mixed in.
//!
//! This guard watches the last [`HISTORY`] steps the engine handed out and refuses to append one
//! that would close a cycle of up to [`MAX_PERIOD`] steps repeated [`CYCLES`] times. When that is
//! about to happen the step is nudged by a count (never across the goal: a step that was going
//! right is not turned into one going left), and the count it was nudged by is written down as a
//! debt. The debt is paid back on a later step, in the same direction and one count smaller, so the
//! path adds up to what the engine planned and only the digits differ. A hand that is still (`0, 0`
//! for ticks on end) is left alone: resting is not a pattern, it is what a crosshair does when
//! nobody moves.

use crate::rng::SplitMix64;

/// Steps of history the guard keeps; enough for [`CYCLES`] cycles of [`MAX_PERIOD`] steps.
pub const HISTORY: usize = 24;

/// Longest cycle the guard looks for: six steps is a quarter of a second of packet history.
pub const MAX_PERIOD: usize = 6;

/// How many times a window has to repeat before it counts as a cycle rather than a coincidence.
pub const CYCLES: usize = 3;

/// The largest nudge a step may take to break a cycle, in counts.
pub const MAX_NUDGE: i32 = 2;

/// The largest debt the path may carry, in counts: a repair is never paid back for long.
pub const MAX_OWED: i32 = 2;

/// Chance that a step pays back what an earlier repair owes, so the payback has no beat.
pub const PAY_CHANCE: f32 = 0.65;

/// The step that is not a movement: both axes at rest.
const REST: u64 = 0;

/// Watches the outgoing step sequence and repairs the cycles in it.
#[derive(Debug)]
pub struct MousePatternGuard {
    history: usize,
    max_period: usize,
    rng: SplitMix64,
    /// The steps handed out, newest at `cursor`; two counts packed into one value.
    ring: Vec<u64>,
    cursor: usize,
    filled: usize,
    /// What a repair owes the path, in counts, per axis. Positive means the engine is one count
    /// ahead of where it wanted to be and the next steps must be smaller; negative is the opposite.
    owed_yaw: i32,
    owed_pitch: i32,
    repairs: u32,
}

impl MousePatternGuard {
    /// A guard with the shipped history and period, driven by `seed`.
    pub fn new(seed: u64) -> Self {
        Self::with_options(HISTORY, MAX_PERIOD, seed)
    }

    /// A guard with an explicit window, for tests that want to read the ring.
    ///
    /// # Panics
    ///
    /// If `history` cannot hold [`CYCLES`] full cycles of `max_period` steps, or `max_period` is
    /// zero: a window that cannot hold a cycle can never detect one.
    pub fn with_options(history: usize, max_period: usize, seed: u64) -> Self {
        assert!(
            max_period > 0 && history >= max_period * CYCLES,
            "history ({history}) must hold {CYCLES} full cycles of up to {max_period} steps"
        );
        Self {
            history,
            max_period,
            rng: SplitMix64::new(seed),
            ring: vec![0; history],
            cursor: 0,
            filled: 0,
            owed_yaw: 0,
            owed_pitch: 0,
            repairs: 0,
        }
    }

    /// Steps this guard changed since the last [`reset`](Self::reset); the debug line shows it.
    pub fn repairs(&self) -> u32 {
        self.repairs
    }

    /// Forgets the history and the debts.
    pub fn reset(&mut self) {
        self.ring.iter_mut().for_each(|slot| *slot = 0);
        self.cursor = 0;
        self.filled = 0;
        self.owed_yaw = 0;
        self.owed_pitch = 0;
        self.repairs = 0;
    }

    /// Returns the step to send in place of `yaw`/`pitch` (whole mouse counts, per axis).
    ///
    /// Called on every step that reaches the lattice, i.e. once per tick while the assist is
    /// correcting. Cheap on purpose: no allocation, at most eight candidates tried.
    pub fn guard(&mut self, yaw: i32, pitch: i32) -> (i32, i32) {
        // 1. Pay what an earlier repair owes. Not on the very next tick every time — a debt paid
        //    back on a fixed beat is itself a beat — and never by turning a step around: the aim
        //    still has to go where the engine pointed it.
        let mut y = yaw;
        let mut p = pitch;
        if self.owed_yaw != 0 && y != 0 && self.rng.next_f32() < PAY_CHANCE {
            let paid = settle(y, self.owed_yaw);
            self.owed_yaw -= paid - y;
            y = paid;
        }
        if self.owed_pitch != 0 && p != 0 && self.rng.next_f32() < PAY_CHANCE {
            let paid = settle(p, self.owed_pitch);
            self.owed_pitch -= paid - p;
            p = paid;
        }

        // 2. The candidate takes its slot before it is tested, so the test reads it as the newest
        //    step of a window that is always in the right order.
        self.ring[self.cursor] = pack(y, p);
        let period = self.cycle();
        if period > 0
            && !(self.ring[self.cursor] == REST && self.cycle_is_rest(period))
            && let Some((fixed_yaw, fixed_pitch)) = self.break_cycle(y, p)
        {
            self.owed_yaw = (self.owed_yaw + (y - fixed_yaw)).clamp(-MAX_OWED, MAX_OWED);
            self.owed_pitch = (self.owed_pitch + (p - fixed_pitch)).clamp(-MAX_OWED, MAX_OWED);
            y = fixed_yaw;
            p = fixed_pitch;
            self.ring[self.cursor] = pack(y, p);
            self.repairs += 1;
        }

        self.cursor = (self.cursor + 1) % self.history;
        if self.filled < self.history {
            self.filled += 1;
        }
        (y, p)
    }

    /// The smallest cycle length the window closes, or zero when it closes none.
    fn cycle(&self) -> usize {
        let depth = (self.filled + 1).min(self.history);
        for period in 1..=self.max_period {
            let need = period * CYCLES;
            if depth < need {
                continue;
            }
            let mut same = true;
            let mut i = 0;
            while i < need - period {
                if self.sample(i) != self.sample(i + period) {
                    same = false;
                    break;
                }
                i += 1;
            }
            if same {
                return period;
            }
        }
        0
    }

    /// True when the whole window of `period` is `0, 0`: a hand at rest, which is not a pattern.
    fn cycle_is_rest(&self, period: usize) -> bool {
        for i in 0..period * CYCLES {
            if self.sample(i) != REST {
                return false;
            }
        }
        true
    }

    fn sample(&self, steps_back: usize) -> u64 {
        let mut index = self.cursor + self.history - steps_back % self.history;
        index %= self.history;
        self.ring[index]
    }

    /// One count off the step, on one axis only: first the axis the engine is busier on, and on both
    /// directions of it before the other axis is touched. The change that breaks the cycle is
    /// taken, so the sequence never falls back into the same window by being repaired "the usual
    /// way" — that is the trap this whole class exists to avoid, and a fixed repair would be a
    /// pattern of its own.
    ///
    /// `None` when no nudge of one or two counts closes the window (a step of one count on both
    /// axes, with every neighbour taken), in which case the step is left exactly as planned.
    fn break_cycle(&mut self, yaw: i32, pitch: i32) -> Option<(i32, i32)> {
        let yaw_first = yaw.abs() >= pitch.abs();
        for amount in 1..=MAX_NUDGE {
            for pass in 0..2 {
                let on_yaw = if pass == 0 { yaw_first } else { !yaw_first };
                let sign = if self.rng.next_bool() { 1 } else { -1 };
                if on_yaw {
                    let Some(candidate) = nudge(yaw, sign, amount) else {
                        continue;
                    };
                    if self.accepts(candidate, pitch) {
                        return Some((candidate, pitch));
                    }
                } else {
                    let Some(candidate) = nudge(pitch, sign, amount) else {
                        continue;
                    };
                    if self.accepts(yaw, candidate) {
                        return Some((yaw, candidate));
                    }
                }
            }
        }
        None
    }

    /// True when `yaw`/`pitch` would not close the window it is about to be appended to.
    fn accepts(&mut self, yaw: i32, pitch: i32) -> bool {
        let kept = self.ring[self.cursor];
        self.ring[self.cursor] = pack(yaw, pitch);
        let period = self.cycle();
        self.ring[self.cursor] = kept;
        period == 0
    }
}

/// The nudge, unless it would turn the step around: an aim never walks away from its goal.
fn nudge(counts: i32, sign: i32, amount: i32) -> Option<i32> {
    let nudged = counts + sign * amount;
    if counts > 0 && nudged < 0 {
        return None;
    }
    if counts < 0 && nudged > 0 {
        return None;
    }
    Some(nudged)
}

/// The step after a debt of `owed` counts, kept on the same side of zero as it was.
fn settle(step: i32, owed: i32) -> i32 {
    let settled = step + owed;
    if step > 0 {
        settled.max(0)
    } else {
        settled.min(0)
    }
}

fn pack(yaw: i32, pitch: i32) -> u64 {
    ((yaw as i64 as u64) << 32) | (pitch as i64 as u64 & 0xFFFF_FFFF)
}

#[cfg(test)]
mod tests {
    use super::MousePatternGuard;

    /// True when `steps` contains a window that repeats `cycles` times with a period of
    /// `max_period` or less — the shape an anticheat calls a pattern.
    fn closes_a_cycle(steps: &[(i32, i32)], max_period: usize, cycles: usize) -> bool {
        for period in 1..=max_period {
            let need = period * cycles;
            if steps.len() < need {
                continue;
            }
            for end in need..=steps.len() {
                let window = &steps[end - need..end];
                let repeating = (0..need - period).all(|i| window[i] == window[i + period]);
                if repeating && window.iter().any(|step| *step != (0, 0)) {
                    return true;
                }
            }
        }
        false
    }

    #[test]
    fn a_two_value_alternation_is_broken() {
        let mut guard = MousePatternGuard::new(1);
        let out: Vec<(i32, i32)> = (0..40)
            .map(|tick| guard.guard(if tick % 2 == 0 { 1 } else { -1 }, 0))
            .collect();

        assert!(
            !closes_a_cycle(&out, 6, 3),
            "the guard left a period cycle in {out:?}"
        );
        assert!(guard.repairs() > 0, "the guard never had to do anything");
        // The path is what was planned: a repair is paid back, it is not a detour.
        let drift: i32 = out.iter().map(|(yaw, _)| *yaw).sum();
        assert!(drift.abs() <= 3, "the aim drifted off the path: {out:?}");
    }

    #[test]
    fn a_constant_step_is_broken() {
        let mut guard = MousePatternGuard::new(1);
        let out: Vec<(i32, i32)> = (0..60).map(|_| guard.guard(3, 1)).collect();

        assert!(
            !closes_a_cycle(&out, 6, 3),
            "the guard left a period cycle in {out:?}"
        );
        assert!(guard.repairs() > 0);
        assert_eq!(
            out.iter().map(|(yaw, _)| *yaw).sum::<i32>(),
            180,
            "sixty three-count steps still have to be 180 counts"
        );
        assert_eq!(out.iter().map(|(_, pitch)| *pitch).sum::<i32>(), 60);
    }

    #[test]
    fn a_hand_at_rest_is_left_alone() {
        let mut guard = MousePatternGuard::new(1);
        for _ in 0..40 {
            assert_eq!(guard.guard(0, 0), (0, 0));
        }
        assert_eq!(
            guard.repairs(),
            0,
            "resting is not a pattern: nothing may be invented out of it"
        );
    }

    #[test]
    fn a_varied_sequence_is_not_touched() {
        let mut guard = MousePatternGuard::new(1);
        let planned = [
            (5, 2),
            (4, 1),
            (6, -2),
            (3, 1),
            (2, 0),
            (5, -1),
            (1, 2),
            (7, 1),
            (4, -3),
            (2, 1),
            (3, 2),
            (6, -1),
            (5, 1),
            (4, 0),
            (2, -2),
            (3, 1),
        ];
        let out: Vec<(i32, i32)> = planned.iter().map(|(y, p)| guard.guard(*y, *p)).collect();
        assert_eq!(
            out, planned,
            "a sequence that never repeats was repaired anyway"
        );
        assert_eq!(guard.repairs(), 0);
    }

    #[test]
    fn a_step_is_never_turned_around() {
        let mut guard = MousePatternGuard::new(1);
        // The assist pulling one count at a time, for four seconds: the worst case for a cycle.
        let out: Vec<(i32, i32)> = (0..80).map(|_| guard.guard(1, 0)).collect();

        assert!(
            out.iter().all(|(yaw, _)| *yaw >= 0),
            "a repair walked the aim backwards: {out:?}"
        );
        assert!(!closes_a_cycle(&out, 6, 3));
        assert_eq!(
            out.iter().map(|(yaw, _)| *yaw).sum::<i32>(),
            80,
            "the total travel must not change"
        );
    }

    #[test]
    fn the_assist_control_loop_stops_closing_cycles() {
        // The loop the module really runs: an error, a pull that is a share of it, the lattice, the
        // step taken off the error. The target keeps sliding away, so the loop never settles — and
        // without the guard it locks into a two or three tick cycle within a few seconds.
        let mut guard = MousePatternGuard::new(1);
        let mut error = 2.0_f64;
        let mut out = Vec::new();
        for _ in 0..300 {
            error += 0.4;
            let wanted = ((error * 0.35).round() as i32).min(8);
            let step = guard.guard(wanted, 0);
            out.push(step);
            error -= f64::from(step.0);
        }

        assert!(
            !closes_a_cycle(&out, 6, 3),
            "the loop still closes a cycle: {out:?}"
        );
        let distinct: std::collections::BTreeSet<i32> = out.iter().map(|(yaw, _)| *yaw).collect();
        assert!(distinct.len() >= 4, "too few distinct steps left: {out:?}");
        assert!(error.abs() < 4.0, "the aim lost the target: error={error}");
    }

    #[test]
    fn reset_forgets_the_history() {
        let mut guard = MousePatternGuard::new(1);
        for _ in 0..20 {
            guard.guard(2, 0);
        }
        assert!(guard.repairs() > 0);

        guard.reset();

        assert_eq!(guard.repairs(), 0);
        // A fresh guard sees no history, so the same step it just refused is planned again.
        let mut fresh = MousePatternGuard::new(1);
        assert_eq!(fresh.guard(2, 0), guard.guard(2, 0));
    }
}
