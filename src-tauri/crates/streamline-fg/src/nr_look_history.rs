//! Source-observation clock for Look only; it never controls NGX reset.
use crate::advanced_settings::LookOptions;
#[derive(Clone, Copy, Debug)]
pub struct Context {
    pub source_frame_id: u64,
    pub now_ms: u64,
    pub reset: bool,
    pub uv_scale: [f32; 2],
}
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub context: Context,
    pub options: LookOptions,
    pub weight: f32,
    pub interval_ms: u64,
    pub reason: &'static str,
}
#[derive(Default)]
pub struct Clock {
    previous: Option<Frame>,
}
impl Clock {
    pub fn plan(&self, options: LookOptions, context: Context) -> Frame {
        let interval_ms = self
            .previous
            .and_then(|old| context.now_ms.checked_sub(old.context.now_ms))
            .unwrap_or(0);
        let reason = if context.reset {
            "upstream_reset"
        } else if let Some(old) = self.previous {
            if old.context.source_frame_id.checked_add(1) != Some(context.source_frame_id) {
                "source_gap"
            } else if old.options != options {
                "look_changed"
            } else if interval_ms == 0 {
                "nonpositive_interval"
            } else if interval_ms > 250 {
                "long_interval"
            } else {
                "continuous"
            }
        } else {
            "first_frame"
        };
        let weight = if reason == "continuous" {
            (-(interval_ms as f32) / f32::from(options.temporal.time_ms))
                .exp()
                .min(f32::from(options.temporal.strength) / 100.0)
        } else {
            0.0
        };
        Frame {
            context,
            options,
            weight,
            interval_ms,
            reason,
        }
    }
    pub fn submitted(&mut self, frame: Frame) {
        self.previous = Some(frame);
    }
    pub fn invalidate(&mut self) {
        self.previous = None;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn elapsed_time_and_rejection_do_not_advance_before_submission() {
        let mut clock = Clock::default();
        let mut options = LookOptions::default();
        options.temporal.enabled = true;
        let c = |id, ms| Context {
            source_frame_id: id,
            now_ms: ms,
            reset: false,
            uv_scale: [1.0; 2],
        };
        let first = clock.plan(options, c(1, 100));
        assert_eq!(first.weight, 0.0);
        clock.submitted(first);
        let fast = clock.plan(options, c(2, 116));
        assert_eq!(fast.weight, 0.75);
        let slow = clock.plan(options, c(2, 200));
        assert!(slow.weight > 0.0 && slow.weight < fast.weight);
        assert_eq!(clock.plan(options, c(2, 400)).reason, "long_interval");
        assert_eq!(clock.plan(options, c(3, 116)).reason, "source_gap");
        assert_eq!(
            clock.plan(options, c(2, 100)).reason,
            "nonpositive_interval"
        );
        options.brighten = 50;
        assert_eq!(clock.plan(options, c(2, 116)).reason, "look_changed");
        clock.invalidate();
        assert_eq!(clock.plan(options, c(2, 116)).weight, 0.0);
    }
}
