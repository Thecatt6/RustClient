//! Aim and AI helpers: the parts of `aiming/` that decide, measure or format.
//!
//! The client-free part of `AimEngine.kt` is being ported in layers. The current Math slice owns
//! the rotation and hitbox arithmetic, settings vocabulary, curve step, humanized GCD lattice and
//! cycle guard; Horizon will still supply player/entity snapshots and apply the returned rotation.
//! The trained-model branch and complete target/event lifecycle are later slices (see `PORTING.md`).
//!
//! | Module | Ported from | Shape |
//! |---|---|---|
//! | [`ai_drift`] | `AiDrift.kt` | pure policy: how much of a drifted model to trust |
//! | [`ai_features`] | `AiFeatures.kt` | pure mapping: the model's six inputs |
//! | [`ai_hand`] | `AiHand.kt` | pure policy: what the hand layer keeps under the model |
//! | [`ai_reach`] | `AiReach.kt` | pure policy: how far outside training to trust the model |
//! | [`ai_stats`] | `AiStats.kt` | counters, the correlation window, the `/liquidaddon ai` line |
//! | [`geometry`] | `AimEngine.kt` | hitbox points, crosshair ray and assist bubble |
//! | [`hand_gate`] | `HandGate.kt` | stateful gate: an assist only helps a moving hand |
//! | [`math`] | `AimEngine.kt`, Engine=Math | Bezier/sigmoid curve, speed cap, GCD lattice and hand layer |
//! | [`mouse_pattern_guard`] | `MousePatternGuard.kt` | stateful repair: no cycles in the steps |
//! | [`rotation`] | `AimEngine.kt` | game-compatible rotation/vector arithmetic |
//! | [`settings`] | `AimEngine.kt` | `AimSettings`, tagged values and persisted GUI rotation mode |
//!
//! The split is not cosmetic. Each of these files was already client-free in Kotlin, with its own
//! test file, precisely so the policy could be argued about without a game; the port keeps that
//! property, which is what lets `cargo test` say something true about a module that cannot run
//! outside Horizon.

pub mod ai_drift;
pub mod ai_features;
pub mod ai_hand;
pub mod ai_reach;
pub mod ai_stats;
pub mod geometry;
pub mod hand_gate;
pub mod math;
pub mod mouse_pattern_guard;
pub mod rotation;
pub mod settings;

pub use ai_drift::{DRIFTED_MODEL_SHARE, MIN_CORR, model_share};
pub use ai_features::{INPUT_SIZE, Situation, features, usable, wrap};
pub use ai_hand::Layer;
pub use ai_stats::{AiStats, Fallback, Snapshot, Step, WINDOW};
pub use geometry::{
    Aabb, AimOffset, TargetShape, bubble, clamp_point_with_offset, closest_point,
    crosshair_angle_to_box, hitbox_radius, max_cos_to_box, on_crosshair, point_on, predict_point,
};
pub use hand_gate::HandGate;
pub use math::{HandObservation, HandProfile, MathAimEngine, MathAimInput, MathStep};
pub use mouse_pattern_guard::MousePatternGuard;
pub use rotation::{Rotation, Vec3};
pub use settings::{
    AimConfig, AimSettings, ROTATION_MODE_KEY, RotationMode, SettingStore, load_rotation_mode,
    save_rotation_mode,
};
