//! Testable RustClient logic modeled on the upstream `DoubleClicks` behavior
//! (`modules/doubleclicks/DoubleClickScheduler.kt`).
//!
//! The module answers each real click with up to three delayed follow-ups, and the whole difficulty
//! is not "click more" but *when*: a follow-up must never ride the input batch that caused it, and
//! a player holding attack must not accumulate a burst that keeps firing seconds after they stop.
//! Both are decisions about a queue, so both live here, independent of LiquidBounce and of the
//! game, and the Horizon side only supplies the two facts each decision needs (is the crosshair on
//! an entity, is the game already mining or inside the miss cooldown).
//!
//! The follow-up itself goes through the vanilla path: on Horizon that is the
//! `native_interaction_key` hook (API 138), which is cancellable but whose cancelled attack still
//! swings unless the swing is cleared too — see `PORTING.md` for the exact contract.

use std::collections::VecDeque;

/// A delayed follow-up: attack through the vanilla path, use through the
/// vanilla path, or only play the swing animation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtraClick {
    Attack,
    Use,
    SwingOnly,
}

/// Keeps follow-ups out of the input batch that caused them and bounds the
/// backlog so fast originals cannot leave clicks firing long after input stops.
#[derive(Debug, Default)]
pub struct DoubleClickScheduler {
    pending: VecDeque<ExtraClick>,
    saw_original_click: bool,
    ticks_until_next: u32,
}

impl DoubleClickScheduler {
    pub const MAX_EXTRA_CLICKS: usize = 3;

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Record one real click and its requested extras. The queue is capped at
    /// three; recording the original always postpones a queued extra this tick.
    pub fn record_original(&mut self, extra_clicks: i32, click: ExtraClick) {
        self.saw_original_click = true;
        let count = extra_clicks.clamp(0, Self::MAX_EXTRA_CLICKS as i32) as usize;
        for _ in 0..count {
            if self.pending.len() < Self::MAX_EXTRA_CLICKS {
                self.pending.push_back(click);
            }
        }
    }

    /// Called once after the game's real input handling for a tick.
    pub fn take_next(&mut self, delay_ticks: i32) -> Option<ExtraClick> {
        let delay = delay_ticks.max(1) as u32;
        if self.saw_original_click {
            self.saw_original_click = false;
            self.ticks_until_next = delay;
            return None;
        }
        if self.pending.is_empty() {
            self.ticks_until_next = 0;
            return None;
        }
        self.ticks_until_next = self.ticks_until_next.saturating_sub(1);
        if self.ticks_until_next > 0 {
            return None;
        }

        self.ticks_until_next = delay;
        self.pending.pop_front()
    }

    pub fn clear(&mut self) {
        self.pending.clear();
        self.saw_original_click = false;
        self.ticks_until_next = 0;
    }
}

/// Repeat an attack only when the refreshed crosshair still targets an entity
/// and neither block mining nor vanilla's miss cooldown blocks the attack.
pub fn extra_click_action(
    planned: ExtraClick,
    over_entity: bool,
    mining: bool,
    miss_cooldown: bool,
) -> ExtraClick {
    if planned == ExtraClick::Attack && over_entity && !mining && !miss_cooldown {
        ExtraClick::Attack
    } else {
        ExtraClick::SwingOnly
    }
}

/// A use follow-up is dropped when vanilla would refuse it; it is never changed
/// into a swing animation.
pub fn use_extra_allowed(
    planned: ExtraClick,
    hands_busy: bool,
    using_item: bool,
    mining: bool,
) -> bool {
    planned == ExtraClick::Use && !hands_busy && !using_item && !mining
}

#[cfg(test)]
mod tests {
    use super::{DoubleClickScheduler, ExtraClick, extra_click_action, use_extra_allowed};

    #[test]
    fn one_to_three_extras_follow_the_original_on_later_ticks() {
        for count in 1..=3 {
            let mut scheduler = DoubleClickScheduler::default();
            scheduler.record_original(count, ExtraClick::Attack);

            assert_eq!(scheduler.take_next(1), None);
            for _ in 0..count {
                assert_eq!(scheduler.take_next(1), Some(ExtraClick::Attack));
            }
            assert_eq!(scheduler.take_next(1), None);
            assert_eq!(scheduler.pending_count(), 0);
        }
    }

    #[test]
    fn delay_spaces_the_first_extra_and_subsequent_extras() {
        let mut scheduler = DoubleClickScheduler::default();
        scheduler.record_original(2, ExtraClick::Attack);

        assert_eq!(scheduler.take_next(2), None);
        assert_eq!(scheduler.take_next(2), None);
        assert_eq!(scheduler.take_next(2), Some(ExtraClick::Attack));
        assert_eq!(scheduler.take_next(2), None);
        assert_eq!(scheduler.take_next(2), Some(ExtraClick::Attack));
        assert_eq!(scheduler.take_next(2), None);
    }

