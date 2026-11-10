//! The model's input, written down once — the six numbers `CombatSample.fillAsInput`
//! (LiquidBounce v0.41.0) hands to every bundled combat model, in the order the training used:
//!
//! ```text
//! [0] deltaYaw   (current -> target, wrapped)     [3] deltaPitch (previous -> current)
//! [1] deltaPitch (current -> target, wrapped)     [4] speed: |playerDiff|xz + |targetDiff|xz
//! [2] deltaYaw   (previous -> current, wrapped)   [5] squared distance to the target, in blocks²
//! ```
//!
//! Pure on purpose, and the mirror of `configs/ai_eval.py` in the source repository: the offline
//! evaluation fits on exactly these features, so what the evaluation measures and what the engine
//! feeds the model cannot drift apart unnoticed — a change of order breaks these tests and the
//! evaluation together.
//!
//! The engine does not re-implement the forward input with this (LiquidBounce's `CombatSample` stays
//! the builder); it uses [`usable`] as the guard that keeps a corrupt vector — a `NaN` out of a
//! degenerate rotation, an `Inf` out of a distance — from ever reaching `predict`, and the tests pin
//! the order this module documents.

/// Bundled combat models take six floats (`CombatSample.INPUT_SIZE`).
pub const INPUT_SIZE: usize = 6;

/// Degrees to `(-180, 180]`, what `Mth.wrapDegrees` / `Rotation.angleDifference` do.
pub fn wrap(degrees: f32) -> f32 {
    ((degrees + 180.0) % 360.0 + 360.0) % 360.0 - 180.0
}

/// The three rotations of the tick (current, previous, target), the two horizontal movement diffs
/// and the squared distance. Named fields instead of eleven positional arguments: the Kotlin
/// version relies on its parameter names at every call site already.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Situation {
    pub current_yaw: f32,
    pub current_pitch: f32,
    pub previous_yaw: f32,
    pub previous_pitch: f32,
    pub target_yaw: f32,
    pub target_pitch: f32,
    pub player_diff_x: f32,
    pub player_diff_z: f32,
    pub target_diff_x: f32,
    pub target_diff_z: f32,
    pub squared_distance: f32,
}

/// The six features, in the order the training used — or `None` when any of them is not a finite
/// number, because a model asked about a corrupt situation answers with a corrupt step.
pub fn features(situation: &Situation) -> Option<[f32; INPUT_SIZE]> {
    let s = situation;
    let out = [
        wrap(s.target_yaw - s.current_yaw),
        wrap(s.target_pitch - s.current_pitch),
        wrap(s.current_yaw - s.previous_yaw),
        wrap(s.current_pitch - s.previous_pitch),
        s.player_diff_x.hypot(s.player_diff_z) + s.target_diff_x.hypot(s.target_diff_z),
        s.squared_distance,
    ];
    if usable(&out) { Some(out) } else { None }
}

/// True when `input` is something a forward pass may be asked about: six slots, all finite.
pub fn usable(input: &[f32]) -> bool {
    input.len() >= INPUT_SIZE && input[..INPUT_SIZE].iter().all(|v| v.is_finite())
}

#[cfg(test)]
mod tests {
    use super::{features, usable, wrap, Situation, INPUT_SIZE};

    fn situation() -> Situation {
        Situation {
            current_yaw: 0.0,
            current_pitch: 0.0,
            previous_yaw: 0.0,
            previous_pitch: 0.0,
            target_yaw: 0.0,
            target_pitch: 0.0,
            player_diff_x: 0.0,
            player_diff_z: 0.0,
            target_diff_x: 0.0,
            target_diff_z: 0.0,
            squared_distance: 1.0,
        }
    }

    fn close(expected: f32, actual: f32, what: &str) {
        assert!((expected - actual).abs() < 1.0e-4, "{what}: expected {expected}, got {actual}");
    }

    #[test]
    fn the_six_slots_are_the_documented_ones_in_the_documented_order() {
        // current looks at yaw 10 / pitch 0, one tick ago it looked at yaw 5 / pitch -2, the goal
        // is yaw 20 / pitch 3; the player moved (0.3, 0.4) horizontally, the target did not move,
        // the squared distance is 9.
        let f = features(&Situation {
            current_yaw: 10.0,
            current_pitch: 0.0,
            previous_yaw: 5.0,
            previous_pitch: -2.0,
            target_yaw: 20.0,
            target_pitch: 3.0,
            player_diff_x: 0.3,
            player_diff_z: 0.4,
            target_diff_x: 0.0,
            target_diff_z: 0.0,
            squared_distance: 9.0,
        })
        .expect("a finite situation has features");

        assert_eq!(f.len(), INPUT_SIZE);
        close(10.0, f[0], "deltaYaw current->target");
        close(3.0, f[1], "deltaPitch current->target");
        close(5.0, f[2], "deltaYaw previous->current");
        close(2.0, f[3], "deltaPitch previous->current");
        close(0.5, f[4], "speed: horizontal player diff plus horizontal target diff");
        close(9.0, f[5], "squared distance, already squared");
    }

    #[test]
    fn deltas_cross_the_meridian_the_way_wrap_degrees_does() {
        let f = features(&Situation {
            current_yaw: 179.0,
            previous_yaw: -179.0,
            target_yaw: -179.0,
            squared_distance: 4.0,
            ..situation()
        })
        .expect("a finite situation has features");

        close(2.0, f[0], "-179 is two degrees east of 179, not 358 west");
        close(-2.0, f[2], "and the previous step wraps the same way");
        assert_eq!(wrap(-358.0), 2.0);
        assert_eq!(wrap(180.0), -180.0);
    }

    #[test]
    fn the_speed_is_horizontal_only_and_adds_both_movers() {
        let f = features(&Situation {
            player_diff_x: 0.0,
            player_diff_z: -0.2,
            target_diff_x: 0.07,
            target_diff_z: 0.24,
            ..situation()
        })
        .expect("a finite situation has features");

        // |playerDiff| xz = 0.2, |targetDiff| xz = 0.25 (the 7/24/25 triple): vertical never counts.
        close(0.45, f[4], "speed");
    }

    #[test]
    fn a_corrupt_situation_has_no_features_at_all() {
        assert_eq!(
            features(&Situation { current_yaw: f32::NAN, ..situation() }),
            None,
            "a NaN rotation must not become an input"
        );
        assert_eq!(
            features(&Situation { squared_distance: f32::INFINITY, ..situation() }),
            None,
            "an infinite distance must not become an input"
        );
    }

    #[test]
    fn usable_is_the_guard_the_engine_puts_in_front_of_predict() {
        assert!(usable(&[0.0; 6]));
        assert!(!usable(&[0.0; 5]), "a short vector is not six features");
        assert!(
            !usable(&[0.0, 0.0, 0.0, 0.0, 0.0, f32::NAN]),
            "one NaN poisons the whole vector"
        );
    }
}
