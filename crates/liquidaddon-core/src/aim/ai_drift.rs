//! The drift guard: what the engine does when the model stops agreeing with the curve on the same
//! input.
//!
//! A regression does not err noisily, it errs coherently. While the model is inside the situation
//! it was fitted on, its step and the curve's step over the same delta move together — the Pearson
//! correlation of the two traces over the last [`crate::aim::ai_stats::WINDOW`] ticks stays high
//! (this is the `corr 0.71` of `/liquidaddon ai`). When the fight walks somewhere the training never
//! saw — a distance, an angle, a target speed outside the recorded cloud — the model keeps
//! answering, but its answers stop tracking the geometry of the situation: the correlation decays
//! while the steps still look confident. That is the shape of a model extrapolating, and
//! extrapolation is where an assist either walks into a wall or walks away from the target.
//!
//! The response is deliberately soft, and deliberately temporary:
//!
//!  - **halfway, not over**: below [`MIN_CORR`] the step becomes half model and half curve. The
//!    curve alone would slam the movement back into a geometric law for everyone watching; the
//!    model alone is the thing just caught guessing. Half keeps a human-shaped step with a
//!    geometric spine, and it converges — the curve's share of the delta still closes the angle;
//!  - **for the window's duration**: the correlation is a sliding window, so the blend lasts
//!    exactly as long as the drifted ticks are inside it. As soon as the model answers
//!    in-distribution again, fresh pairs push the correlation back up and the blend releases
//!    itself — no timer to tune, no state to reset;
//!  - **never on a young window**: with fewer pairs than a full window there is no evidence of
//!    drift yet ([`crate::aim::ai_stats::pearson`] answers `NaN` below two pairs, and a fresh
//!    session must not open half-blind), so the guard stays out of the way until it has
//!    [`crate::aim::ai_stats::WINDOW`] pairs to judge.
//!
//! Pure on purpose: the engine reads the correlation from [`crate::aim::ai_stats`] and applies the
//! share this module returns, so the policy is testable without a client and the telemetry line
//! shows the same number the guard acted on.

/// Below this correlation the model is judged to be extrapolating. Not a tight leash:
/// in-distribution play measures well above it (the two engines agree on *when* to move even where
/// they disagree on *how much*), and a healthy window oscillates with the fight. `NaN` — a dead or
/// too young trace — is never below anything.
pub const MIN_CORR: f32 = 0.30;

/// Half model, half curve: the blend below [`MIN_CORR`]. Never zero — see the module docs.
pub const DRIFTED_MODEL_SHARE: f32 = 0.5;

/// The share of the step the model keeps this tick: `1` while the window is young, unreadable
/// (`NaN`) or agreeing (correlation at or above [`MIN_CORR`]); [`DRIFTED_MODEL_SHARE`] while it is
/// drifting. `window_full` is the engine's promise that
/// [`crate::aim::ai_stats::WINDOW`] pairs have been collected — the guard does not act on partial
/// evidence.
pub fn model_share(corr: f32, window_full: bool) -> f32 {
    if !window_full || corr.is_nan() {
        return 1.0;
    }
    if corr < MIN_CORR {
        DRIFTED_MODEL_SHARE
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::{model_share, DRIFTED_MODEL_SHARE, MIN_CORR};
    use crate::aim::ai_stats::{AiStats, Step, WINDOW};

    #[test]
    fn an_agreeing_window_leaves_the_model_alone() {
        assert_eq!(model_share(0.71, true), 1.0);
        assert_eq!(model_share(MIN_CORR, true), 1.0, "the threshold itself is still agreement");
        assert_eq!(model_share(1.0, true), 1.0);
    }

    #[test]
    fn a_drifted_window_takes_half_the_step_back_never_all_of_it() {
        assert_eq!(model_share(0.29, true), DRIFTED_MODEL_SHARE);
        assert_eq!(model_share(0.0, true), DRIFTED_MODEL_SHARE);
        assert_eq!(
            model_share(-0.9, true),
            DRIFTED_MODEL_SHARE,
            "anti-correlation is drift, not a different species"
        );
        assert_eq!(DRIFTED_MODEL_SHARE, 0.5, "halfway is the documented policy");
    }

    #[test]
    fn a_young_or_dead_window_is_not_evidence() {
        assert_eq!(
            model_share(f32::NAN, true),
            1.0,
            "a trace that never moves has no correlation to act on"
        );
        assert_eq!(
            model_share(0.1, false),
            1.0,
            "partial windows do not bend the step, whatever they seem to say"
        );
        assert_eq!(model_share(f32::NAN, false), 1.0);
    }

    #[test]
    fn the_guard_reads_what_the_telemetry_shows() {
        // The pairs of a healthy session: model and curve move together, magnitudes apart.
        let mut stats = AiStats::default();
        for i in 0..WINDOW {
            let i = i as f32;
            stats.note_used(
                1.2 + i * 0.01,
                0.8,
                Step { ai_yaw: 1.0 + i * 0.1, curve_yaw: 2.0 + i * 0.2 },
            );
        }
        let corr = stats.snapshot().corr;
        assert!(
            (corr - 1.0).abs() < 1e-3,
            "the two traces are the same line: corr {corr}"
        );
        assert_eq!(
            model_share(corr, true),
            1.0,
            "corr 1.00 -> the model drives"
        );

        // The same window, but the model now answers against the geometry of the situation.
        stats.reset();
        for i in 0..WINDOW {
            let i = i as f32;
            stats.note_used(
                1.0,
                1.0,
                Step { ai_yaw: 1.0 + i * 0.1, curve_yaw: 2.0 - i * 0.2 },
            );
        }
        let drifted = stats.snapshot().corr;
        assert_eq!(
            model_share(drifted, true),
            DRIFTED_MODEL_SHARE,
            "corr -1.00 -> half curve until the window forgets"
        );
    }
}
