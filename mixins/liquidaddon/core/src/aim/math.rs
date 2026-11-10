//! The `Engine = Math` path of `AimEngine.kt`, without Minecraft objects.
//!
//! A caller supplies the current rotation, the point-derived goal, the mouse GCD and the gates it
//! already evaluated (hitbox bubble, turn direction, fade/glide). This module owns the source's
//! curve, the per-target/slow hand variation, the dead-zone rest behaviour, pitch/yaw caps, mouse
//! lattice, carried dither, hand noise and final [`MousePatternGuard`] pass. It returns the rotation
//! to apply; visible-vs-silent delivery stays a Horizon concern and is selected by
//! [`RotationMode`](super::settings::RotationMode) at runtime.
//!
//! The order is the one in Kotlin: turn delta → dead-zone/rest gate → curve and gates → jitter →
//! soft caps → desired pitch clamp → optional GCD quantisation/carry → hand layer → cycle guard →
//! absolute pitch clamp. Keeping the order explicit is important: moving the carry after the hand
//! noise, for example, turns a bounded wobble into a walk away from the goal.

use crate::rng::SplitMix64;

use super::ai_hand;
use super::geometry::{Aabb, AimOffset};
use super::hand_gate::{HandGate, movement_counts};
use super::mouse_pattern_guard::MousePatternGuard;
use super::rotation::{PLAN_PITCH_LIMIT, Rotation, WRITE_PITCH_LIMIT, angle_difference};
use super::settings::{AiHandMode, AimSettings, Engine, Mode, Smoothing};

/// How far the response curve saturates yaw input, in degrees (`shape`).
pub const SHAPE_YAW_SCALE: f32 = 12.0;
/// Amplitude of the per-target speed walk, before the changing hand profile is applied.
pub const SPEED_DRIFT_STEP: f32 = 0.07;
/// Chance a resting silent rotation keeps still on an eligible rest tick.
pub const REST_CHANCE_PERCENT: u32 = 35;
/// How much smaller than a mouse count a dead-zone error may be before a rest tick (`REST_COUNTS`).
pub const REST_COUNTS: f32 = 1.15;
/// Dither displacement, in mouse-count units, around the nearest-integer boundary.
pub const DITHER_WOBBLE: f64 = 0.18;
/// How often the hand profile is redrawn, minimum interval in milliseconds.
pub const HAND_PROFILE_MIN_MS: u64 = 20_000;
/// Additional random profile interval (inclusive), in milliseconds.
pub const HAND_PROFILE_JITTER_MS: u32 = 40_001;

/// The Fitts-style share of the yaw error for one smoothing curve (`AimassistCore.shape`).
pub fn shape(smoothing: Smoothing, delta_yaw: f32) -> f32 {
    let t = (delta_yaw.abs() / SHAPE_YAW_SCALE).clamp(0.0, 1.0);
    match smoothing {
        Smoothing::Linear => 1.0,
        Smoothing::Bezier => 0.55 + 0.45 * (t * t * (3.0 - 2.0 * t)),
        Smoothing::Sigmoid => 0.45 + 0.55 / (1.0 + (-6.0 * (t - 0.45)).exp()),
    }
}

/// Saturate a step smoothly at `limit` (`softLimit`); a non-positive limit grants no movement.
pub fn soft_limit(value: f32, limit: f32) -> f32 {
    if limit <= 0.0 {
        0.0
    } else {
        limit * (value / limit).tanh()
    }
}

/// The slowly changing hand profile that the source redraws every 20–60 seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandProfile {
    pub refresh_at_ms: u64,
    pub noise: f32,
    pub overshoot: f32,
    pub drift: f32,
    pub undershoot_percent: i32,
}

impl Default for HandProfile {
    fn default() -> Self {
        Self {
            refresh_at_ms: 0,
            noise: 1.0,
            overshoot: 1.0,
            drift: 1.0,
            undershoot_percent: 10,
        }
    }
}

/// Inputs computed by the client-side controller for one aim tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MathAimInput {
    pub from: Rotation,
    pub goal: Rotation,
    /// `RotationUtil.gcd` in degrees for this player's mouse sensitivity. Keep it `f64` for the
    /// quantizer, which uses the source's double-precision GCD; the hand gate casts it to `f32`.
    pub gcd: f64,
    /// Monotonic game time in milliseconds (used only to redraw the hand profile).
    pub now_ms: u64,
    /// Weight from the hitbox assist bubble, `0..=1`.
    pub weight: f32,
    /// Turn-away gate (`1` when disabled or allowed, `0..1` while blending out).
    pub turn_gate: f32,
    /// Idle-frame glide (`1` normally, `0.15` for a visible idle beat).
    pub glide: f32,
    /// Smooth fade-in progress, `0..=1`.
    pub fade: f32,
}

