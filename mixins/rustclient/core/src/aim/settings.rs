//! The setting vocabulary shared by the Aimassist/AimbotAI math engine.
//!
//! `AimSettings` mirrors the read-only interface in `AimEngine.kt`; [`AimConfig`] is the Rust
//! settings value that a Horizon adapter loads and updates. The game-independent crate deliberately
//! does not know about Horizon's `Game`, file paths, GUI batches or key events.
//!
//! The user-facing rotation choice is [`RotationMode`], not a build-time flag. The Horizon bundle
//! persists it under [`ROTATION_MODE_KEY`] and exposes it as a button on Options > Mods > RustClient.
//! The core can compute a step without deciding whether it is written to the camera or only to the
//! outgoing rotation: applying that step is the rotation adapter's job.

use std::fmt;

use super::rotation::DEFAULT_PITCH_STEP_MAX;

/// The setting file name stored by Horizon's `setting` / `set_setting` helpers.
///
/// It is deliberately a single safe filename component: Horizon stores one setting per file in
/// `horizon-mods/.horizon-mod-data/rustclient/` and does not create nested parents for keys
/// containing `/`.
pub const ROTATION_MODE_KEY: &str = "aim_rotation_mode";

macro_rules! tagged_enum {
    ($(#[$attr:meta])* $name:ident { $($(#[$variant_attr:meta])* $variant:ident => $tag:literal),+ $(,)? }) => {
        $(#[$attr])*
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub enum $name {
            $($(#[$variant_attr])* $variant),+
        }

        impl $name {
            /// The display/persistence tag used in the source GUI.
            pub const fn tag(self) -> &'static str {
                match self {
                    $(Self::$variant => $tag),+
                }
            }

            /// All choices in their GUI order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// Parse a GUI/persisted tag, accepting case differences but no unknown values.
            pub fn parse_tag(tag: &str) -> Option<Self> {
                $(if tag.eq_ignore_ascii_case($tag) { return Some(Self::$variant); })+
                None
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.tag())
            }
        }
    };
}

tagged_enum!(Mode {
    Assist => "Assist",
    Friction => "Friction",
    Both => "Both",
});

tagged_enum!(Priority {
    Fov => "Fov",
    Distance => "Distance",
    Health => "Health",
    Armor => "Armor",
});

tagged_enum!(Smoothing {
    Linear => "Linear",
    Bezier => "Bezier",
    Sigmoid => "Sigmoid",
});

tagged_enum!(Engine {
    Math => "Math",
    Ai => "AI",
});

tagged_enum!(AiModel {
    Model19kc8kp => "19KC8KP",
    Model21kc11kp => "21KC11KP",
    Client => "Client",
});

tagged_enum!(AiInputPoint {
    Eyes => "Eyes",
    Assist => "Assist",
});

tagged_enum!(AiHandMode {
    On => "On",
    Off => "Off",
});

tagged_enum!(AimPoint {
    Closest => "Closest",
    Head => "Head",
    Chest => "Chest",
});

tagged_enum!(MoveFix {
    Off => "Off",
    Strict => "Strict",
    Silent => "Silent",
});

tagged_enum!(Preset {
    Custom => "Custom",
    Blatant => "Blatant",
    Legit => "Legit",
    Ghost => "Ghost",
    Closet => "Closet",
    Ai => "AI",
    ProfileA => "Profile A",
    ProfileB => "Profile B",
});

tagged_enum!(
    /// Where a computed rotation is applied. Unlike the source's `Silent` boolean, this is a named
    /// choice and is editable at runtime from the native mod settings page.
    #[derive(Default)]
    RotationMode {
        #[default]
        Visible => "Visible",
        Silent => "Silent",
    }
);

impl RotationMode {
    /// The next GUI choice (there are exactly two).
    pub const fn next(self) -> Self {
        match self {
            Self::Visible => Self::Silent,
            Self::Silent => Self::Visible,
        }
    }

    pub const fn is_silent(self) -> bool {
        matches!(self, Self::Silent)
    }
}

/// Minimal read/write interface for the persisted rotation-mode setting.
///
/// Core tests use an in-memory implementation; the Horizon hooks use the corresponding
/// `native_setting` and `native_set_setting` helpers. Keeping the trait tiny lets parsing, defaults
/// and round-trips be tested without importing Horizon types.
pub trait SettingStore {
    fn setting(&self, key: &str) -> Option<String>;
    fn set_setting(&mut self, key: &str, value: &str);
}

/// Read the persisted mode, falling back to visible for a fresh install or an unknown old value.
pub fn load_rotation_mode(store: &impl SettingStore) -> RotationMode {
    store
        .setting(ROTATION_MODE_KEY)
        .as_deref()
        .and_then(RotationMode::parse_tag)
        .unwrap_or_default()
}

/// Persist a user-selected rotation mode under the stable key.
pub fn save_rotation_mode(store: &mut impl SettingStore, mode: RotationMode) {
    store.set_setting(ROTATION_MODE_KEY, mode.tag());
}

/// The source `AimSettings` interface, including the AI-facing values for the later model branch.
///
/// The `values` indirection lets both modules expose one shared [`AimConfig`] while retaining their
/// own module-specific setting adapters. Methods are values, rather than references to fields, so
/// a caller can supply a different settings provider without copying engine policy into the
/// module. `rotation_mode` supersedes the old compile-time/boolean `silent` switch.
pub trait AimSettings {
    fn values(&self) -> &AimConfig;

    fn range(&self) -> f32 {
        self.values().range
    }
    fn fov(&self) -> f32 {
        self.values().fov
    }
    fn priority(&self) -> Priority {
        self.values().priority
    }
    fn through_walls(&self) -> bool {
        self.values().through_walls
    }
    fn anti_bot(&self) -> bool {
        self.values().anti_bot
    }
    fn min_health(&self) -> i32 {
        self.values().min_health
    }
    fn lock_target(&self) -> bool {
        self.values().lock_target
    }
    fn switch_delay(&self) -> i32 {
        self.values().switch_delay
    }

    fn mode(&self) -> Mode {
        self.values().mode
    }
    fn strength(&self) -> f32 {
        self.values().strength
    }
    fn max_speed(&self) -> f32 {
        self.values().max_speed
    }
    fn smoothing(&self) -> Smoothing {
        self.values().smoothing
    }
    fn engine(&self) -> Engine {
        self.values().engine
    }
    fn ai_model(&self) -> AiModel {
        self.values().ai_model
    }
    fn ai_input_point(&self) -> AiInputPoint {
        self.values().ai_input_point
    }
    fn ai_hand(&self) -> AiHandMode {
        self.values().ai_hand
    }
    fn ai_yaw_multiplier(&self) -> f32 {
        self.values().ai_yaw_multiplier
    }
    fn ai_pitch_multiplier(&self) -> f32 {
        self.values().ai_pitch_multiplier
    }
    fn ai_correction(&self) -> f32 {
        self.values().ai_correction
    }
    fn ai_strength(&self) -> f32 {
        self.values().ai_strength
    }

    fn pitch_assist(&self) -> f32 {
        self.values().pitch_assist
    }
    fn aim_point(&self) -> AimPoint {
        self.values().aim_point
    }
    fn predict(&self) -> f32 {
        self.values().predict
    }
    fn predict_size(&self) -> i32 {
        self.values().predict_size
    }
    fn only_on_crosshair(&self) -> bool {
        self.values().only_on_crosshair
    }
    fn turn_check(&self) -> bool {
        self.values().turn_check
    }
    fn friction(&self) -> f32 {
        self.values().friction
    }
    fn dead_zone(&self) -> f32 {
        self.values().dead_zone
    }
    fn reaction_delay(&self) -> i32 {
        self.values().reaction_delay
    }
    fn reaction_random(&self) -> i32 {
        self.values().reaction_random
    }
    fn fade_in(&self) -> i32 {
        self.values().fade_in
    }
    fn idle_chance(&self) -> i32 {
        self.values().idle_chance
    }
    fn jitter_yaw(&self) -> f32 {
        self.values().jitter_yaw
    }
    fn jitter_pitch(&self) -> f32 {
        self.values().jitter_pitch
    }
    fn gcd_fix(&self) -> bool {
        self.values().gcd_fix
    }
    fn gcd_dither(&self) -> bool {
        self.values().gcd_dither
    }

    fn pitch_step_max(&self) -> f32 {
        DEFAULT_PITCH_STEP_MAX
    }
    fn rest_beats(&self) -> bool {
        true
    }
    fn mouse_noise(&self) -> i32 {
        self.values().mouse_noise
    }
    fn overshoot(&self) -> i32 {
        self.values().overshoot
    }
    fn strength_spread(&self) -> i32 {
        self.values().strength_spread
    }
    fn pattern_guard(&self) -> bool {
        self.values().pattern_guard
    }
    fn hand_only(&self) -> bool {
        self.values().hand_only
    }
    fn hand_threshold(&self) -> f32 {
        self.values().hand_threshold
    }
    fn hand_fade(&self) -> i32 {
        self.values().hand_fade
    }

    fn rotation_mode(&self) -> RotationMode {
        self.values().rotation_mode
    }
    fn silent(&self) -> bool {
        self.rotation_mode().is_silent()
    }
    fn move_fix(&self) -> MoveFix {
        self.values().move_fix
    }
    fn frame_interpolation(&self) -> bool {
        self.values().frame_interpolation
    }
    fn pause_on_inventory(&self) -> bool {
        self.values().pause_on_inventory
    }
    fn pause_on_use(&self) -> bool {
        self.values().pause_on_use
    }
    fn chat_log(&self) -> bool {
        self.values().chat_log
    }
    fn debug(&self) -> bool {
        self.values().debug
    }
    fn preset_name(&self) -> &'static str {
        self.values().preset.tag()
    }
    fn attack_sync(&self) -> bool {
        self.values().attack_sync
    }
    fn auto_aim(&self) -> bool {
        self.values().auto_aim
    }
}

/// The shared settings values, with the Kotlin `AimValues` defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct AimConfig {
    pub range: f32,
    pub fov: f32,
    pub priority: Priority,
    pub through_walls: bool,
    pub anti_bot: bool,
    pub min_health: i32,
    pub lock_target: bool,
    pub switch_delay: i32,

    pub preset: Preset,
    pub mode: Mode,
    pub strength: f32,
    pub max_speed: f32,
    pub smoothing: Smoothing,
    pub engine: Engine,
    pub ai_model: AiModel,
    pub ai_input_point: AiInputPoint,
    pub ai_hand: AiHandMode,
    pub ai_yaw_multiplier: f32,
    pub ai_pitch_multiplier: f32,
    pub ai_correction: f32,
    pub ai_strength: f32,

    pub pitch_assist: f32,
    pub aim_point: AimPoint,
    pub predict: f32,
    pub predict_size: i32,
    pub only_on_crosshair: bool,
    pub turn_check: bool,
    pub friction: f32,
    pub dead_zone: f32,
    pub reaction_delay: i32,
    pub reaction_random: i32,
    pub fade_in: i32,
    pub idle_chance: i32,
    pub jitter_yaw: f32,
    pub jitter_pitch: f32,
    pub gcd_fix: bool,
    pub gcd_dither: bool,

    pub mouse_noise: i32,
    pub overshoot: i32,
    pub strength_spread: i32,
    pub pattern_guard: bool,
    pub hand_only: bool,
    pub hand_threshold: f32,
    pub hand_fade: i32,

    pub rotation_mode: RotationMode,
    pub move_fix: MoveFix,
    pub frame_interpolation: bool,
    pub pause_on_inventory: bool,
    pub pause_on_use: bool,
    pub chat_log: bool,
    pub debug: bool,
    pub attack_sync: bool,
    pub auto_aim: bool,
}

