//! Game-independent core of the LiquidAddon port.
//!
//! The client-free policies here are ported from the pinned Kotlin source, and the Math slice
//! re-expresses the relevant part of `AimEngine.kt` without Horizon, LiquidBounce or Minecraft
//! types. This crate is nested inside the Horizon bundle so horizon-mixins can stage it without an
//! out-of-bundle path dependency; the repository root Cargo workspace still tests it independently
//! of the game.
//!
//! * **This crate** holds decisions and arithmetic: aim geometry and curves, AI policy helpers,
//!   mouse gates and pattern repair, click scheduling, settings vocabulary and HUD formatting.
//! * **The Horizon bundle** supplies live player/entity snapshots, persistent storage and GUI hooks,
//!   then applies actions through Horizon hooks or pinned source mixins. The gameplay aim controller
//!   and rotation-delivery hook are not implemented yet.
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
//! | [`aim::geometry`] | `AimEngine.kt` | client-free hitbox and crosshair geometry |
//! | [`aim::hand_gate`] | `aiming/HandGate.kt` | stateful gate |
//! | [`aim::math`] | `AimEngine.kt`, `Engine = Math` | curve, lattice, hand layer and cycle guard |
//! | [`aim::mouse_pattern_guard`] | `aiming/MousePatternGuard.kt` | stateful repair |
//! | [`aim::rotation`] | `AimEngine.kt` | game-compatible rotation/vector arithmetic |
//! | [`aim::settings`] | `AimEngine.kt` | settings values and runtime rotation mode |
//! | [`combat::double_clicks`] | `modules/doubleclicks/DoubleClickScheduler.kt` | stateful scheduler |
//! | [`hud::jump_reset`] | `modules/jumpreset/JumpResetHud.kt` | pure formatter |
//!
//! The trained-model branch, complete target/event lifecycle, live Horizon adapter, other module
//! behavior and commands remain to be ported; see the repository's `PORTING.md` for the plan.

#![forbid(unsafe_code)]

pub mod aim;
pub mod combat;
pub mod hud;
pub mod rng;

pub use rng::SplitMix64;