impl MathAimInput {
    /// A normal fully granted tick. Callers can set the individual gates after construction.
    pub const fn new(from: Rotation, goal: Rotation, gcd: f64, now_ms: u64) -> Self {
        Self {
            from,
            goal,
            gcd,
            now_ms,
            weight: 1.0,
            turn_gate: 1.0,
            glide: 1.0,
            fade: 1.0,
        }
    }
}

/// The player's mouse measurement and the share the hand gate grants this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandObservation {
    pub counts: f32,
    pub share: f32,
}

/// A single computed result. `rotation` is the unwrapped absolute yaw the game may accumulate;
/// `yaw_delta`/`pitch_delta` are the increments the adapter should write or send.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MathStep {
    pub rotation: Rotation,
    pub yaw_delta: f32,
    pub pitch_delta: f32,
    /// Counts after noise and cycle repair, when the GCD path actually produced mouse counts.
    pub mouse_counts: Option<(i32, i32)>,
    /// Curve share before the soft caps and the lattice.
    pub curve_share: f32,
    /// True for the small random walk used when a silent aim is already inside its dead zone.
    pub rest_drift: bool,
}

#[derive(Clone, Copy, Debug)]
struct Quantized {
    yaw_delta: f32,
    pitch_delta: f32,
    mouse_counts: Option<(i32, i32)>,
}

/// Stateful curve engine for one module instance.
#[derive(Debug)]
pub struct MathAimEngine {
    rng: SplitMix64,
    patterns: MousePatternGuard,
    hand_gate: HandGate,
    hand_share: f32,
    hand_counts: f32,
    profile: HandProfile,
    aim_offset: AimOffset,
    response_jitter: f32,
    strength_scale: f32,
    speed_drift: f32,
    last_counts_yaw: Option<i32>,
    last_counts_pitch: Option<i32>,
    yaw_carry: f64,
    pitch_carry: f64,
    still_ticks: u32,
    drift_sign: i32,
}

impl MathAimEngine {
    /// New deterministic state; the game adapter should seed this once per module instance.
    pub fn new(seed: u64) -> Self {
        let mut rng = SplitMix64::new(seed);
        let drift_sign = if rng.next_bool() { 1 } else { -1 };
        Self {
            rng,
            patterns: MousePatternGuard::new(seed ^ 0xA17E_5EED_0BAD_F00D),
            hand_gate: HandGate::new(),
            hand_share: 0.0,
            hand_counts: 0.0,
            profile: HandProfile::default(),
            aim_offset: AimOffset::new(),
            response_jitter: 1.0,
            strength_scale: 1.0,
            speed_drift: 1.0,
            last_counts_yaw: None,
            last_counts_pitch: None,
            yaw_carry: 0.0,
            pitch_carry: 0.0,
            still_ticks: 0,
            drift_sign,
        }
    }

    /// A clean lifecycle reset, mirroring `AimassistCore.onEnable`.
    pub fn reset(&mut self) {
        self.hand_gate.reset();
        self.hand_share = 0.0;
        self.hand_counts = 0.0;
        self.profile = HandProfile::default();
        self.aim_offset.reset();
        self.response_jitter = 1.0;
        self.strength_scale = 1.0;
        self.speed_drift = 1.0;
        self.last_counts_yaw = None;
        self.last_counts_pitch = None;
        self.yaw_carry = 0.0;
        self.pitch_carry = 0.0;
        self.still_ticks = 0;
        self.drift_sign = if self.rng.next_bool() { 1 } else { -1 };
        self.patterns.reset();
    }

    /// Current profile values, exposed for deterministic diagnostics and tests.
    pub const fn hand_profile(&self) -> HandProfile {
        self.profile
    }

    /// Last measured mouse movement in counts, as in the source's debug line.
    pub const fn hand_counts(&self) -> f32 {
        self.hand_counts
    }

    /// Current activity share of the hand gate.
    pub const fn hand_share(&self) -> f32 {
        self.hand_share
    }

    /// Number of cycle repairs since the last reset.
    pub fn pattern_repairs(&self) -> u32 {
        self.patterns.repairs()
    }