impl Default for AimConfig {
    fn default() -> Self {
        Self {
            range: 3.4,
            fov: 24.0,
            priority: Priority::Fov,
            through_walls: false,
            anti_bot: true,
            min_health: 0,
            lock_target: true,
            switch_delay: 450,

            preset: Preset::ProfileA,
            mode: Mode::Both,
            strength: 35.0,
            max_speed: 7.0,
            smoothing: Smoothing::Bezier,
            engine: Engine::Math,
            ai_model: AiModel::Model19kc8kp,
            ai_input_point: AiInputPoint::Eyes,
            ai_hand: AiHandMode::On,
            ai_yaw_multiplier: 1.2,
            ai_pitch_multiplier: 1.0,
            ai_correction: 10.0,
            ai_strength: 100.0,

            pitch_assist: 50.0,
            aim_point: AimPoint::Closest,
            predict: 0.05,
            predict_size: 2,
            only_on_crosshair: true,
            turn_check: true,
            friction: 20.0,
            dead_zone: 0.4,
            reaction_delay: 200,
            reaction_random: 100,
            fade_in: 300,
            idle_chance: 15,
            jitter_yaw: 0.1,
            jitter_pitch: 0.06,
            gcd_fix: true,
            gcd_dither: true,

            mouse_noise: 2,
            overshoot: 10,
            strength_spread: 20,
            pattern_guard: true,
            hand_only: true,
            hand_threshold: 1.0,
            hand_fade: 4,

            rotation_mode: RotationMode::Visible,
            move_fix: MoveFix::Silent,
            frame_interpolation: true,
            pause_on_inventory: true,
            pause_on_use: false,
            chat_log: false,
            debug: false,
            attack_sync: true,
            auto_aim: false,
        }
    }
}

