//! The telemetry behind `/liquidaddon ai`: one session of the aiming core under `Engine = AI`,
//! reduced to the numbers that say whether the model is really driving.
//!
//! Pure on purpose — counters, two step traces and a formatter, no client, no module, no rotation —
//! so the line a player reads in chat is the same line a test asserts:
//!
//! ```text
//! ai 94% used | fb: init 0, model 0, nan 0, reach 6% | step 1.20° vs curve 0.85° | corr 0.71 | aiStr 100
//! ```
//!
//!  - `used` is the share of the ticks that asked the model and got an answer back; the rest fell
//!    back to the curve, and [`Fallback`] says why (engine not initialised, no model loaded, a
//!    `NaN` out of the forward pass, anything else);
//!  - `reach` is the share of the asked ticks where [`super::ai_reach`] faded the answer (blend
//!    below 1), i.e. the situation was outside what the model was fitted on and the curve took a
//!    share back;
//!  - `step vs curve` are the mean step magnitudes, in degrees, of what the model drove and of what
//!    the maths curve would have driven on the *same* tick — the pair that tells whether the model
//!    is authoring the movement or just nudging it;
//!  - `corr` is the Pearson correlation between the model's yaw step and the curve's yaw step over
//!    the last [`WINDOW`] AI ticks. A regression does not err noisily, it errs coherently: a
//!    correlate that decays is the model diverging from the law on the same input, which is exactly
//!    when the law should be trusted again (see [`super::ai_drift`]).
//!
//! Nothing here decides anything; it only counts. The guards live where they can act.

use std::collections::VecDeque;

/// Ticks of model/curve pairs the correlation is measured on (the drift guard's window).
pub const WINDOW: usize = 20;

/// Why a tick that asked the model ended up running the curve instead.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Fallback {
    /// The engine was never initialised.
    Init,
    /// No model was loaded.
    Model,
    /// The forward pass answered `NaN`.
    Nan,
    /// Anything else.
    Other,
}

/// One AI tick, as the two yaw steps it could have been: what the model drove (after the caps) and
/// what the curve would have driven on the same delta. The window correlation lives on these.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub ai_yaw: f32,
    pub curve_yaw: f32,
}

/// What [`AiStats::snapshot`] reports, and what [`format`] turns into the chat line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snapshot {
    /// Ticks that asked the model: the denominator of every percentage below.
    pub ticks: i32,
    pub used_pct: i32,
    pub fb_init: i32,
    pub fb_model: i32,
    pub fb_nan: i32,
    pub fb_other: i32,
    pub reach_pct: i32,
    pub mean_ai_step: f32,
    pub mean_curve_step: f32,
    /// Pearson's r of the current window, or `NaN` when there is nothing to correlate.
    pub corr: f32,
}

/// The counters of one session.
#[derive(Debug, Default)]
pub struct AiStats {
    requested: i32,
    used: i32,
    faded: i32,
    fb_init: i32,
    fb_model: i32,
    fb_nan: i32,
    fb_other: i32,
    ai_mag_sum: f64,
    curve_mag_sum: f64,
    steps: i32,
    window: VecDeque<Step>,
}

impl AiStats {
    /// A session that has not asked the model anything yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// A tick asked the model for a step, whatever the answer turns out to be.
    pub fn note_requested(&mut self) {
        self.requested += 1;
    }

    /// [`super::ai_reach`] faded the answer of this tick: the curve took a share of the step back.
    pub fn note_faded(&mut self) {
        self.faded += 1;
    }

    /// The tick fell back to the curve, and this is why.
    pub fn note_fallback(&mut self, kind: Fallback) {
        match kind {
            Fallback::Init => self.fb_init += 1,
            Fallback::Model => self.fb_model += 1,
            Fallback::Nan => self.fb_nan += 1,
            Fallback::Other => self.fb_other += 1,
        }
    }

    /// The model drove this tick: `ai_mag`/`curve_mag` are whole-step magnitudes in degrees.
    pub fn note_used(&mut self, ai_mag: f32, curve_mag: f32, step: Step) {
        self.used += 1;
        self.steps += 1;
        self.ai_mag_sum += f64::from(ai_mag);
        self.curve_mag_sum += f64::from(curve_mag);
        self.window.push_back(step);
        while self.window.len() > WINDOW {
            self.window.pop_front();
        }
    }