    /// Sample the source's held in-box target offset using the same deterministic hand RNG.
    pub fn sample_aim_offset(&mut self, now_ms: u64, bbox: &Aabb) -> super::rotation::Vec3 {
        self.aim_offset.sample(now_ms, bbox, &mut self.rng)
    }

    /// Called when a new target is committed. The source draws a response multiplier and a
    /// per-target strength spread once, not once per frame.
    pub fn begin_target(&mut self, settings: &impl AimSettings, now_ms: u64) {
        self.refresh_hand_profile(now_ms);
        let layer = effective_hand_layer(settings);
        self.response_jitter = 0.85 + self.rng.next_f32() * 0.3;
        self.strength_scale =
            1.0 + (self.rng.next_f32() - 0.5) * 2.0 * (layer.strength_spread as f32 / 100.0);
        self.speed_drift = 1.0;
        self.still_ticks = 0;
    }

    /// Measure the player's own rotation for this tick and update the hand gate. The caller should
    /// subtract the previous aim step before calling, exactly as `AimassistCore.tick` does. Call it
    /// every tick, including without a target: the hand-profile clock and gate fade use the tick
    /// clock, not only the ticks that happen to aim.
    pub fn observe_hand(
        &mut self,
        settings: &impl AimSettings,
        own_yaw: f32,
        own_pitch: f32,
        gcd: f64,
        now_ms: u64,
    ) -> HandObservation {
        self.refresh_hand_profile(now_ms);
        let counts = movement_counts(own_yaw, own_pitch, gcd as f32);
        let share = if settings.hand_only() {
            self.hand_gate
                .update(counts, settings.hand_threshold(), settings.hand_fade())
        } else {
            1.0
        };
        self.hand_counts = counts;
        self.hand_share = share;
        HandObservation { counts, share }
    }

    /// One `Engine = Math` aim tick, or the same curve used as an AI fallback.
    ///
    /// `observe_hand` must be called first on every tick (including no-target ticks) so the gate's
    /// fade is measured on the tick clock. `None` means no rotation was produced: mode is Friction,
    /// the player's hand gate/turn gate is closed, the visible aim is resting in its dead zone, or an
    /// input is not finite. The outer Horizon controller remains responsible for target choice,
    /// reaction/pause gates, interaction synchronization, and actual camera/packet application.
    pub fn step(&mut self, settings: &impl AimSettings, input: MathAimInput) -> Option<MathStep> {
        self.refresh_hand_profile(input.now_ms);
        if settings.mode() == Mode::Friction || !input_is_finite(input) {
            return None;
        }
        if (settings.hand_only() && self.hand_share <= 0.0) || input.turn_gate <= 0.0 {
            return None;
        }
        let delta_yaw = angle_difference(input.goal.yaw, input.from.yaw);
        let delta_pitch = input.goal.pitch - input.from.pitch;
        if !delta_yaw.is_finite() || !delta_pitch.is_finite() {
            return None;
        }

        if self.inside_dead_zone(settings, input.gcd, delta_yaw, delta_pitch) {
            return self.still_tick(settings, input);
        }

        self.speed_drift = (self.speed_drift
            + (self.rng.next_f32() - 0.5) * SPEED_DRIFT_STEP * self.profile.drift)
            .clamp(0.85, 1.15);

        let allowance = input.weight
            * self.response_jitter
            * self.speed_drift
            * input.turn_gate
            * input.glide
            * input.fade
            * self.hand_share;
        let share = (settings.strength() / 100.0)
            * self.strength_scale
            * shape(settings.smoothing(), delta_yaw)
            * allowance;

        let mut step_yaw = delta_yaw * share;
        let mut step_pitch = delta_pitch * share * (settings.pitch_assist() / 100.0);
        step_yaw += (self.rng.next_f32() - 0.5) * settings.jitter_yaw();
        step_pitch += (self.rng.next_f32() - 0.5) * settings.jitter_pitch();

        let yaw_limit = settings.max_speed() * (0.85 + self.rng.next_f32() * 0.15);
        let pitch_limit = (settings.max_speed() * 0.7)
            .min(settings.pitch_step_max() * (0.85 + self.rng.next_f32() * 0.15));
        step_yaw = soft_limit(step_yaw, yaw_limit);
        step_pitch = soft_limit(step_pitch, pitch_limit);

        self.apply_aim(
            settings,
            input.from,
            Rotation::new(step_yaw, step_pitch),
            share,
            false,
            input.gcd,
        )
    }

