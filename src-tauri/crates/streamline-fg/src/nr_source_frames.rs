//! NR color observations are distinct from Present and generated-frame IDs.
//! An identical input reuses the completed output. Control changes on a repeat
//! remain pending until a new input, so model history advances only once.
use crate::advanced_settings::NrOptions;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Controls {
    pub intensity: f32,
    pub options: NrOptions,
    pub revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub id: u64,
    pub evaluate: bool,
    pub reset: bool,
    pub controls_pending: bool,
    pub reset_consumers: bool,
}
#[derive(Default)]
pub struct SourceFrames {
    id: u64,
    applied: Option<Controls>,
    pending_reset: bool,
}
impl SourceFrames {
    pub fn applied(&self) -> Option<Controls> {
        self.applied
    }
    /// `boundary` invalidates the previous source epoch (mapping changes,
    /// pause/resume or lost guidance), even if its first color is identical.
    /// Comparison must cover the actual NR input, including alpha.
    pub fn plan(
        &mut self,
        identical: bool,
        boundary: bool,
        reset: bool,
        requested: Controls,
    ) -> Result<Frame, &'static str> {
        if !requested.intensity.is_finite() || !(0.0..=2.0).contains(&requested.intensity) {
            return Err("invalid NR source-frame intensity");
        }
        let changed = self.applied.is_some_and(|old| {
            old.intensity != requested.intensity || old.options != requested.options
        });
        self.pending_reset |= reset || boundary;
        let graph_changed = self.applied.is_some_and(|old| {
            old.options.second_pass.enabled != requested.options.second_pass.enabled
                || old.options.second_pass.retry != requested.options.second_pass.retry
        });
        self.pending_reset |= graph_changed;
        let evaluate = self.applied.is_none() || boundary || graph_changed || !identical;
        Ok(Frame {
            id: if evaluate {
                self.id
                    .checked_add(1)
                    .ok_or("NR source-frame ID exhausted")?
            } else {
                self.id
            },
            evaluate,
            reset: evaluate && (self.applied.is_none() || self.pending_reset),
            controls_pending: !evaluate && changed,
            reset_consumers: evaluate && (changed || self.applied.is_none() || self.pending_reset),
        })
    }
    /// Commit only after successful submission. A skipped or failed evaluate
    /// cannot publish a new ID or claim that requested controls were applied.
    pub fn submitted(&mut self, frame: Frame, requested: Controls) {
        if frame.evaluate {
            self.id = frame.id;
            self.applied = Some(requested);
            self.pending_reset = false;
        } else if !frame.controls_pending {
            // A revision may change without changing NR values.
            if let Some(applied) = self.applied.as_mut() {
                applied.revision = requested.revision;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pass_count_and_explicit_retry_start_new_epochs_even_on_a_repeat() {
        let mut frames = SourceFrames::default();
        let mut c = controls();
        let first = frames.plan(false, false, false, c).unwrap();
        frames.submitted(first, c);
        c.options.second_pass.enabled = true;
        let two = frames.plan(true, false, false, c).unwrap();
        assert!(two.evaluate && two.reset && two.reset_consumers);
        assert_eq!(two.id, 2);
        frames.submitted(two, c);
        c.options.second_pass.inherit = false;
        let tuning = frames.plan(true, false, false, c).unwrap();
        assert!(!tuning.evaluate && tuning.controls_pending);
        c.options.second_pass.retry += 1;
        let retry = frames.plan(true, false, false, c).unwrap();
        assert!(retry.evaluate && retry.reset);
        assert_eq!(retry.id, 3);
        frames.submitted(retry, c);
        c.options.second_pass.enabled = false;
        let one = frames.plan(true, false, false, c).unwrap();
        assert!(one.evaluate && one.reset);
        assert_eq!(one.id, 4);
    }
    fn controls() -> Controls {
        Controls {
            intensity: 1.0,
            options: NrOptions::default(),
            revision: 1,
        }
    }
    #[test]
    fn repeats_reuse_output_without_advancing_history() {
        let mut frames = SourceFrames::default();
        let c = controls();
        let first = frames.plan(true, false, false, c).unwrap();
        assert_eq!(
            first,
            Frame {
                id: 1,
                evaluate: true,
                reset: true,
                controls_pending: false,
                reset_consumers: true
            }
        );
        frames.submitted(first, c);
        for _ in 0..100 {
            let repeated = frames.plan(true, false, false, c).unwrap();
            assert_eq!(repeated.id, 1);
            assert!(!repeated.evaluate && !repeated.reset && !repeated.controls_pending);
            frames.submitted(repeated, c);
        }
        let next = frames.plan(false, false, false, c).unwrap();
        assert_eq!(next.id, 2);
        assert!(next.evaluate && !next.reset);
    }
    #[test]
    fn controls_wait_for_new_color_and_preserve_a_pending_model_reset() {
        let mut frames = SourceFrames::default();
        let c = controls();
        let first = frames.plan(false, false, true, c).unwrap();
        frames.submitted(first, c);
        let mut changed = c;
        changed.intensity = 0.5;
        changed.revision = 2;
        let repeated = frames.plan(true, false, true, changed).unwrap();
        assert!(repeated.controls_pending && !repeated.evaluate && !repeated.reset);
        frames.submitted(repeated, changed);
        assert_eq!(frames.applied(), Some(c));
        let next = frames.plan(false, false, false, changed).unwrap();
        assert_eq!(next.id, 2);
        assert!(next.evaluate && next.reset);
        frames.submitted(next, changed);
        assert_eq!(frames.applied(), Some(changed));
    }
    #[test]
    fn look_revision_waits_without_resetting_the_model() {
        let mut frames = SourceFrames::default();
        let c = controls();
        let first = frames.plan(false, false, false, c).unwrap();
        frames.submitted(first, c);
        let mut changed = c;
        changed.options.look.brighten = 50;
        changed.revision = 3;
        let repeat = frames.plan(true, false, false, changed).unwrap();
        frames.submitted(repeat, changed);
        assert!(repeat.controls_pending && !repeat.evaluate);
        let next = frames.plan(false, false, false, changed).unwrap();
        assert!(next.evaluate && !next.reset && next.reset_consumers);
        frames.submitted(next, changed);
        changed.revision = 4;
        let revision = frames.plan(true, false, false, changed).unwrap();
        frames.submitted(revision, changed);
        assert!(!revision.controls_pending);
        assert_eq!(frames.applied().unwrap().revision, 4);
    }
    #[test]
    fn boundary_invalidates_cached_output_and_failed_record_does_not_commit() {
        let mut frames = SourceFrames::default();
        let c = controls();
        let first = frames.plan(false, false, false, c).unwrap();
        assert!(frames.applied().is_none());
        assert_eq!(frames.plan(false, false, false, c).unwrap().id, first.id);
        frames.submitted(first, c);
        let resumed = frames.plan(true, true, false, c).unwrap();
        assert!(resumed.evaluate && resumed.reset);
        assert_eq!(resumed.id, 2);
        frames.submitted(resumed, c);
        assert!(!frames.plan(true, false, false, c).unwrap().evaluate);
        assert!(frames
            .plan(
                false,
                false,
                false,
                Controls {
                    intensity: f32::NAN,
                    ..c
                }
            )
            .is_err());
        assert_eq!(frames.plan(true, false, false, c).unwrap().id, 2);
    }
}