    /// Forgets the session: a reset is a new session, not a paused one.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The numbers the chat line is built from.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            ticks: self.requested,
            used_pct: percent(self.used, self.requested),
            fb_init: self.fb_init,
            fb_model: self.fb_model,
            fb_nan: self.fb_nan,
            fb_other: self.fb_other,
            reach_pct: percent(self.faded, self.requested),
            mean_ai_step: mean(self.ai_mag_sum, self.steps),
            mean_curve_step: mean(self.curve_mag_sum, self.steps),
            corr: self.window_corr(),
        }
    }

    /// The correlation of the current window: the number the drift guard acts on (see
    /// [`super::ai_drift`]).
    pub fn window_corr(&self) -> f32 {
        let ai: Vec<f32> = self.window.iter().map(|s| s.ai_yaw).collect();
        let curve: Vec<f32> = self.window.iter().map(|s| s.curve_yaw).collect();
        pearson(&ai, &curve)
    }

    /// Whether [`WINDOW`] pairs have been collected: the guard does not act on partial evidence.
    pub fn window_full(&self) -> bool {
        self.window.len() >= WINDOW
    }
}

fn percent(part: i32, whole: i32) -> i32 {
    if whole <= 0 {
        0
    } else {
        ((100.0 * f64::from(part) / f64::from(whole)) + 0.5) as i32
    }
}

fn mean(sum: f64, count: i32) -> f32 {
    if count <= 0 {
        0.0
    } else {
        (sum / f64::from(count)) as f32
    }
}

/// The chat line, exactly as [`Snapshot`] says it. A session with no AI tick yet is not a line of
/// zeroes — zeroes would read as "the model answered nothing", which is a different fact.
pub fn format(s: &Snapshot, ai_strength: f32) -> String {
    if s.ticks == 0 {
        return "nessun tick AI in questa sessione".to_string();
    }
    let fb_other = s.fb_other;
    let other = if fb_other > 0 {
        format!(", other {fb_other}")
    } else {
        String::new()
    };
    let corr = if s.corr.is_nan() {
        "n/a".to_string()
    } else {
        format!("{:.2}", s.corr)
    };
    let (used, fb_init, fb_model, fb_nan, reach) =
        (s.used_pct, s.fb_init, s.fb_model, s.fb_nan, s.reach_pct);
    let (ai_step, curve_step) = (s.mean_ai_step, s.mean_curve_step);
    let strength = (ai_strength + 0.5) as i32;
    format!(
        "ai {used}% used | fb: init {fb_init}, model {fb_model}, nan {fb_nan}{other}, \
         reach {reach}% | step {ai_step:.2}° vs curve {curve_step:.2}° | corr {corr} | \
         aiStr {strength}"
    )
}

/// Pearson's r between two traces of the same length. `NaN` when there is nothing to correlate —
/// fewer than two pairs, or a trace that never moves: a constant has no direction, and "perfectly
/// correlated" would be a lie about a dead signal.
pub fn pearson(xs: &[f32], ys: &[f32]) -> f32 {
    if xs.len() != ys.len() || xs.len() < 2 {
        return f32::NAN;
    }
    let count = xs.len() as f64;
    let mx = xs.iter().map(|v| f64::from(*v)).sum::<f64>() / count;
    let my = ys.iter().map(|v| f64::from(*v)).sum::<f64>() / count;
    let (mut cov, mut vx, mut vy) = (0.0, 0.0, 0.0);
    for (x, y) in xs.iter().zip(ys) {
        let dx = f64::from(*x) - mx;
        let dy = f64::from(*y) - my;
        cov += dx * dy;
        vx += dx * dx;
        vy += dy * dy;
    }
    let den = (vx * vy).sqrt();
    if den <= 1.0e-9 {
        return f32::NAN;
    }
    (cov / den) as f32
}

/// The reason string the aiming core produces when the model could not answer, mapped to the
/// counter a player reads. Strings on purpose: the reasons are sentences written for the debug
/// line, and the counter only needs the family of the failure.
pub fn fallback_kind(reason: &str) -> Fallback {
    if reason.contains("not initialised") {
        Fallback::Init
    } else if reason.starts_with("no model") {
        Fallback::Model
    } else if reason.contains("NaN") {
        Fallback::Nan
    } else {
        Fallback::Other
    }
}

#[cfg(test)]
mod tests {
    use super::{AiStats, Fallback, Snapshot, Step, WINDOW, fallback_kind, format, pearson};

    fn empty() -> Snapshot {
        Snapshot {
            ticks: 0,
            used_pct: 0,
            fb_init: 0,
            fb_model: 0,
            fb_nan: 0,
            fb_other: 0,
            reach_pct: 0,
            mean_ai_step: 0.0,
            mean_curve_step: 0.0,
            corr: f32::NAN,
        }
    }

    #[test]
    fn a_session_without_ai_ticks_says_so_instead_of_printing_zeroes() {
        // Zeroes would read as "the model answered nothing", which is a different fact from
        // "the engine never asked": Engine = Math, or AI never engaged this session.
        assert_eq!(format(&empty(), 100.0), "nessun tick AI in questa sessione");
    }

