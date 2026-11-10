//! Game-independent core of the LiquidAddon port.
//!
//! Everything in this crate is a direct translation of a Kotlin file of
//! [`Thecatt6/LiquidAddon`](https://github.com/Thecatt6/LiquidAddon) at the revision pinned in
//! `compatibility.json`, and nothing here names a LiquidBounce type, a Horizon type or a
//! Minecraft class. That is deliberate: the port is split in two halves, and this is the half
//! that can be compiled and tested without a game running.
//!
//! * **This crate** holds the decisions: when a follow-up click may fire, how much of an AI
//!   answer to trust, when a sequence of mouse steps has become a pattern, what a HUD line says.
//!   Every rule keeps the Kotlin tests it came with, so a behaviour change shows up as a failing
//!   test rather than as a difference nobody notices until a fight.
//! * **The Horizon bundle** (not in this crate) supplies the numbers and performs the effects:
//!   it reads the player's rotation, the target, the item in hand, and it calls the hooks or
//!   source mixins listed in `PORTING.md`. It owns no policy of its own, so a hook that changes
//!   shape with a Horizon update cannot silently change what the module decides.
//!
//! # Layout
//!
//! | Module | Ported from | Kind |
//! |---|---|---|
//! | [`aim::ai_drift`] | `aiming/AiDrift.kt` | pure policy |
//! | [`aim::ai_features`] | `aiming/AiFeatures.kt` | pure mapping |
//! | [`aim::ai_hand`] | `aiming/AiHand.kt` | pure policy |
//! | [`aim::ai_reach`] | `aiming/AiReach.kt` | pure policy |
//! | [`aim::ai_stats`] | `aiming/AiStats.kt` | stateful counters + formatter |
//! | [`aim::hand_gate`] | `aiming/HandGate.kt` | stateful gate |
//! | [`aim::mouse_pattern_guard`] | `aiming/MousePatternGuard.kt` | stateful repair |
//! | [`combat::double_clicks`] | `modules/doubleclicks/DoubleClickScheduler.kt` | stateful scheduler |
//! | [`hud::jump_reset`] | `modules/jumpreset/JumpResetHud.kt` | pure formatter |
//!
//! `AimEngine.kt` (2620 lines), the module files and the model/telemetry helpers are still to be
//! ported; see `PORTING.md` for the order and for the hook each one needs on the Horizon side.

#![forbid(unsafe_code)]

pub mod aim;
pub mod combat;
pub mod hud;
pub mod rng;

pub use rng::SplitMix64;