    fn refresh_hand_profile(&mut self, now_ms: u64) {
        if now_ms <= self.profile.refresh_at_ms {
            return;
        }
        self.profile.refresh_at_ms =
            now_ms
                .saturating_add(HAND_PROFILE_MIN_MS)
                .saturating_add(u64::from(random_below(
                    &mut self.rng,
                    HAND_PROFILE_JITTER_MS,
                )));
        self.profile.noise = 0.6 + self.rng.next_f32();
        self.profile.overshoot = 0.5 + self.rng.next_f32() * 1.3;
        self.profile.drift = 0.5 + self.rng.next_f32() * 1.1;
        self.profile.undershoot_percent = 6 + random_below(&mut self.rng, 13) as i32;
    }

    fn inside_dead_zone(
        &self,
        settings: &impl AimSettings,
        gcd: f64,
        delta_yaw: f32,
        delta_pitch: f32,
    ) -> bool {
        let lattice = if settings.gcd_fix() { gcd as f32 } else { 0.0 };
        let rest_yaw = settings.dead_zone().max(lattice * REST_COUNTS);
        let rest_pitch = (settings.dead_zone() * 0.7).max(lattice * REST_COUNTS);
        delta_yaw.abs() < rest_yaw && delta_pitch.abs() < rest_pitch
    }

    fn still_tick(&mut self, settings: &impl AimSettings, input: MathAimInput) -> Option<MathStep> {
        if !settings.silent() {
            return None;
        }
        self.still_ticks += 1;
        if self.still_ticks < 2 {
            return None;
        }
        self.still_ticks = 0;

        if random_below(&mut self.rng, 100) < REST_CHANCE_PERCENT {
            return None;
        }

        let roll = random_below(&mut self.rng, 100);
        let base_counts = match roll {
            0..=59 => 1,
            60..=87 => 2,
            88..=96 => 3,
            _ => 4 + random_below(&mut self.rng, 3) as i32,
        };
        let drift_counts =
            kotlin_round_to_i32(f64::from(base_counts) * f64::from(self.profile.noise)).clamp(1, 8);
        if random_below(&mut self.rng, 100) < 30 {
            self.drift_sign = -self.drift_sign;
        }

        let step_yaw = (self.drift_sign * drift_counts) as f32 * input.gcd as f32;
        let step_pitch = if settings.pitch_assist() > 0.0 && random_below(&mut self.rng, 100) < 25 {
            (if self.rng.next_bool() { 1.0 } else { -1.0 }) * input.gcd as f32
        } else {
            0.0
        };
        self.apply_aim(
            settings,
            input.from,
            Rotation::new(step_yaw, step_pitch),
            0.0,
            true,
            input.gcd,
        )
    }

    fn apply_aim(
        &mut self,
        settings: &impl AimSettings,
        from: Rotation,
        step_delta: Rotation,
        curve_share: f32,
        rest_drift: bool,
        gcd: f64,
    ) -> Option<MathStep> {
        let desired = Rotation::new(
            from.yaw + step_delta.yaw,
            (from.pitch + step_delta.pitch).clamp(-PLAN_PITCH_LIMIT, PLAN_PITCH_LIMIT),
        );
        let quantized = if settings.gcd_fix() {
            self.quantize_delta(settings, from, desired, gcd, rest_drift)?
        } else {
            Quantized {
                yaw_delta: angle_difference(desired.yaw, from.yaw),
                pitch_delta: desired.pitch - from.pitch,
                mouse_counts: None,
            }
        };
        let rotation = Rotation::new(
            from.yaw + quantized.yaw_delta,
            (from.pitch + quantized.pitch_delta).clamp(-WRITE_PITCH_LIMIT, WRITE_PITCH_LIMIT),
        );
        if !rotation.yaw.is_finite()
            || !rotation.pitch.is_finite()
            || !quantized.yaw_delta.is_finite()
            || !quantized.pitch_delta.is_finite()
        {
            return None;
        }
        Some(MathStep {
            rotation,
            yaw_delta: quantized.yaw_delta,
            pitch_delta: quantized.pitch_delta,
            mouse_counts: quantized.mouse_counts,
            curve_share,
            rest_drift,
        })
    }