    #[test]
    fn a_new_original_click_takes_priority_over_pending_extras() {
        let mut scheduler = DoubleClickScheduler::default();
        scheduler.record_original(1, ExtraClick::Attack);
        assert_eq!(scheduler.take_next(1), None);

        scheduler.record_original(1, ExtraClick::Attack);
        assert_eq!(scheduler.take_next(1), None);
        assert_eq!(scheduler.take_next(1), Some(ExtraClick::Attack));
        assert_eq!(scheduler.take_next(1), Some(ExtraClick::Attack));
        assert_eq!(scheduler.take_next(1), None);
    }

    #[test]
    fn rapid_originals_never_leave_an_unbounded_backlog() {
        let mut scheduler = DoubleClickScheduler::default();
        for _ in 0..100 {
            scheduler.record_original(3, ExtraClick::Attack);
            assert_eq!(scheduler.take_next(1), None);
            assert_eq!(scheduler.pending_count(), 3);
        }

        for _ in 0..3 {
            assert_eq!(scheduler.take_next(1), Some(ExtraClick::Attack));
        }
        assert_eq!(scheduler.take_next(1), None);
    }

    #[test]
    fn original_click_kinds_keep_their_queue_order() {
        let mut scheduler = DoubleClickScheduler::default();
        scheduler.record_original(1, ExtraClick::SwingOnly);
        scheduler.record_original(1, ExtraClick::Attack);

        assert_eq!(scheduler.take_next(1), None);
        assert_eq!(scheduler.take_next(1), Some(ExtraClick::SwingOnly));
        assert_eq!(scheduler.take_next(1), Some(ExtraClick::Attack));
    }

    #[test]
    fn block_mining_and_miss_cooldown_never_turn_into_extra_attacks() {
        assert_eq!(
            extra_click_action(ExtraClick::SwingOnly, true, false, false),
            ExtraClick::SwingOnly
        );
        assert_eq!(
            extra_click_action(ExtraClick::Attack, false, false, false),
            ExtraClick::SwingOnly
        );
        assert_eq!(
            extra_click_action(ExtraClick::Attack, true, true, false),
            ExtraClick::SwingOnly
        );
        assert_eq!(
            extra_click_action(ExtraClick::Attack, true, false, true),
            ExtraClick::SwingOnly
        );
        assert_eq!(
            extra_click_action(ExtraClick::Attack, true, false, false),
            ExtraClick::Attack
        );
    }

    #[test]
    fn left_and_right_schedulers_keep_their_own_click_kind() {
        let mut left = DoubleClickScheduler::default();
        let mut right = DoubleClickScheduler::default();
        left.record_original(1, ExtraClick::Attack);
        right.record_original(3, ExtraClick::Use);

        assert_eq!(left.take_next(1), None);
        assert_eq!(right.take_next(1), None);
        assert_eq!(left.take_next(1), Some(ExtraClick::Attack));
        for _ in 0..3 {
            assert_eq!(right.take_next(1), Some(ExtraClick::Use));
        }
        assert_eq!(left.take_next(1), None);
        assert_eq!(right.take_next(1), None);
    }

    #[test]
    fn a_use_vanilla_would_refuse_is_dropped_not_turned_into_a_swing() {
        assert!(use_extra_allowed(ExtraClick::Use, false, false, false));
        assert!(!use_extra_allowed(ExtraClick::Use, true, false, false));
        assert!(!use_extra_allowed(ExtraClick::Use, false, true, false));
        assert!(!use_extra_allowed(ExtraClick::Use, false, false, true));
        assert!(!use_extra_allowed(ExtraClick::Attack, false, false, false));
        assert!(!use_extra_allowed(
            ExtraClick::SwingOnly,
            false,
            false,
            false
        ));
    }

    #[test]
    fn clearing_discards_pending_actions_and_timing_state() {
        let mut scheduler = DoubleClickScheduler::default();
        scheduler.record_original(3, ExtraClick::SwingOnly);
        assert_eq!(scheduler.take_next(3), None);
        scheduler.clear();

        assert_eq!(scheduler.pending_count(), 0);
        assert_eq!(scheduler.take_next(1), None);
        scheduler.record_original(1, ExtraClick::Attack);
        assert_eq!(scheduler.take_next(1), None);
        assert_eq!(scheduler.take_next(1), Some(ExtraClick::Attack));
        assert_eq!(scheduler.take_next(1), None);
    }

    #[test]
    fn invalid_values_cannot_create_a_burst_or_an_oversized_queue() {
        let mut scheduler = DoubleClickScheduler::default();
        scheduler.record_original(100, ExtraClick::Attack);
        assert_eq!(scheduler.pending_count(), 3);
        assert_eq!(scheduler.take_next(0), None);
        for _ in 0..3 {
            assert_eq!(scheduler.take_next(0), Some(ExtraClick::Attack));
        }
        assert_eq!(scheduler.take_next(0), None);

        scheduler.record_original(-1, ExtraClick::Attack);
        assert_eq!(scheduler.take_next(1), None);
        assert_eq!(scheduler.take_next(1), None);
    }
}
