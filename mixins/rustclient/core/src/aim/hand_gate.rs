//! The hand on the mouse, as a gate the assist has to pass.
//!
//! Everything else in the aiming engine shapes *how* a correction looks. This is about whether
//! there is a correction to make at all, and it is the one difference between an assist and an
//! aimbot that no amount of noise can hide:
//!
//!  - a **hand** changes the rotation only while it moves. Stop the mouse and the view stops, and
//!    that unchanged rotation is what the server receives: a normal, boring packet every tick.
//!  - a **module** keeps correcting with the mouse standing still. The target drifts a degree, the
//!    loop answers, and every step in the packets is the module's own — the case a "pattern" or a
//!    "constant rotation" check is built on, and the case [`super::mouse_pattern_guard`] has to
//!    spend its repairs on.
//!
//! So the assist is only granted while the player is actually moving the mouse. The measurement is
//! free and already exact: the aiming core reads the player's own turn every tick
//! (`me.yRot - lastCameraYaw - lastAssistYaw`), i.e. with the module's own contribution removed, so
//! the gate cannot certify itself. Divided by the mouse lattice (LiquidBounce's `RotationUtil.gcd`)
//! that angle becomes **mouse counts**, which is the unit the player's hand actually works in: half
//! a count of movement is not a movement, it is the camera sitting still.
//!
//! The gate does not cut on the last moving tick. A hand that stops still carries the pull for a
//! moment — and a binary on/off edge is itself a shape a check can see — so the activity decays
//! linearly over a few ticks (`fade_ticks`) instead of dropping to zero, and comes straight back to
//! full the moment the mouse moves again.

/// The share of the help that is allowed this tick, `0..=1`.
#[derive(Clone, Copy, Debug, Default)]
pub struct HandGate {
    activity: f32,
}

impl HandGate {
    /// A gate that has not seen a hand yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// What the gate is allowing this tick, `0..=1`. Read by the debug line.
    pub fn activity(&self) -> f32 {
        self.activity
    }

    /// Forgets the hand: nothing is moving yet. Called when the module starts.
    pub fn reset(&mut self) {
        self.activity = 0.0;
    }

    /// One tick of the gate: `movement_counts` is how many mouse counts the player moved,
    /// `threshold` how much counts as a movement, `fade_ticks` how many ticks the pull survives
    /// after the hand stops. Returns the share of the help that is allowed this tick, `0..=1`.
    ///
    /// A `threshold` of zero (or less) switches the gate off entirely: the assist behaves as it did
    /// before this class existed, which is also what the profiles that aim on their own ask for.
    pub fn update(&mut self, movement_counts: f32, threshold: f32, fade_ticks: i32) -> f32 {
        if threshold <= 0.0 {
            self.activity = 1.0;
            return 1.0;
        }
        if movement_counts >= threshold {
            self.activity = 1.0;
            return 1.0;
        }

        // The hand stopped: the pull runs out over `fade_ticks` ticks, never below zero and never
        // in a single step (that step would be the edge this exists to remove).
        self.activity = if fade_ticks <= 0 {
            0.0
        } else {
            (self.activity - 1.0 / fade_ticks as f32).max(0.0)
        };
        self.activity
    }
}

/// How far the player moved the mouse this tick, in whole mouse counts.
///
/// The two axes are added, not compared: a diagonal movement of half a count on each is a mouse
/// that moved, and taking the larger of the two would call it still. A lattice of zero (no `GcdFix`,
/// an unknown sensitivity) means there is no count to measure in, and the gate must not block on a
/// unit it cannot compute: the movement is reported as unbounded.
pub fn movement_counts(own_yaw: f32, own_pitch: f32, gcd: f32) -> f32 {
    if gcd <= 1.0e-6 {
        return f32::INFINITY;
    }
    let counts = (own_yaw.abs() + own_pitch.abs()) / gcd;
    // A NaN (a rotation that came out of nowhere) is not a movement: it has to decay.
    if counts.is_nan() { 0.0 } else { counts }
}