    fn quantize_delta(
        &mut self,
        settings: &impl AimSettings,
        from: Rotation,
        desired: Rotation,
        gcd: f64,
        rest_drift: bool,
    ) -> Option<Quantized> {
        let delta_yaw = f64::from(angle_difference(desired.yaw, from.yaw));
        let delta_pitch = f64::from(desired.pitch - from.pitch);
        let step = gcd;
        if step < 1.0e-6 {
            // Same early exit as Kotlin: no hand noise, repetition check or cycle guard without a
            // usable mouse lattice.
            return Some(Quantized {
                yaw_delta: delta_yaw as f32,
                pitch_delta: delta_pitch as f32,
                mouse_counts: None,
            });
        }

        let dither = settings.gcd_dither();
        let raw_yaw = delta_yaw / step + if dither { self.yaw_carry } else { 0.0 };
        let raw_pitch = delta_pitch / step + if dither { self.pitch_carry } else { 0.0 };
        if !raw_yaw.is_finite() || !raw_pitch.is_finite() {
            return None;
        }

        let mut counts_yaw = if dither {
            self.hand_round(raw_yaw)
        } else {
            kotlin_round_to_i32(raw_yaw)
        };
        let mut counts_pitch = if dither {
            self.hand_round(raw_pitch)
        } else {
            kotlin_round_to_i32(raw_pitch)
        };

        if dither {
            self.yaw_carry = (raw_yaw - f64::from(counts_yaw)).clamp(-0.95, 0.95);
            self.pitch_carry = (raw_pitch - f64::from(counts_pitch)).clamp(-0.95, 0.95);
        }

        // The rest walk already chose bounded counts; normal hand noise/repetition repair could
        // enlarge its deliberate one-count pitch drift.
        if !rest_drift {
            let hand = effective_hand_layer(settings);
            (counts_yaw, counts_pitch) = self.hand_on_lattice(
                counts_yaw,
                counts_pitch,
                delta_yaw,
                delta_pitch,
                hand.mouse_noise,
                hand.overshoot,
            );
        }

        if settings.pattern_guard() {
            (counts_yaw, counts_pitch) = self.patterns.guard(counts_yaw, counts_pitch);
        }
        self.last_counts_yaw = Some(counts_yaw);
        self.last_counts_pitch = Some(counts_pitch);
        Some(Quantized {
            yaw_delta: (f64::from(counts_yaw) * step) as f32,
            pitch_delta: (f64::from(counts_pitch) * step) as f32,
            mouse_counts: Some((counts_yaw, counts_pitch)),
        })
    }

    fn hand_round(&mut self, counts: f64) -> i32 {
        let wobble = (self.rng.next_f64() * 2.0 - 1.0) * DITHER_WOBBLE;
        kotlin_round_to_i32(counts + wobble)
    }

    fn hand_on_lattice(
        &mut self,
        counts_yaw: i32,
        counts_pitch: i32,
        delta_yaw: f64,
        delta_pitch: f64,
        mouse_noise: i32,
        overshoot: i32,
    ) -> (i32, i32) {
        let mut yaw = counts_yaw;
        let mut pitch = counts_pitch;
        if counts_yaw == 0 && counts_pitch == 0 {
            return (0, 0);
        }

        if self.profile.undershoot_percent > 0
            && random_below(&mut self.rng, 100) < self.profile.undershoot_percent as u32
        {
            let fold = 0.55 + f64::from(self.rng.next_f32()) * 0.3;
            if yaw.saturating_abs() >= 3 {
                let short = kotlin_round_to_i32(f64::from(yaw) * fold);
                if short != 0 {
                    yaw = short;
                }
            }
            if pitch.saturating_abs() >= 3 {
                let short = kotlin_round_to_i32(f64::from(pitch) * fold);
                if short != 0 {
                    pitch = short;
                }
            }
        }

        let noise =
            kotlin_round_to_i32(f64::from(mouse_noise.max(0)) * f64::from(self.profile.noise));
        if noise > 0 {
            yaw += self.noise_counts(noise, counts_yaw.saturating_abs());
            if delta_pitch != 0.0 || random_below(&mut self.rng, 100) < 30 {
                pitch += self.noise_counts(noise.min(2), counts_pitch.saturating_abs());
            }
        }

        if let Some(last) = self.last_counts_yaw
            && yaw != 0
            && yaw == last
        {
            yaw += if self.rng.next_bool() { 1 } else { -1 };
        }
        if let Some(last) = self.last_counts_pitch
            && pitch != 0
            && pitch == last
        {
            pitch += if self.rng.next_bool() { 1 } else { -1 };
        }

        let overshoot_chance =
            kotlin_round_to_i32(f64::from(overshoot.max(0)) * f64::from(self.profile.overshoot));
        if overshoot_chance > 0 && random_below(&mut self.rng, 100) < overshoot_chance as u32 {
            let yaw_sign = if delta_yaw > 0.0 {
                1
            } else if delta_yaw < 0.0 {
                -1
            } else if yaw > 0 {
                1
            } else if yaw < 0 {
                -1
            } else {
                0
            };
            if yaw_sign != 0 {
                yaw += yaw_sign;
            }
            if delta_pitch != 0.0 {
                pitch += if delta_pitch > 0.0 { 1 } else { -1 };
            }
        }

        (yaw, pitch)
    }

