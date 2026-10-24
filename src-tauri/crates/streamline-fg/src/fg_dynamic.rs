//! Vulkan fallback when native Dynamic MFG is unavailable. Measure new color
//! arrivals, not generated outputs; retain the chosen multiplier on exact repeats.
use crate::advanced_settings::{FgMode, FgOptions};
use std::time::Duration;

#[derive(Default)]
pub(super) struct Dynamic {
    previous: Option<Duration>,
    interval: Option<f64>,
    present_previous: Option<Duration>,
    present_interval: Option<f64>,
    changed: Option<Duration>,
    multiplier: u8,
}
impl Dynamic {
    pub(super) fn configure(
        &mut self,
        tuning: FgOptions,
        native_supported: bool,
        target: u32,
        now: Duration,
        active: bool,
        new_color: bool,
    ) -> (FgOptions, bool) {
        if tuning.mode != FgMode::Dynamic || native_supported || !active {
            *self = Self::default();
            // Never submit an unsupported native mode, including while paused.
            let options = if tuning.mode == FgMode::Dynamic && !native_supported {
                FgOptions {
                    mode: FgMode::Fixed,
                    multiplier: 2,
                    ..tuning
                }
            } else {
                tuning
            };
            return (options, false);
        }
        if self.multiplier == 0 {
            self.multiplier = 2;
        }
        self.multiplier = self.multiplier.min(tuning.multiplier);
        if let Some(previous) = self.present_previous {
            let elapsed = now.saturating_sub(previous).as_secs_f64();
            if (0.001..=0.25).contains(&elapsed) {
                self.present_interval = Some(
                    self.present_interval
                        .map_or(elapsed, |old| old * 0.9 + elapsed * 0.1),
                );
            } else {
                self.present_interval = None;
            }
        }
        self.present_previous = Some(now);
        if new_color {
            if let Some(previous) = self.previous {
                let elapsed = now.saturating_sub(previous).as_secs_f64();
                if (0.001..=0.25).contains(&elapsed) {
                    let interval = self
                        .interval
                        .map_or(elapsed, |old| old * 0.9 + elapsed * 0.1);
                    self.interval = Some(interval);
                    // Small timing jitter around an exact multiple must not
                    // alternate 4x/5x. Hold a selection for at least one second.
                    // Repeated app Presents still display one original each.
                    // Only new color pairs can add interpolated frames. Solve
                    // target = app Presents/sec + generated * new colors/sec.
                    let app_fps = 1.0 / self.present_interval.unwrap_or(interval);
                    let generated = (((f64::from(target) - app_fps).max(0.0) * interval - 0.1)
                        .ceil()
                        .max(0.0) as u8)
                        .min(tuning.multiplier - 1);
                    let desired = generated + 1;
                    if desired != self.multiplier
                        && self
                            .changed
                            .is_none_or(|last| now.saturating_sub(last) >= Duration::from_secs(1))
                    {
                        self.multiplier = desired;
                        self.changed = Some(now);
                    }
                } else {
                    self.interval = None;
                }
            }
            self.previous = Some(now);
        }
        (
            FgOptions {
                mode: FgMode::Fixed,
                multiplier: self.multiplier.max(2),
                ..tuning
            },
            self.multiplier == 1,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tuning() -> FgOptions {
        FgOptions {
            mode: FgMode::Dynamic,
            multiplier: 6,
            target_fps: 120,
            ..Default::default()
        }
    }
    #[test]
    fn thirty_fps_selects_four_and_accounts_for_repeated_presents() {
        for repeats in [false, true] {
            let mut controller = Dynamic::default();
            for frame in 0..240 {
                let now = Duration::from_secs_f64(frame as f64 / if repeats { 60.0 } else { 30.0 });
                let (applied, suspended) = controller.configure(
                    tuning(),
                    false,
                    120,
                    now,
                    true,
                    !repeats || frame % 2 == 0,
                );
                assert!(!suspended);
                if frame > 2 {
                    assert_eq!(applied.multiplier, if repeats { 3 } else { 4 });
                    let output = if repeats {
                        60 + (u32::from(applied.multiplier) - 1) * 30
                    } else {
                        u32::from(applied.multiplier) * 30
                    };
                    assert_eq!(output, 120);
                }
                assert_eq!(applied.mode, FgMode::Fixed);
            }
        }
    }
    #[test]
    fn cap_pause_recovery_and_native_passthrough() {
        let mut controller = Dynamic::default();
        let capped = FgOptions {
            multiplier: 3,
            ..tuning()
        };
        controller.configure(capped, false, 240, Duration::ZERO, true, true);
        let (out, _) =
            controller.configure(capped, false, 240, Duration::from_millis(33), true, true);
        assert_eq!(out.multiplier, 3);
        let (paused, _) =
            controller.configure(capped, false, 240, Duration::from_secs(4), false, false);
        assert_eq!((paused.mode, paused.multiplier), (FgMode::Fixed, 2));
        assert_eq!(
            controller.configure(tuning(), true, 120, Duration::ZERO, true, true),
            (tuning(), false)
        );
    }
    #[test]
    fn adapts_to_sustained_rate_change_without_chasing_jitter() {
        let mut controller = Dynamic::default();
        let mut time = Duration::ZERO;
        for frame in 0..180 {
            time += Duration::from_secs_f64(if frame % 2 == 0 { 0.033 } else { 0.034 });
            let (out, _) = controller.configure(tuning(), false, 120, time, true, true);
            if frame > 0 {
                assert_eq!(out.multiplier, 4);
            }
        }
        let mut last = tuning();
        for _ in 0..240 {
            time += Duration::from_secs_f64(1.0 / 60.0);
            (last, _) = controller.configure(tuning(), false, 120, time, true, true);
        }
        assert_eq!(last.multiplier, 2);
    }
    #[test]
    fn target_already_met_suspends_interpolation_and_can_resume() {
        let mut controller = Dynamic::default();
        controller.configure(tuning(), false, 60, Duration::ZERO, true, true);
        let (_, suspended) =
            controller.configure(tuning(), false, 60, Duration::from_millis(16), true, true);
        assert!(suspended);
        let mut time = Duration::from_millis(16);
        let mut suspended = true;
        for _ in 0..120 {
            time += Duration::from_millis(33);
            (_, suspended) = controller.configure(tuning(), false, 60, time, true, true);
        }
        assert!(!suspended);
    }
}
