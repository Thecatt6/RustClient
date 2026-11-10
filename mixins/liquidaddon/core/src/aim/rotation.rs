//! Rotations and the small amount of vector maths the aim path needs.
//!
//! Ported from the `Rotation`/`Vec3`/`RotationUtil` uses in `AimEngine.kt` and from what
//! LiquidBounce's `Rotation` does, but written out here so the core keeps its "no game types" rule.
//!
//! Two conventions have to match the game exactly, because they end up in a packet:
//!
//! * **yaw is not normalised anywhere the aim accumulates.** The vanilla client adds a delta to
//!   `yRot` and lets the field grow (`361.4`, `-720.8`, …); the server wraps it when it applies the
//!   packet. Assigning a wrapped absolute angle instead makes the field jump by almost a full turn
//!   in one tick, which is the single most recognisable rotation flag there is. [`Rotation::new`]
//!   therefore keeps the number it is given; ask for [`Rotation::normalised`] explicitly.
//! * **yaw 0 looks towards +Z, yaw -90 towards +X, pitch -90 is straight up and +90 is down** — the
//!   same numbers as the game, so a value copied from a log means the same thing on both sides.

use std::ops::{Add, Sub};

use super::ai_features::wrap;

/// The default vertical step cap in `AimSettings.pitchStepMax`.
pub const DEFAULT_PITCH_STEP_MAX: f32 = 3.0;

/// The pitch the engine plans within (`applyAim` clamps to `-89.5..89.5` when it builds the goal and
/// to `-89.9..89.9` when it writes the step, so a server never sees a pitch clamped at ±90).
pub const PLAN_PITCH_LIMIT: f32 = 89.5;

/// The pitch the written rotation is clamped to.
pub const WRITE_PITCH_LIMIT: f32 = 89.9;

/// A point or a direction, in blocks, in the level.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn scale(self, factor: f64) -> Vec3 {
        Vec3::new(self.x * factor, self.y * factor, self.z * factor)
    }

    pub fn dot(self, other: Vec3) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    /// Horizontal length, the way `Entity.distanceToSqr`'s callers use it.
    pub fn horizontal_length(self) -> f64 {
        self.x.hypot(self.z)
    }

    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }

    pub fn distance_to(self, other: Vec3) -> f64 {
        (self - other).length()
    }
}

impl Add for Vec3 {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        Self::new(self.x + other.x, self.y + other.y, self.z + other.z)
    }
}

impl Sub for Vec3 {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        Self::new(self.x - other.x, self.y - other.y, self.z - other.z)
    }
}

/// A view direction: yaw and pitch in degrees.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rotation {
    pub yaw: f32,
    pub pitch: f32,
}

impl Rotation {
    /// As given: no wrapping, no clamping. See the module docs for why.
    pub const fn new(yaw: f32, pitch: f32) -> Self {
        Self { yaw, pitch }
    }

    /// The same rotation with the yaw brought into `-180..180` (the range `Mth.wrapDegrees` uses)
    /// and the pitch into `-PLAN_PITCH_LIMIT..=PLAN_PITCH_LIMIT`.
    pub fn normalised(self) -> Self {
        Self {
            yaw: wrap(self.yaw),
            pitch: self.pitch.clamp(-PLAN_PITCH_LIMIT, PLAN_PITCH_LIMIT),
        }
    }

    /// The unit vector this rotation looks along (`Entity.getViewVector`).
    pub fn direction_vector(self) -> Vec3 {
        let yaw = f64::from(self.yaw).to_radians();
        let pitch = f64::from(self.pitch).to_radians();
        Vec3::new(
            -yaw.sin() * pitch.cos(),
            -pitch.sin(),
            yaw.cos() * pitch.cos(),
        )
    }

