//! Combat modules: what to do with a click, and when.
//!
//! Ported from `src/main/kotlin/com/liquidaddon/modules/` (`BlockHit`, `DoubleClicks`, `HitFlick`,
//! `HitSelect`, `TriggerBotPlus`). The parts that are decisions and not game calls live here; the
//! parts that read the world live in the Horizon bundle.
//!
//! | Module | Core here | Needs from Horizon |
//! |---|---|---|
//! | `DoubleClicks` | [`double_clicks`] | `native_interaction_key` (138) |
//! | `HitSelect` | to port | `native_interaction_key` (138), `native_hurt_enemy` (59) |
//! | `BlockHit` | to port | `native_interaction_key` (138), `native_use_item` (13) |
//! | `HitFlick` | to port | aim core + the rotation path in `PORTING.md` |
//! | `TriggerBotPlus` | to port | `native_interaction_key` (138), `native_target_hit` (372) |

pub mod double_clicks;

pub use double_clicks::{extra_click_action, use_extra_allowed, DoubleClickScheduler, ExtraClick};
