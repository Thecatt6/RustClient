//! Where on the enemy the aim is pulled to, and how hard it pulls.
//!
//! Reimplemented from `AimEngine.kt`: `pointOn`, `closestPoint`, `clampAxis`, `onCrosshair`, `bubble`,
//! `hitboxRadius`, the lead and the small periodically redrawn in-box offset. The exact closest
//! crosshair angle is LiquidBounce 0.42.0's `RotationUtil.crosshairAngleToEntity` / `maxCosToBox`:
//! it tests the ray against the box, then minimizes over the silhouette edges instead of aiming at
//! the center. All of this is arithmetic on a box, so it remains testable without a game.

use crate::rng::SplitMix64;

use super::rotation::{Rotation, Vec3};
use super::settings::AimPoint;

/// An axis-aligned box, the way the game reports a hitbox.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub const fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    /// A box around `feet` with the given width, depth and height (`Entity.getBoundingBox`).
    pub fn around(feet: Vec3, width: f64, height: f64) -> Self {
        Self {
            min: Vec3::new(feet.x - width * 0.5, feet.y, feet.z - width * 0.5),
            max: Vec3::new(feet.x + width * 0.5, feet.y + height, feet.z + width * 0.5),
        }
    }

    pub fn center(&self) -> Vec3 {
        Vec3::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
            (self.min.z + self.max.z) * 0.5,
        )
    }

    pub fn contains(&self, point: Vec3) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
            && point.z >= self.min.z
            && point.z <= self.max.z
    }

    /// `AABB.clip`: how far along `from -> to` the box starts, in `0..=1`, or `None` when the segment
    /// misses it. A segment that starts inside the box returns `0.0`.
    pub fn clip(&self, from: Vec3, to: Vec3) -> Option<f64> {
        let d = to - from;
        let mut t_min = 0.0_f64;
        let mut t_max = 1.0_f64;
        let slabs = [
            (from.x, d.x, self.min.x, self.max.x),
            (from.y, d.y, self.min.y, self.max.y),
            (from.z, d.z, self.min.z, self.max.z),
        ];
        for (origin, direction, low, high) in slabs {
            if direction.abs() < 1.0e-12 {
                // Parallel to this pair of faces: the segment has to start between them.
                if origin < low || origin > high {
                    return None;
                }
                continue;
            }
            let mut t1 = (low - origin) / direction;
            let mut t2 = (high - origin) / direction;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            t_min = t_min.max(t1);
            t_max = t_max.min(t2);
            if t_min > t_max {
                return None;
            }
        }
        if t_min > 1.0 { None } else { Some(t_min) }
    }
}

/// The enemy as the aim sees it: its box, its eyes and its feet.
///
/// In LiquidBounce these are three calls on the entity; keeping them in one struct is what lets the
/// point selection below be a pure function.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TargetShape {
    pub bbox: Aabb,
    pub eye: Vec3,
    pub feet: Vec3,
    /// `bbHeight`, used by the chest point.
    pub height: f32,
}

/// The source's aim point before adding its random in-box offset.
pub fn point_on(aim_point: AimPoint, shape: &TargetShape, eyes: Vec3, aim: Rotation) -> Vec3 {
    match aim_point {
        AimPoint::Head => shape.eye,
        AimPoint::Chest => shape.feet + Vec3::new(0.0, f64::from(shape.height) * 0.6, 0.0),
        AimPoint::Closest => closest_point(eyes, aim, &shape.bbox),
    }
}

/// The point of the hitbox the current aim is already pointing at, clamped inside the box.
///
/// Aiming at the part of the enemy you were already looking at is what makes the assist feel like it
/// is helping *your* aim; pulling towards the geometric centre instead makes the crosshair slide to
/// the middle of the hitbox, which every rotation check reads as a magnet.
pub fn closest_point(eyes: Vec3, aim: Rotation, bbox: &Aabb) -> Vec3 {
    let center = bbox.center();
    let distance = eyes.distance_to(center);

    // Point blank: a box that surrounds the camera would make the aim swing tens of degrees for a
    // few centimetres of movement. The centre moves smoothly with the entity.
    if distance < 1.5 {
        return center;
    }

    let hit = eyes + aim.direction_vector().scale(distance);
    Vec3::new(
        clamp_axis(hit.x, bbox.min.x + 0.12, bbox.max.x - 0.12),
        clamp_axis(hit.y, bbox.min.y + 0.10, bbox.max.y - 0.12),
        clamp_axis(hit.z, bbox.min.z + 0.12, bbox.max.z - 0.12),
    )
}

