//! Combat modules: what to do with a click, and when.
//!
//! RustClient reimplements selected behaviors from the upstream reference paths under
//! `src/main/kotlin/com/liquidaddon/modules/` (`BlockHit`, `DoubleClicks`, `HitFlick`, `HitSelect`,
//! `TriggerBotPlus`). Pure decisions live here; parts that read the world live in the Horizon bundle.
//!
//! | Module | Core here | Needs from Horizon |
//! |---|---|---|
//! | `DoubleClicks` | [`double_clicks`] | `native_interaction_key` (138) |
//! | `HitSelect` | not implemented | `native_interaction_key` (138), `native_hurt_enemy` (59) |
//! | `BlockHit` | not implemented | `native_interaction_key` (138), `native_use_item` (13) |
//! | `HitFlick` | not implemented | aim core + the rotation path in `PORTING.md` |
//! | `TriggerBotPlus` | not implemented | `native_interaction_key` (138), `native_target_hit` (372) |

pub mod double_clicks;

pub use double_clicks::{DoubleClickScheduler, ExtraClick, extra_click_action, use_extra_allowed};