    /// The rotation from `eyes` to `point` (`Entity.lookAt` / LiquidBounce's `Rotation.lookingAt`).
    pub fn looking_at(point: Vec3, eyes: Vec3) -> Self {
        let d = point - eyes;
        let horizontal = d.horizontal_length();
        let yaw = d.z.atan2(d.x).to_degrees() - 90.0;
        let pitch = -d.y.atan2(horizontal).to_degrees();
        Self {
            yaw: wrap(yaw as f32),
            pitch: (pitch as f32).clamp(-90.0, 90.0),
        }
    }

    /// The pitch clamp `applyAim` applies to the rotation it writes.
    pub fn write_clamped(self) -> Self {
        Self {
            yaw: self.yaw,
            pitch: self.pitch.clamp(-WRITE_PITCH_LIMIT, WRITE_PITCH_LIMIT),
        }
    }
}

/// The shortest signed angle from `from` to `target`, in degrees, in `-180..180`
/// (`Mth.wrapDegrees(target - from)`).
pub fn angle_difference(target: f32, from: f32) -> f32 {
    wrap(target - from)
}

#[cfg(test)]
mod tests {
    use super::{PLAN_PITCH_LIMIT, Rotation, Vec3, angle_difference};

    fn close(expected: f32, actual: f32) {
        assert!(
            (expected - actual).abs() < 1.0e-3,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn the_axes_are_the_games_axes() {
        let eye = Vec3::new(0.0, 0.0, 0.0);
        // +Z is yaw 0, +X is yaw -90, straight up is pitch -90.
        close(0.0, Rotation::looking_at(Vec3::new(0.0, 0.0, 5.0), eye).yaw);
        close(
            -90.0,
            Rotation::looking_at(Vec3::new(5.0, 0.0, 0.0), eye).yaw,
        );
        close(
            -180.0,
            Rotation::looking_at(Vec3::new(0.0, 0.0, -5.0), eye).yaw,
        );
        close(
            -90.0,
            Rotation::looking_at(Vec3::new(0.0, 5.0, 0.0), eye).pitch,
        );
        close(
            90.0,
            Rotation::looking_at(Vec3::new(0.0, -5.0, 0.0), eye).pitch,
        );
    }

    #[test]
    fn a_direction_vector_and_a_look_at_are_inverses() {
        for (yaw, pitch) in [(0.0, 0.0), (37.5, 12.0), (-120.0, -33.0), (179.0, 44.0)] {
            let eye = Vec3::new(12.5, 64.0, -3.25);
            let rotation = Rotation::new(yaw, pitch);
            let point = eye + rotation.direction_vector().scale(7.0);
            let back = Rotation::looking_at(point, eye);
            close(rotation.normalised().yaw, back.yaw);
            close(rotation.pitch, back.pitch);
        }
    }

    #[test]
    fn the_yaw_of_a_rotation_is_left_alone_unless_asked() {
        // The field accumulates in the game; a wrapped absolute angle is what rotation checks see.
        assert_eq!(Rotation::new(361.4, 0.0).yaw, 361.4);
        close(1.4, Rotation::new(361.4, 0.0).normalised().yaw);
    }

    #[test]
    fn the_shortest_way_round_is_the_one_taken() {
        close(2.0, angle_difference(-179.0, 179.0));
        close(-2.0, angle_difference(179.0, -179.0));
        close(10.0, angle_difference(20.0, 10.0));
        close(-10.0, angle_difference(350.0, 0.0));
    }

    #[test]
    fn both_pitch_clamps_stop_before_the_vanilla_edge() {
        assert_eq!(
            Rotation::new(0.0, 120.0).normalised().pitch,
            PLAN_PITCH_LIMIT
        );
        assert_eq!(
            Rotation::new(0.0, -120.0).normalised().pitch,
            -PLAN_PITCH_LIMIT
        );
        assert_eq!(Rotation::new(5.0, 120.0).write_clamped().pitch, 89.9);
        assert_eq!(Rotation::new(5.0, 120.0).write_clamped().yaw, 5.0);
    }
}