    fn noise_counts(&mut self, bound: i32, magnitude: i32) -> i32 {
        let reach = if magnitude >= 40 {
            3
        } else if magnitude >= 12 {
            2
        } else if magnitude >= 4 {
            1
        } else {
            0
        };
        let top = (bound + reach).clamp(1, 6);
        let roll = random_below(&mut self.rng, 100);
        let amplitude = if roll < 30 {
            0
        } else if top >= 3 && roll >= 92 {
            3
        } else if top >= 2 && roll >= 76 {
            2
        } else {
            1
        };
        if amplitude == 0 {
            0
        } else if self.rng.next_bool() {
            amplitude
        } else {
            -amplitude
        }
    }
}

impl Default for MathAimEngine {
    fn default() -> Self {
        Self::new(0)
    }
}

fn effective_hand_layer(settings: &impl AimSettings) -> ai_hand::Layer {
    ai_hand::layer(
        settings.ai_hand() == AiHandMode::On,
        settings.engine() == Engine::Ai,
        settings.mouse_noise(),
        settings.overshoot(),
        settings.strength_spread(),
    )
}

fn input_is_finite(input: MathAimInput) -> bool {
    input.from.yaw.is_finite()
        && input.from.pitch.is_finite()
        && input.goal.yaw.is_finite()
        && input.goal.pitch.is_finite()
        && input.gcd.is_finite()
        && input.weight.is_finite()
        && input.turn_gate.is_finite()
        && input.glide.is_finite()
        && input.fade.is_finite()
}

/// Kotlin's `roundToInt`/the source's `round(...).toInt()` decision boundary: floor(x + 0.5),
/// saturating at the integer endpoints as Kotlin's floating conversion does.
fn kotlin_round_to_i32(value: f64) -> i32 {
    if value.is_nan() {
        0
    } else if value >= f64::from(i32::MAX) {
        i32::MAX
    } else if value <= f64::from(i32::MIN) {
        i32::MIN
    } else {
        (value + 0.5).floor() as i32
    }
}

