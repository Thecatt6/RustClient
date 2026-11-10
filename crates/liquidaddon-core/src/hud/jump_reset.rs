//! The text `JumpReset` shows next to its name in the HUD.
//!
//! The module has two numbers that look alike and mean different things, so the HUD shows the one a
//! player tunes and the one a player earns, side by side:
//!
//!  - **the probability** (`chance`, in percent): how often a hit that qualifies is answered with a
//!    jump at all. It is the `Chance` slider, and seeing it in the HUD is the point — a module that
//!    silently sits at 20% looks broken in a fight, and the tag is where that is visible without
//!    opening the GUI;
//!  - **the tally** (`perfect/attempts`): how many of the jumps that actually left were timed on the
//!    exact tick the knockback arrived. It is the measured side of the same story, and it only
//!    appears once there is something to measure.
//!
//! Two shapes, nothing else: `60%` while the module has not jumped yet, `60% 3/4` afterwards.

/// The largest and smallest percentage a value can mean, whatever the settings say.
pub const MIN_PERCENT: i32 = 0;
/// The largest percentage a value can mean, whatever the settings say.
pub const MAX_PERCENT: i32 = 100;

/// `60%` before the first jump, `60% 3/4` after it.
///
/// Both axes are clamped: a config file can hold anything, and a HUD that reads `140%` or `9/4`
/// would be a lie about the module rather than a bug in the game.
pub fn tag(chance: i32, perfect: i32, attempts: i32) -> String {
    let probability = chance.clamp(MIN_PERCENT, MAX_PERCENT);
    let jumps = attempts.max(0);
    if jumps == 0 {
        return format!("{probability}%");
    }

    // A perfect jump is a subset of the jumps: four jumps can never hold nine of them.
    let landed = perfect.clamp(0, jumps);
    format!("{probability}% {landed}/{jumps}")
}

#[cfg(test)]
mod tests {
    use super::tag;

    #[test]
    fn the_probability_is_there_before_anything_happens() {
        assert_eq!(tag(100, 0, 0), "100%");
        assert_eq!(tag(60, 0, 0), "60%");
    }

    #[test]
    fn a_chance_of_zero_is_a_number_not_an_empty_tag() {
        // A module that never fires is exactly what somebody wants to see in the HUD.
        assert_eq!(tag(0, 0, 0), "0%");
    }

    #[test]
    fn the_tally_joins_the_probability_once_there_are_jumps() {
        assert_eq!(tag(100, 3, 4), "100% 3/4");
        assert_eq!(tag(60, 0, 1), "60% 0/1");
    }

    #[test]
    fn a_jump_that_is_not_perfect_still_counts_as_a_jump() {
        // Every jump is an attempt, only the well-timed ones are perfect: 0/5 is the honest read of
        // a module that is jumping and never resetting (no sprint, most likely).
        assert_eq!(tag(85, 0, 5), "85% 0/5");
    }

    #[test]
    fn a_perfect_jump_is_never_more_than_a_jump() {
        assert_eq!(tag(60, 9, 4), "60% 4/4");
    }

    #[test]
    fn a_percentage_outside_its_range_is_clamped() {
        // A hand-edited config file must not be able to print "140%".
        assert_eq!(tag(140, 0, 0), "100%");
        assert_eq!(tag(-5, 0, 0), "0%");
    }

    #[test]
    fn a_negative_count_is_not_a_jump() {
        assert_eq!(tag(70, -3, -4), "70%");
    }
}
