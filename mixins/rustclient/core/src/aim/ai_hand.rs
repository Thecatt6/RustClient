//! What is left of the hand layer when the model is the author.
//!
//! The lattice wobble — `mouseNoise`, the extra count of `overshoot`, the per-target spread of
//! `strengthSpread` — was written to humanise a *curve*: a deterministic law whose steps come out
//! of one formula and need a hand painted on top. A model trained on recorded human ticks does not
//! need that paint: its answer already carries the wobble, the sloppy counts and the varied pull of
//! the hand it learned from, and the layer on top does not add humanity — it bends an authored step
//! into a different one (and the telemetry of `/rustclient ai` keeps measuring the step *before*
//! the bend, so the line would lie).
//!
//! So with `AIHand = On` and `Engine = AI`:
//!
//!  - `mouseNoise` is capped at one count — the axis is never dead silent on the wire, but the
//!    model's own texture is no longer buried under up to four counts of added noise;
//!  - `overshoot` goes to zero — the lattice stops throwing the extra count, because a model
//!    overshoots on its own when the hand it learned from did (the AI branch's clamp still
//!    *allows* that overshoot up to the configured percent; what is removed is the invented one);
//!  - `strengthSpread` goes to zero — the pull is not the model's knob; spreading it per target
//!    only varies the trust and correction terms around an answer that was not built for them.
//!
//! What stays, because it is the floor and not the paint: the GCD quantisation and its carried
//! residual, the no-repeat rule of [`super::mouse_pattern_guard`], the hand gate
//! ([`super::hand_gate`]) and the undershoot that lands a step short now and then.
//!
//! With `Engine = Math` — Profile A, the curve — nothing changes at all: the layer is exactly what
//! it was, because the curve is exactly what needs it.

/// The three dials of the hand layer, as this tick's lattice should read them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Layer {
    pub mouse_noise: i32,
    pub overshoot: i32,
    pub strength_spread: i32,
}

/// The effective layer. A `mouse_noise` the player set to zero stays zero — capping at one never
/// *adds* wobble somebody deliberately removed; it only takes away what would bury the model's own.
pub fn layer(
    ai_hand_on: bool,
    engine_ai: bool,
    mouse_noise: i32,
    overshoot: i32,
    strength_spread: i32,
) -> Layer {
    if ai_hand_on && engine_ai {
        Layer {
            mouse_noise: mouse_noise.min(1),
            overshoot: 0,
            strength_spread: 0,
        }
    } else {
        Layer {
            mouse_noise,
            overshoot,
            strength_spread,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Layer, layer};

    #[test]
    fn the_ai_engine_keeps_at_most_one_count_of_noise() {
        assert_eq!(
            layer(true, true, 4, 3, 2),
            Layer {
                mouse_noise: 1,
                overshoot: 0,
                strength_spread: 0
            }
        );
        assert_eq!(
            layer(true, true, 0, 3, 2),
            Layer {
                mouse_noise: 0,
                overshoot: 0,
                strength_spread: 0
            },
            "a player who removed the noise does not get it back"
        );
    }

    #[test]
    fn the_math_engine_keeps_the_whole_layer() {
        for mouse_noise in [0, 1, 4] {
            assert_eq!(
                layer(false, false, mouse_noise, 3, 2),
                Layer {
                    mouse_noise,
                    overshoot: 3,
                    strength_spread: 2
                }
            );
        }
    }

    #[test]
    fn the_switch_needs_both_conditions() {
        let untouched = Layer {
            mouse_noise: 4,
            overshoot: 3,
            strength_spread: 2,
        };
        assert_eq!(
            layer(true, false, 4, 3, 2),
            untouched,
            "AIHand off-engine AI"
        );
        assert_eq!(
            layer(false, true, 4, 3, 2),
            untouched,
            "engine AI, AIHand off"
        );
    }
}