fn random_below(rng: &mut SplitMix64, exclusive: u32) -> u32 {
    debug_assert!(exclusive > 0);
    let bound = u64::from(exclusive);
    let limit = u64::MAX - (u64::MAX % bound);
    loop {
        let draw = rng.next_u64();
        if draw < limit {
            return (draw % bound) as u32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MathAimEngine, MathAimInput, shape, soft_limit};
    use crate::aim::rotation::{DEFAULT_PITCH_STEP_MAX, Rotation};
    use crate::aim::settings::{AimConfig, Mode, RotationMode, Smoothing};

    fn close(expected: f32, actual: f32, tolerance: f32) {
        assert!(
            (expected - actual).abs() <= tolerance,
            "expected {expected}, got {actual} (tolerance {tolerance})"
        );
    }

    fn quiet_config() -> AimConfig {
        AimConfig {
            strength: 35.0,
            max_speed: 7.0,
            pitch_assist: 100.0,
            jitter_yaw: 0.0,
            jitter_pitch: 0.0,
            gcd_fix: false,
            gcd_dither: false,
            mouse_noise: 0,
            overshoot: 0,
            strength_spread: 0,
            pattern_guard: false,
            hand_only: false,
            dead_zone: 0.0,
            ..AimConfig::default()
        }
    }

    #[test]
    fn the_three_math_curves_match_the_source_formula() {
        assert_eq!(shape(Smoothing::Linear, 0.0), 1.0);
        assert_eq!(shape(Smoothing::Linear, 100.0), 1.0);

        close(0.55, shape(Smoothing::Bezier, 0.0), 1.0e-6);
        close(0.775, shape(Smoothing::Bezier, 6.0), 1.0e-6);
        close(1.0, shape(Smoothing::Bezier, 12.0), 1.0e-6);
        assert_eq!(shape(Smoothing::Bezier, 40.0), 1.0);
        assert_eq!(
            shape(Smoothing::Bezier, -6.0),
            shape(Smoothing::Bezier, 6.0)
        );

        let sigmoid_start = shape(Smoothing::Sigmoid, 0.0);
        let sigmoid_mid = shape(Smoothing::Sigmoid, 5.4);
        let sigmoid_end = shape(Smoothing::Sigmoid, 12.0);
        assert!(sigmoid_start > 0.45 && sigmoid_start < sigmoid_mid);
        assert!(sigmoid_mid < sigmoid_end && sigmoid_end < 1.0);
    }

    #[test]
    fn soft_limit_is_near_linear_below_the_knee_and_bounded_above_it() {
        close(0.999_630, soft_limit(1.0, 30.0), 1.0e-5);
        assert_eq!(soft_limit(4.0, 0.0), 0.0);
        let capped = soft_limit(100.0, 7.0);
        assert!((6.99..=7.0).contains(&capped));
        let negative_capped = soft_limit(-100.0, 7.0);
        assert!((-7.0..-6.99).contains(&negative_capped));
    }

    #[test]
    fn the_hand_gate_is_measured_before_a_math_step() {
        let mut engine = MathAimEngine::new(2);
        let settings = AimConfig::default();
        let still = engine.observe_hand(&settings, 0.0, 0.0, 0.15, 1_000);
        assert_eq!(still.counts, 0.0);
        assert_eq!(still.share, 0.0);

        let moving = engine.observe_hand(&settings, 0.15, 0.0, 0.15, 1_000);
        assert_eq!(moving.counts, 1.0);
        assert_eq!(moving.share, 1.0);

        let fading = engine.observe_hand(&settings, 0.0, 0.0, 0.15, 1_000);
        close(0.75, fading.share, 1.0e-6);
    }

    #[test]
    fn curve_steps_are_finite_and_slow_down_as_the_error_closes() {
        let settings = quiet_config();
        let mut engine = MathAimEngine::new(11);
        engine.begin_target(&settings, 1_000);
        engine.observe_hand(&settings, 0.0, 0.0, 0.15, 1_000);
        // The hand-only switch is off in this test; the movement observation still follows source order.
        let first = engine
            .step(
                &settings,
                MathAimInput::new(
                    Rotation::new(0.0, 0.0),
                    Rotation::new(10.0, 4.0),
                    0.15,
                    1_050,
                ),
            )
            .expect("a valid math tick returns a step");
        assert!(first.rotation.yaw > 0.0 && first.rotation.yaw < 10.0);
        assert!(first.rotation.pitch > 0.0 && first.rotation.pitch < 4.0);
        assert!(first.yaw_delta.is_finite() && first.pitch_delta.is_finite());
        assert_eq!(first.mouse_counts, None, "GCD fixing is disabled");
        assert!(first.curve_share > 0.0);
    }

    #[test]
    fn fixed_gcd_steps_land_on_whole_mouse_counts() {
        let settings = AimConfig {
            gcd_fix: true,
            gcd_dither: false,
            mouse_noise: 0,
            overshoot: 0,
            ..quiet_config()
        };
        let mut engine = MathAimEngine::new(42);
        engine.begin_target(&settings, 10_000);
        engine.observe_hand(&settings, 0.0, 0.0, 0.15, 1_000);
        let step = engine
            .step(
                &settings,
                MathAimInput::new(
                    Rotation::new(0.0, 0.0),
                    Rotation::new(20.0, 6.0),
                    0.15,
                    10_050,
                ),
            )
            .expect("a valid math tick returns a step");
        let counts = step.mouse_counts.expect("the GCD path returns counts");
        close(counts.0 as f32, step.yaw_delta / 0.15, 1.0e-4);
        close(counts.1 as f32, step.pitch_delta / 0.15, 1.0e-4);
    }

    #[test]
    fn the_caps_bound_each_written_step_and_pitch_never_reaches_ninety() {
        let settings = AimConfig {
            strength: 100.0,
            max_speed: 40.0,
            pitch_assist: 100.0,
            jitter_yaw: 0.0,
            jitter_pitch: 0.0,
            gcd_fix: false,
            dead_zone: 0.0,
            ..quiet_config()
        };
        let mut engine = MathAimEngine::new(99);
        engine.begin_target(&settings, 1_000);
        engine.observe_hand(&settings, 0.0, 0.0, 0.15, 1_000);
        let step = engine
            .step(
                &settings,
                MathAimInput::new(
                    Rotation::new(0.0, 88.0),
                    Rotation::new(110.0, -40.0),
                    0.15,
                    1_050,
                ),
            )
            .expect("a valid math tick returns a step");
        assert!(step.yaw_delta.abs() <= 40.0);
        assert!(step.pitch_delta.abs() <= DEFAULT_PITCH_STEP_MAX);
        assert!(step.rotation.pitch.abs() < 90.0);
    }

    #[test]
    fn a_visible_rotation_does_not_drift_once_it_is_in_the_dead_zone() {
        let settings = AimConfig {
            rotation_mode: RotationMode::Visible,
            dead_zone: 0.5,
            gcd_fix: false,
            hand_only: false,
            ..quiet_config()
        };
        let mut engine = MathAimEngine::new(3);
        engine.begin_target(&settings, 1_000);
        engine.observe_hand(&settings, 0.0, 0.0, 0.15, 1_000);
        let output = engine.step(
            &settings,
            MathAimInput::new(
                Rotation::new(0.0, 0.0),
                Rotation::new(0.2, 0.1),
                0.15,
                1_050,
            ),
        );
        assert_eq!(output, None);
    }

    #[test]
    fn silent_dead_zone_uses_the_slow_random_walk_not_the_curve() {
        let settings = AimConfig {
            rotation_mode: RotationMode::Silent,
            dead_zone: 1.0,
            gcd_fix: true,
            gcd_dither: false,
            mouse_noise: 0,
            overshoot: 0,
            hand_only: false,
            pattern_guard: false,
            ..quiet_config()
        };
        let mut engine = MathAimEngine::new(7);
        engine.begin_target(&settings, 1_000);
        engine.observe_hand(&settings, 0.0, 0.0, 0.15, 1_000);
        let mut moved = false;
        for tick in 0..200 {
            if let Some(step) = engine.step(
                &settings,
                MathAimInput::new(
                    Rotation::new(0.0, 0.0),
                    Rotation::new(0.0, 0.0),
                    0.15,
                    1_050 + tick * 50,
                ),
            ) {
                moved = true;
                assert!(step.rest_drift);
                assert!(step.yaw_delta.abs() <= 8.0 * 0.15 + 1.0e-4);
                assert!(step.pitch_delta.abs() <= 0.15 + 1.0e-4);
            }
        }
        assert!(
            moved,
            "over two hundred ticks a silent rest should occasionally drift"
        );
    }

    #[test]
    fn math_mode_declines_friction_only_ticks_and_bad_numbers() {
        let mut settings = quiet_config();
        settings.mode = Mode::Friction;
        let mut engine = MathAimEngine::new(1);
        engine.begin_target(&settings, 1_000);
        engine.observe_hand(&settings, 0.0, 0.0, 0.15, 1_000);
        let input = MathAimInput::new(Rotation::default(), Rotation::new(10.0, 0.0), 0.15, 1_050);
        assert_eq!(engine.step(&settings, input), None);

        settings.mode = Mode::Both;
        assert_eq!(
            engine.step(
                &settings,
                MathAimInput::new(
                    Rotation::new(f32::NAN, 0.0),
                    Rotation::new(10.0, 0.0),
                    0.15,
                    1_100,
                )
            ),
            None
        );
    }

    #[test]
    fn the_reproducible_seed_repeats_a_whole_sequence() {
        let settings = AimConfig {
            hand_only: false,
            ..quiet_config()
        };
        let mut a = MathAimEngine::new(123);
        let mut b = MathAimEngine::new(123);
        a.begin_target(&settings, 1_000);
        b.begin_target(&settings, 1_000);
        let mut out_a = Vec::new();
        let mut out_b = Vec::new();
        for tick in 0..12 {
            let input = MathAimInput::new(
                Rotation::new(tick as f32 * 0.25, 0.0),
                Rotation::new(12.0, 3.0),
                0.15,
                1_050 + tick * 50,
            );
            out_a.push(a.step(&settings, input));
            out_b.push(b.step(&settings, input));
        }
        assert_eq!(out_a, out_b);
    }
}