/// Clamp the random aim offset back inside the hitbox, with the Kotlin engine's two-centimetre
/// inset (and a midpoint fallback for a hitbox thinner than that margin).
pub fn clamp_point_with_offset(point: Vec3, offset: Vec3, bbox: &Aabb) -> Vec3 {
    let low_x = bbox.min.x + 0.02;
    let low_y = bbox.min.y + 0.02;
    let low_z = bbox.min.z + 0.02;
    Vec3::new(
        clamp_axis(point.x + offset.x, low_x, (bbox.max.x - 0.02).max(low_x)),
        clamp_axis(point.y + offset.y, low_y, (bbox.max.y - 0.02).max(low_y)),
        clamp_axis(point.z + offset.z, low_z, (bbox.max.z - 0.02).max(low_z)),
    )
}

/// A small random offset that is held for 150–399 ms before the source redraws it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AimOffset {
    until_ms: u64,
    offset: Vec3,
}

impl AimOffset {
    pub const fn new() -> Self {
        Self {
            until_ms: 0,
            offset: Vec3::new(0.0, 0.0, 0.0),
        }
    }

    pub fn reset(&mut self) {
        self.until_ms = 0;
        self.offset = Vec3::new(0.0, 0.0, 0.0);
    }

    /// Return the held offset, redrawing from the current box when its deadline passes.
    pub fn sample(&mut self, now_ms: u64, bbox: &Aabb, rng: &mut SplitMix64) -> Vec3 {
        if now_ms > self.until_ms {
            let size = bbox.max - bbox.min;
            self.until_ms = now_ms.saturating_add(150 + (rng.next_u64() % 250));
            self.offset = Vec3::new(
                (rng.next_f64() - 0.5) * size.x * 0.24,
                (rng.next_f64() - 0.5) * size.y * 0.18,
                (rng.next_f64() - 0.5) * size.z * 0.24,
            );
        }
        self.offset
    }
}

/// Add a one-tick velocity lead (`predict * predict_size`) after the in-box offset, as in
/// `AimassistCore.goalRotation`.
pub fn predict_point(point: Vec3, velocity: Vec3, predict: f32, predict_size: i32) -> Vec3 {
    if predict > 0.0 && predict_size > 0 {
        point + velocity.scale(f64::from(predict) * f64::from(predict_size))
    } else {
        point
    }
}

/// `coerceIn` that also survives a box thinner than the margin.
pub fn clamp_axis(value: f64, min: f64, max: f64) -> f64 {
    if min > max {
        (min + max) * 0.5
    } else {
        value.clamp(min, max)
    }
}

/// True while the crosshair ray crosses the hitbox (or the box contains the eyes), with the
/// source's `max(entityInteractionRange, configuredRange) + 0.5` reach.
pub fn on_crosshair(
    eyes: Vec3,
    view: Rotation,
    bbox: &Aabb,
    interaction_range: f64,
    configured_range: f64,
) -> bool {
    if bbox.contains(eyes) {
        return true;
    }
    let reach_length = interaction_range.max(configured_range) + 0.5;
    let end = eyes + view.direction_vector().scale(reach_length);
    bbox.clip(eyes, end).is_some()
}

