//! HUD text: the strings a module shows beside its name.
//!
//! RustClient reimplements selected behavior from `src/main/kotlin/com/liquidaddon/modules/*/`.
//! These are small on purpose — a HUD formatter is the only part of a module that can be tested
//! without a game running, and it is also the part a player reads, so a wrong number there is a lie
//! about what the module is doing.

pub mod jump_reset;

pub use jump_reset::tag;