    #[test]
    fn the_line_is_the_example_line_number_by_number() {
        let mut stats = AiStats::new();
        for _ in 0..4 {
            stats.note_requested();
        }
        stats.note_faded();
        for i in 0..3 {
            let v = i as f32 + 1.0;
            stats.note_used(
                1.2,
                0.85,
                Step {
                    ai_yaw: v,
                    curve_yaw: v,
                },
            );
        }
        let line = format(&stats.snapshot(), 100.0);
        assert_eq!(
            line,
            "ai 75% used | fb: init 0, model 0, nan 0, reach 25% | \
             step 1.20° vs curve 0.85° | corr 1.00 | aiStr 100"
        );
    }

    #[test]
    fn a_fourth_fallback_family_is_printed_only_when_it_happens() {
        let mut stats = AiStats::new();
        stats.note_requested();
        stats.note_fallback(fallback_kind("model returned 1 values"));
        let line = format(&stats.snapshot(), 100.0);
        assert!(line.contains("nan 0, other 1, reach 0%"), "{line}");
    }

    #[test]
    fn percentages_round_they_do_not_truncate() {
        let mut stats = AiStats::new();
        for _ in 0..3 {
            stats.note_requested();
        }
        stats.note_used(
            1.0,
            1.0,
            Step {
                ai_yaw: 1.0,
                curve_yaw: 1.0,
            },
        );
        stats.note_used(
            1.0,
            1.0,
            Step {
                ai_yaw: 2.0,
                curve_yaw: 2.0,
            },
        );
        assert_eq!(stats.snapshot().used_pct, 67);
    }

    #[test]
    fn the_reasons_of_a_fallback_land_in_the_family_a_player_reads() {
        assert_eq!(
            fallback_kind("deep learning not initialised"),
            Fallback::Init
        );
        assert_eq!(fallback_kind("no model '21KC11KP' loaded"), Fallback::Model);
        assert_eq!(fallback_kind("model returned NaN"), Fallback::Nan);
        assert_eq!(fallback_kind("model returned 1 values"), Fallback::Other);
        assert_eq!(fallback_kind("ClosedStateException"), Fallback::Other);
    }

    #[test]
    fn correlation_is_one_on_a_trace_and_minus_one_against_it() {
        assert_eq!(pearson(&[1.0, 2.0, 3.0], &[2.0, 4.0, 6.0]), 1.0);
        assert_eq!(pearson(&[1.0, 2.0, 3.0], &[6.0, 4.0, 2.0]), -1.0);
    }

    #[test]
    fn a_trace_that_never_moves_has_no_correlation_to_report() {
        // A dead model output is not "perfectly in phase" with the curve: it is not a signal.
        assert!(pearson(&[2.0, 2.0, 2.0], &[1.0, 2.0, 3.0]).is_nan());
        assert!(pearson(&[1.0], &[1.0]).is_nan());
    }

    #[test]
    fn the_correlation_window_forgets_what_left_it() {
        let mut stats = AiStats::new();
        // Five ticks of a model fighting the curve, then twenty of agreement: the window is 20,
        // so the fight must be gone from the number by the time the agreement ends.
        for i in 0..5 {
            let v = i as f32 + 1.0;
            stats.note_used(
                1.0,
                1.0,
                Step {
                    ai_yaw: -v,
                    curve_yaw: v,
                },
            );
        }
        for i in 0..20 {
            let v = i as f32 + 1.0;
            stats.note_used(
                1.0,
                1.0,
                Step {
                    ai_yaw: v,
                    curve_yaw: v,
                },
            );
        }
        let corr = stats.snapshot().corr;
        assert!((corr - 1.0).abs() < 1e-3, "corr {corr}");
    }

    #[test]
    fn the_drift_guard_reads_a_window_not_a_guess() {
        let mut stats = AiStats::new();
        for i in 0..WINDOW - 1 {
            let v = i as f32;
            stats.note_used(
                1.0,
                1.0,
                Step {
                    ai_yaw: v,
                    curve_yaw: v * 2.0,
                },
            );
        }
        assert!(!stats.window_full(), "one pair short is partial evidence");
        stats.note_used(
            1.0,
            1.0,
            Step {
                ai_yaw: 99.0,
                curve_yaw: 198.0,
            },
        );
        assert!(stats.window_full(), "WINDOW pairs fill the window");
        assert_eq!(stats.snapshot().corr, stats.window_corr());
        let corr = stats.window_corr();
        assert!(
            (corr - 1.0).abs() < 1e-3,
            "the pairs are one line: corr {corr}"
        );
    }

    #[test]
    fn a_reset_is_a_new_session_not_a_paused_one() {
        let mut stats = AiStats::new();
        for _ in 0..3 {
            stats.note_requested();
        }
        stats.note_used(
            1.0,
            1.0,
            Step {
                ai_yaw: 1.0,
                curve_yaw: 1.0,
            },
        );
        stats.reset();
        assert_eq!(
            format(&stats.snapshot(), 100.0),
            "nessun tick AI in questa sessione"
        );
    }
}