/// Maximum cosine between the unit crosshair vector and any direction from `eyes` to the box.
///
/// This reproduces LiquidBounce v0.42.0's `maxCosToBox`: a ray hit is a cosine of one; on a miss, the
/// closest direction lies on a silhouette edge and the stationary point of each candidate edge is
/// evaluated along with its endpoints.
pub fn max_cos_to_box(eyes: Vec3, view: Vec3, bbox: &Aabb) -> f64 {
    if ray_hits_box(eyes, view, bbox) {
        return 1.0;
    }

    let x_lo_front = eyes.x < bbox.min.x;
    let x_hi_front = eyes.x > bbox.max.x;
    let y_lo_front = eyes.y < bbox.min.y;
    let y_hi_front = eyes.y > bbox.max.y;
    let z_lo_front = eyes.z < bbox.min.z;
    let z_hi_front = eyes.z > bbox.max.z;

    let mut best = -1.0_f64;

    for yi in 0..=1 {
        let y = if yi == 0 { bbox.min.y } else { bbox.max.y };
        let y_front = if yi == 0 { y_lo_front } else { y_hi_front };
        for zi in 0..=1 {
            let z_front = if zi == 0 { z_lo_front } else { z_hi_front };
            if y_front == z_front {
                continue;
            }
            let z = if zi == 0 { bbox.min.z } else { bbox.max.z };
            best = best.max(edge_max_cos(
                eyes,
                view,
                Vec3::new(bbox.min.x, y, z),
                Vec3::new(bbox.max.x - bbox.min.x, 0.0, 0.0),
            ));
        }
    }

    for xi in 0..=1 {
        let x = if xi == 0 { bbox.min.x } else { bbox.max.x };
        let x_front = if xi == 0 { x_lo_front } else { x_hi_front };
        for zi in 0..=1 {
            let z_front = if zi == 0 { z_lo_front } else { z_hi_front };
            if x_front == z_front {
                continue;
            }
            let z = if zi == 0 { bbox.min.z } else { bbox.max.z };
            best = best.max(edge_max_cos(
                eyes,
                view,
                Vec3::new(x, bbox.min.y, z),
                Vec3::new(0.0, bbox.max.y - bbox.min.y, 0.0),
            ));
        }
    }

    for xi in 0..=1 {
        let x = if xi == 0 { bbox.min.x } else { bbox.max.x };
        let x_front = if xi == 0 { x_lo_front } else { x_hi_front };
        for yi in 0..=1 {
            let y_front = if yi == 0 { y_lo_front } else { y_hi_front };
            if x_front == y_front {
                continue;
            }
            let y = if yi == 0 { bbox.min.y } else { bbox.max.y };
            best = best.max(edge_max_cos(
                eyes,
                view,
                Vec3::new(x, y, bbox.min.z),
                Vec3::new(0.0, 0.0, bbox.max.z - bbox.min.z),
            ));
        }
    }
    best
}

/// Degrees to the closest direction toward the hitbox (`RotationUtil.crosshairAngleToEntity`).
pub fn crosshair_angle_to_box(eyes: Vec3, view: Rotation, bbox: &Aabb) -> f32 {
    max_cos_to_box(eyes, view.direction_vector(), bbox)
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees() as f32
}

/// How wide the hitbox is, seen from the eyes, in degrees (`hitboxRadius`).
pub fn hitbox_radius(eyes: Vec3, bbox: &Aabb) -> f32 {
    let distance = eyes.distance_to(bbox.center()).max(0.1);
    let half = (bbox.max.x - bbox.min.x).max(bbox.max.y - bbox.min.y) * 0.5;
    half.atan2(distance).to_degrees() as f32
}

/// The weight of the assist, `0..=1` (`bubble`).
///
/// `1.0` while the crosshair is on the hitbox, then a straight falloff to `0` at the edge of the
/// cone: the pull is strongest exactly where the player cannot see it — they are already aiming at
/// the target — and vanishes at the distance at which it would become a snap.
pub fn bubble(
    on_crosshair: bool,
    auto_aim: bool,
    offset_degrees: f32,
    radius_degrees: f32,
    fov_degrees: f32,
) -> f32 {
    if auto_aim || on_crosshair {
        return 1.0;
    }
    let edge = fov_degrees / 2.0;
    if edge <= radius_degrees {
        return 0.0;
    }
    if offset_degrees <= radius_degrees {
        return 1.0;
    }
    (1.0 - (offset_degrees - radius_degrees) / (edge - radius_degrees)).clamp(0.0, 1.0)
}

fn ray_hits_box(eyes: Vec3, view: Vec3, bbox: &Aabb) -> bool {
    let mut t_min = 0.0_f64;
    let mut t_max = f64::INFINITY;
    let slabs = [
        (eyes.x, view.x, bbox.min.x, bbox.max.x),
        (eyes.y, view.y, bbox.min.y, bbox.max.y),
        (eyes.z, view.z, bbox.min.z, bbox.max.z),
    ];
    for (origin, direction, low, high) in slabs {
        if direction != 0.0 {
            let t1 = (low - origin) / direction;
            let t2 = (high - origin) / direction;
            t_min = t_min.max(t1.min(t2));
            t_max = t_max.min(t1.max(t2));
            if t_min > t_max {
                return false;
            }
        } else if origin < low || origin > high {
            return false;
        }
    }
    true
}

fn edge_max_cos(eyes: Vec3, view: Vec3, start: Vec3, direction: Vec3) -> f64 {
    let relative = start - eyes;
    let a = relative.dot(view);
    let b = direction.dot(view);
    let c = relative.dot(relative);
    let d = relative.dot(direction);
    let e = direction.dot(direction);

    let mut best = cos_at(a, c).max(cos_at(a + b, c + 2.0 * d + e));
    // A zero denominator yields NaN or infinity, both rejected by the open-interval test.
    let t = (a * d - b * c) / (b * d - a * e);
    if t > 0.0 && t < 1.0 {
        best = best.max(cos_at(a + b * t, c + 2.0 * d * t + e * t * t));
    }
    best
}

