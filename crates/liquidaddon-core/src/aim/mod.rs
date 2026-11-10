//! Aim and AI helpers: the parts of `aiming/` that decide, measure or format.
//!
//! The heavy file of the source, `AimEngine.kt` (2620 lines), is not ported yet: it is a rotation
//! generator that mixes prediction, a fitted model, a hand lattice and telemetry, and it cannot be
//! translated honestly before its Horizon-side inputs exist (see `PORTING.md`, "Aim and shared AI").
//! What is ported here is everything that could be separated from it, which is also everything a
//! player notices when it is wrong:
//!
//! | Module | Ported from | Shape |
//! |---|---|---|
//! | [`ai_drift`] | `AiDrift.kt` | pure policy: how much of a drifted model to trust |
//! | [`ai_features`] | `AiFeatures.kt` | pure mapping: the model's six inputs |
//! | [`ai_hand`] | `AiHand.kt` | pure policy: what the hand layer keeps under the model |
//! | [`ai_reach`] | `AiReach.kt` | pure policy: how far outside training to trust the model |
//! | [`ai_stats`] | `AiStats.kt` | counters, the correlation window, the `/liquidaddon ai` line |
//! | [`hand_gate`] | `HandGate.kt` | stateful gate: an assist only helps a moving hand |
//! | [`mouse_pattern_guard`] | `MousePatternGuard.kt` | stateful repair: no cycles in the steps |
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
pub mod hand_gate;
pub mod mouse_pattern_guard;

pub use ai_drift::{model_share, DRIFTED_MODEL_SHARE, MIN_CORR};
pub use ai_features::{features, usable, wrap, Situation, INPUT_SIZE};
pub use ai_hand::Layer;
pub use ai_stats::{AiStats, Fallback, Snapshot, Step, WINDOW};
pub use hand_gate::HandGate;
pub use mouse_pattern_guard::MousePatternGuard;
