//! A small deterministic random source.
//!
//! The Kotlin sources take a `kotlin.random.Random` in the two places where a decision must not
//! have a fixed beat ([`crate::aim::mouse_pattern_guard`] and the noise of the aim lattice). Kotlin
//! tests seed it (`Random(11)`), and the Rust tests must do the same, so the port cannot depend on
//! `rand`: this crate has no dependencies at all, which also keeps the eventual bundle free of a
//! Cargo graph the launcher has to build.
//!
//! `SplitMix64` is used because it is eight lines, has no state to warm up and produces the same
//! bits on every platform. It is **not** cryptographic and is not used for anything that needs to
//! be.

/// SplitMix64, seeded explicitly.
///
/// ```
/// use liquidaddon_core::SplitMix64;
///
/// let mut rng = SplitMix64::new(1);
/// let a: f32 = rng.next_f32();
/// assert!((0.0..1.0).contains(&a));
/// assert_eq!(SplitMix64::new(1).next_f32(), a, "same seed, same stream");
/// ```
#[derive(Clone, Debug)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// A stream that is fully determined by `seed`.
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next 64 raw bits (the standard SplitMix64 output function).
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `[0, 1)`, taken from the top 24 bits so every result is an exact `f32`.
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / 16_777_216.0
    }

    /// A value in `[0, 1)`, with 53 random bits for dither thresholds in `f64`.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / 9_007_199_254_740_992.0
    }

    /// A coin, from the top bit of a fresh draw.
    pub fn next_bool(&mut self) -> bool {
        self.next_u64() >> 63 != 0
    }
}

#[cfg(test)]
mod tests {
    use super::SplitMix64;

    #[test]
    fn the_stream_is_reproducible_and_inside_the_unit_interval() {
        let mut a = SplitMix64::new(7);
        let mut b = SplitMix64::new(7);
        for _ in 0..1_000 {
            let value = a.next_f32();
            assert!((0.0..1.0).contains(&value), "out of range: {value}");
            assert_eq!(value, b.next_f32());
        }
    }

    #[test]
    fn different_seeds_disagree() {
        let mut a = SplitMix64::new(1);
        let mut b = SplitMix64::new(2);
        let differing = (0..64).filter(|_| a.next_u64() != b.next_u64()).count();
        assert!(
            differing > 55,
            "the two streams are too similar: {differing}"
        );
    }
}