fn cos_at(along: f64, length_squared: f64) -> f64 {
    if length_squared <= 0.0 {
        1.0
    } else {
        along / length_squared.sqrt()
    }
}

#[cfg(test)]
mod tests {
    use crate::rng::SplitMix64;

    use super::{
        Aabb, AimOffset, TargetShape, bubble, clamp_axis, clamp_point_with_offset, closest_point,
        crosshair_angle_to_box, hitbox_radius, max_cos_to_box, on_crosshair, point_on,
        predict_point,
    };
    use crate::aim::rotation::{Rotation, Vec3};
    use crate::aim::settings::AimPoint;

    fn box_0_1() -> Aabb {
        Aabb::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 2.0, 1.0))
    }

    fn close(expected: f32, actual: f32) {
        assert!(
            (expected - actual).abs() < 1.0e-3,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn a_ray_enters_the_box_where_the_slabs_say() {
        let hit = box_0_1().clip(Vec3::new(0.5, 0.5, -5.0), Vec3::new(0.5, 0.5, 5.0));
        close(0.5, hit.expect("the ray crosses the box") as f32);
        assert_eq!(
            box_0_1().clip(Vec3::new(0.5, 0.5, 0.5), Vec3::new(5.0, 5.0, 5.0)),
            Some(0.0)
        );
        assert_eq!(
            box_0_1().clip(Vec3::new(0.5, 5.0, -5.0), Vec3::new(0.5, 5.0, 5.0)),
            None
        );
        assert_eq!(
            box_0_1().clip(Vec3::new(0.5, 0.5, -5.0), Vec3::new(0.5, 0.5, -4.0)),
            None
        );
    }

    #[test]
    fn the_box_reports_what_it_holds() {
        assert!(box_0_1().contains(Vec3::new(0.5, 1.0, 0.5)));
        assert!(!box_0_1().contains(Vec3::new(1.5, 1.0, 0.5)));
        assert_eq!(box_0_1().center(), Vec3::new(0.5, 1.0, 0.5));
    }

    #[test]
    fn a_clamp_survives_a_box_thinner_than_the_margin() {
        assert!((clamp_axis(5.0, 0.4, 0.2) - 0.3).abs() < 1.0e-12);
        assert_eq!(clamp_axis(5.0, 0.0, 1.0), 1.0);
    }

    #[test]
    fn point_blank_the_aim_takes_the_centre_not_a_corner() {
        let eyes = Vec3::new(0.5, 1.0, 0.2);
        let point = closest_point(eyes, Rotation::new(0.0, 0.0), &box_0_1());
        assert_eq!(point, box_0_1().center());
    }

    #[test]
    fn the_closest_point_stays_inside_the_hitbox() {
        let eyes = Vec3::new(0.5, 1.0, 40.0);
        let point = closest_point(eyes, Rotation::new(0.0, 0.0), &box_0_1());
        assert!(box_0_1().contains(point));
        close(1.0, point.z as f32 + 0.12);
        close(1.0, point.y as f32);
    }

    #[test]
    fn the_three_aim_points_are_the_three_the_source_names() {
        let shape = TargetShape {
            bbox: box_0_1(),
            eye: Vec3::new(0.5, 1.62, 0.5),
            feet: Vec3::new(0.5, 0.0, 0.5),
            height: 1.8,
        };
        let eyes = Vec3::new(0.5, 1.62, -4.0);
        let aim = Rotation::new(0.0, 0.0);
        assert_eq!(point_on(AimPoint::Head, &shape, eyes, aim), shape.eye);
        close(1.08, point_on(AimPoint::Chest, &shape, eyes, aim).y as f32);
        let closest = point_on(AimPoint::Closest, &shape, eyes, aim);
        assert!(shape.bbox.contains(closest));
    }

    #[test]
    fn random_offset_is_held_then_clamped_back_inside_and_prediction_is_after_it() {
        let bbox = box_0_1();
        let mut offset = AimOffset::new();
        let mut rng = SplitMix64::new(17);
        let first = offset.sample(1, &bbox, &mut rng);
        assert_eq!(
            offset.sample(100, &bbox, &mut rng),
            first,
            "the offset is held for 150ms or more"
        );
        let point = clamp_point_with_offset(Vec3::new(0.5, 1.0, 0.5), first, &bbox);
        assert!(bbox.contains(point));
        assert_eq!(
            predict_point(point, Vec3::new(1.0, 0.0, -2.0), 0.25, 2),
            Vec3::new(point.x + 0.5, point.y, point.z - 1.0)
        );
        assert_eq!(
            predict_point(point, Vec3::new(1.0, 0.0, 0.0), 0.25, 0),
            point
        );
        let second = offset.sample(10_000, &bbox, &mut rng);
        assert_ne!(second, first, "the offset is redrawn after its deadline");
    }

    #[test]
    fn crosshair_test_uses_ray_hit_and_the_same_extended_interaction_reach() {
        let bbox = Aabb::new(Vec3::new(-0.5, -0.5, 5.0), Vec3::new(0.5, 0.5, 6.0));
        let eye = Vec3::new(0.0, 0.0, 0.0);
        assert!(on_crosshair(eye, Rotation::new(0.0, 0.0), &bbox, 3.0, 6.0));
        assert!(!on_crosshair(
            eye,
            Rotation::new(90.0, 0.0),
            &bbox,
            3.0,
            6.0
        ));
        assert!(!on_crosshair(eye, Rotation::new(0.0, 0.0), &bbox, 3.0, 3.0));
        assert!(on_crosshair(
            bbox.center(),
            Rotation::new(180.0, 0.0),
            &bbox,
            0.0,
            0.0
        ));
    }

    #[test]
    fn crosshair_angle_is_zero_on_the_box_and_tracks_its_silhouette_not_its_centre() {
        let bbox = Aabb::new(Vec3::new(-1.0, -1.0, 5.0), Vec3::new(1.0, 1.0, 6.0));
        let eye = Vec3::new(0.0, 0.0, 0.0);
        let view = Rotation::new(0.0, 0.0);
        assert_eq!(max_cos_to_box(eye, view.direction_vector(), &bbox), 1.0);
        assert_eq!(crosshair_angle_to_box(eye, view, &bbox), 0.0);
        // The view is above the centre but still points to the box's near top edge, so the
        // closest-angle test is smaller than aiming at the centre's angular difference.
        let above = Rotation::new(0.0, -15.0);
        let angle = crosshair_angle_to_box(eye, above, &bbox);
        assert!(
            (0.0..15.0).contains(&angle),
            "the box spans the crosshair: {angle}°"
        );
    }

    #[test]
    fn max_cos_matches_liquidbounce_for_inside_silhouette_and_behind_box_cases() {
        let bbox = Aabb::new(Vec3::new(1.0, 0.5, -1.0), Vec3::new(2.0, 1.5, 1.0));
        let forward = Vec3::new(1.0, 0.0, 0.0);

        assert_eq!(
            max_cos_to_box(Vec3::new(1.5, 1.0, 0.0), Vec3::new(0.0, 1.0, 0.0), &bbox),
            1.0
        );
        let edge = max_cos_to_box(Vec3::default(), forward, &bbox);
        assert!((edge - 2.0 / 4.25_f64.sqrt()).abs() < 1.0e-12, "got {edge}");
        let behind = max_cos_to_box(Vec3::new(20.0, 1.0, 0.0), forward, &bbox);
        assert!(
            (behind - (-18.0 / 325.25_f64.sqrt())).abs() < 1.0e-12,
            "got {behind}"
        );
    }

    #[test]
    fn a_hitbox_has_an_angular_radius() {
        let bbox = Aabb::new(Vec3::new(-0.3, -0.3, 9.5), Vec3::new(0.3, 0.3, 10.5));
        close(1.718, hitbox_radius(Vec3::new(0.0, 0.0, 0.0), &bbox));
        let near = Aabb::new(Vec3::new(-0.3, -0.3, 0.0), Vec3::new(0.3, 0.3, 1.0));
        assert!(hitbox_radius(Vec3::new(0.0, 0.0, 0.5), &near).is_finite());
    }

    #[test]
    fn the_bubble_is_full_on_the_crosshair_and_gone_at_the_edge() {
        assert_eq!(bubble(true, false, 20.0, 3.0, 30.0), 1.0);
        assert_eq!(
            bubble(false, true, 20.0, 3.0, 30.0),
            1.0,
            "auto aim never fades"
        );
        assert_eq!(
            bubble(false, false, 2.0, 3.0, 30.0),
            1.0,
            "inside the hitbox"
        );
        assert_eq!(
            bubble(false, false, 15.0, 3.0, 30.0),
            0.0,
            "the edge of the cone"
        );
        close(0.5, bubble(false, false, 9.0, 3.0, 30.0));
        assert_eq!(
            bubble(false, false, 9.0, 20.0, 30.0),
            0.0,
            "a cone narrower than the box pulls nothing"
        );
    }
}