impl AimSettings for AimConfig {
    fn values(&self) -> &AimConfig {
        self
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{
        AimConfig, AimSettings, Mode, Preset, ROTATION_MODE_KEY, RotationMode, SettingStore,
        Smoothing, load_rotation_mode, save_rotation_mode,
    };

    #[derive(Default)]
    struct MemorySettings(HashMap<String, String>);

    impl SettingStore for MemorySettings {
        fn setting(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }

        fn set_setting(&mut self, key: &str, value: &str) {
            self.0.insert(key.to_owned(), value.to_owned());
        }
    }

    #[test]
    fn the_shared_aim_config_keeps_the_source_defaults() {
        let config = AimConfig::default();
        assert_eq!(config.range(), 3.4);
        assert_eq!(config.fov(), 24.0);
        assert_eq!(config.mode(), Mode::Both);
        assert_eq!(config.strength(), 35.0);
        assert_eq!(config.max_speed(), 7.0);
        assert_eq!(config.smoothing(), Smoothing::Bezier);
        assert_eq!(config.engine().tag(), "Math");
        assert_eq!(config.preset_name(), "Profile A");
        assert!(config.hand_only());
        assert!(config.pattern_guard());
        assert!(!config.silent());
        assert_eq!(config.rotation_mode(), RotationMode::Visible);
    }

    #[test]
    fn all_enum_tags_round_trip_without_changing_source_labels() {
        for tag in ["Assist", "Friction", "Both"] {
            assert_eq!(Mode::parse_tag(tag).expect("known mode").tag(), tag);
        }
        for tag in ["Linear", "Bezier", "Sigmoid"] {
            assert_eq!(Smoothing::parse_tag(tag).expect("known curve").tag(), tag);
        }
        for tag in [
            "Custom",
            "Blatant",
            "Legit",
            "Ghost",
            "Closet",
            "AI",
            "Profile A",
            "Profile B",
        ] {
            assert_eq!(Preset::parse_tag(tag).expect("known preset").tag(), tag);
        }
        assert_eq!(
            RotationMode::parse_tag("sIlEnT"),
            Some(RotationMode::Silent)
        );
        assert_eq!(RotationMode::parse_tag("not-a-mode"), None);
    }

    #[test]
    fn the_rotation_mode_persists_and_unknown_values_fail_open_to_visible() {
        let mut store = MemorySettings::default();
        assert_eq!(load_rotation_mode(&store), RotationMode::Visible);

        save_rotation_mode(&mut store, RotationMode::Silent);
        assert_eq!(store.setting(ROTATION_MODE_KEY).as_deref(), Some("Silent"));
        assert_eq!(load_rotation_mode(&store), RotationMode::Silent);

        store.set_setting(ROTATION_MODE_KEY, "future-mode");
        assert_eq!(load_rotation_mode(&store), RotationMode::Visible);
    }

    #[test]
    fn the_rotation_mode_is_a_runtime_choice_with_a_stable_toggle() {
        assert_eq!(RotationMode::default(), RotationMode::Visible);
        assert_eq!(RotationMode::Visible.next(), RotationMode::Silent);
        assert_eq!(RotationMode::Silent.next(), RotationMode::Visible);

        let config = AimConfig {
            rotation_mode: RotationMode::Silent,
            ..AimConfig::default()
        };
        assert!(config.silent());
        assert_eq!(config.rotation_mode(), RotationMode::Silent);
    }
}