#[cfg(test)]
mod tests {
    use super::{HandGate, movement_counts};

    fn close(expected: f32, actual: f32) {
        assert!(
            (expected - actual).abs() < 1.0e-5,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn a_moving_hand_is_allowed_in_full() {
        let mut gate = HandGate::new();
        assert_eq!(gate.update(2.0, 1.0, 4), 1.0);
        assert_eq!(gate.activity(), 1.0);
    }

    #[test]
    fn a_still_hand_is_refused() {
        let mut gate = HandGate::new();
        assert_eq!(gate.update(0.0, 1.0, 4), 0.0);
    }

    #[test]
    fn the_pull_survives_a_few_ticks_after_the_hand_stops() {
        let mut gate = HandGate::new();
        gate.update(3.0, 1.0, 4);

        // Four ticks of a stopped hand: one quarter of the pull per tick, and zero on the fourth.
        close(0.75, gate.update(0.0, 1.0, 4));
        close(0.50, gate.update(0.0, 1.0, 4));
        close(0.25, gate.update(0.0, 1.0, 4));
        assert_eq!(gate.update(0.0, 1.0, 4), 0.0);
    }

    #[test]
    fn no_fade_means_the_gate_closes_on_the_first_still_tick() {
        let mut gate = HandGate::new();
        gate.update(3.0, 1.0, 0);
        assert_eq!(gate.update(0.0, 1.0, 0), 0.0);
    }

    #[test]
    fn the_hand_coming_back_opens_the_gate_at_once() {
        let mut gate = HandGate::new();
        gate.update(3.0, 1.0, 4);
        gate.update(0.0, 1.0, 4);

        assert_eq!(gate.update(1.0, 1.0, 4), 1.0);
    }

    #[test]
    fn a_threshold_of_zero_switches_the_gate_off() {
        let mut gate = HandGate::new();
        // A module that asked for no gate must keep working with the mouse perfectly still.
        for _ in 0..5 {
            assert_eq!(gate.update(0.0, 0.0, 0), 1.0);
        }
    }

    #[test]
    fn a_movement_under_the_threshold_does_not_open_the_gate() {
        let mut gate = HandGate::new();
        assert_eq!(gate.update(0.9, 1.0, 4), 0.0);
    }

    #[test]
    fn the_decay_never_goes_below_zero() {
        let mut gate = HandGate::new();
        gate.update(3.0, 1.0, 2);
        for _ in 0..10 {
            let share = gate.update(0.0, 1.0, 2);
            assert!(share >= 0.0, "the share went negative: {share}");
        }
        assert_eq!(gate.activity(), 0.0);
    }

    #[test]
    fn reset_forgets_the_hand() {
        let mut gate = HandGate::new();
        gate.update(3.0, 1.0, 4);
        gate.reset();

        assert_eq!(gate.activity(), 0.0);
        assert_eq!(gate.update(0.0, 1.0, 4), 0.0);
    }

    #[test]
    fn movement_is_counted_in_mouse_counts_on_both_axes() {
        // One mouse count at the default sensitivity: the degree value `RotationUtil.gcd` reports.
        let count = 0.15;

        // A diagonal movement of one count on each axis is a movement of two counts.
        close(2.0, movement_counts(count, -count, count));
        close(1.0, movement_counts(0.0, count, count));
        assert_eq!(movement_counts(0.0, 0.0, count), 0.0);
        // Half a count is not a movement on this lattice, but it is still half a count.
        close(0.5, movement_counts(count / 2.0, 0.0, count));
    }

    #[test]
    fn without_a_lattice_nothing_is_measured_and_nothing_is_blocked() {
        // No GcdFix, an unknown sensitivity: there is no count to compare against, so the gate must
        // not be the thing that stops the assist.
        assert_eq!(movement_counts(0.4, 0.1, 0.0), f32::INFINITY);
    }

    #[test]
    fn a_rotation_out_of_nowhere_is_not_a_movement() {
        assert_eq!(movement_counts(f32::NAN, 0.0, 0.15), 0.0);
    }
}
