//! How much of the model's answer to use, against how far the situation is from its training.
//!
//! A regression answers inside the distribution it was fitted on and keeps answering outside it.
//! The numbers it was trained with say where "inside" is: LiquidBounce recorded `CombatTrainer`
//! slimes at two to three blocks with the crosshair on them, and `Combat` fights inside ten, with a
//! sample written only while the crosshair was closing on the target. A 40° swing at eight blocks
//! is therefore not a hard case for the model, it is a case it has never seen — and a confident
//! wrong answer is worse than the curve, which at least has a known shape.
//!
//! So the answer is weighted by distance and by angle of error: full inside, nothing past the end
//! of the ramp, linear in between. Two independent ramps rather than one product of both, because
//! a close target behind a wide swing and a far target with a small error are two different
//! failures.

/// Everything up to here was in the data: two to six blocks, the range the profiles fight in.
pub const FULL_BLOCKS: f32 = 6.0;

/// Past here the model is guessing; the range slider cannot even hold a target that far.
pub const NONE_BLOCKS: f32 = 8.0;

/// A correction is a degree or two; thirty is a swing the hand does in one tick.
pub const FULL_DEGREES: f32 = 30.0;

/// Past this the aim is not being "helped", it is being pointed somewhere else.
pub const NONE_DEGREES: f32 = 45.0;

/// The share of the model's answer to take this tick, `0..=1`: `blocks` is the distance to the
/// target in blocks, `degrees` the size of the correction it was asked for.
///
/// A `NaN` is not a range: it is a number the loop could not compute, and the model must not be
/// asked to answer for it.
pub fn blend(blocks: f32, degrees: f32) -> f32 {
    let reach =
        ramp(blocks, FULL_BLOCKS, NONE_BLOCKS).min(ramp(degrees, FULL_DEGREES, NONE_DEGREES));
    if reach.is_nan() {
        return 0.0;
    }
    reach.clamp(0.0, 1.0)
}

/// `1` up to `full`, `0` from `none` on, linear in between.
fn ramp(value: f32, full: f32, none: f32) -> f32 {
    if value.is_nan() {
        0.0
    } else if value <= full {
        1.0
    } else if value >= none {
        0.0
    } else {
        1.0 - (value - full) / (none - full)
    }
}

#[cfg(test)]
mod tests {
    use super::blend;

    fn close(expected: f32, actual: f32) {
        assert!(
            (expected - actual).abs() < 1.0e-4,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn the_range_the_profiles_fight_in_is_trusted_whole() {
        assert_eq!(blend(3.4, 2.0), 1.0);
        assert_eq!(blend(6.0, 30.0), 1.0);
    }

    #[test]
    fn a_wide_swing_at_close_range_is_not_the_models_case() {
        // 30..45°: half way is 37.5.
        close(0.5, blend(3.0, 37.5));
        assert_eq!(blend(3.0, 45.0), 0.0);
    }

    #[test]
    fn a_far_target_is_not_either_however_small_the_correction() {
        // 6..8 blocks: half way is 7.
        close(0.5, blend(7.0, 1.0));
        assert_eq!(blend(9.0, 1.0), 0.0);
    }

    #[test]
    fn the_stricter_of_the_two_ramps_wins() {
        // Far (half) and wide (nothing): the answer is nothing, not a product of the two.
        assert_eq!(blend(7.0, 50.0), 0.0);
        close(0.5, blend(7.0, 20.0));
    }

    #[test]
    fn the_ramp_runs_from_one_to_zero_without_a_jump() {
        let steps: Vec<f32> = (0..21).map(|i| blend(6.0 + i as f32 * 0.1, 0.0)).collect();
        assert_eq!(steps[0], 1.0);
        assert_eq!(steps[20], 0.0);
        for pair in steps.windows(2) {
            assert!(pair[1] <= pair[0], "the ramp must not go back up");
            assert!(pair[0] - pair[1] <= 0.051, "the ramp must not jump");
        }
    }

    #[test]
    fn a_number_that_is_not_a_number_is_not_a_range() {
        assert_eq!(blend(f32::NAN, 1.0), 0.0);
        assert_eq!(blend(3.0, f32::NAN), 0.0);
        assert_eq!(blend(f32::INFINITY, 1.0), 0.0);
    }

    #[test]
    fn within_the_training_regime_nothing_is_taken_away() {
        for blocks in [0.0, 1.0, 2.0, 4.0, 6.0] {
            for degrees in [0.0, 1.0, 10.0, 30.0] {
                assert_eq!(
                    blend(blocks, degrees),
                    1.0,
                    "blocks={blocks} degrees={degrees}"
                );
            }
        }
    }
}
