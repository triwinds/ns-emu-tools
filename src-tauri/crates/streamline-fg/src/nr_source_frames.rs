//! NR color observations are distinct from Present and generated-frame IDs.
//! Repeats reuse model output; pure Look changes recompose without observing
//! a new color. Model changes rerun the affected suffix on the same source ID.
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
    pub first_evaluate: bool,
    pub observed: bool,
    pub model_recompute: bool,
    pub reset: bool,
    pub controls_pending: bool,
    pub reset_consumers: bool,
    pub look_recompute: bool,
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
        let first_changed = self.applied.is_some_and(|old| {
            old.intensity != requested.intensity
                || old.options.model_only() != requested.options.model_only()
        });
        let second_changed = self.applied.is_some_and(|old| {
            !old.options.second_pass.execution_eq(
                requested.options.second_pass,
                requested.intensity,
                requested.options,
            )
        });
        let look_changed = self
            .applied
            .is_some_and(|old| old.options.look != requested.options.look);
        let changed = first_changed || second_changed || look_changed;
        self.pending_reset |= reset || boundary;
        let graph_changed = self.applied.is_some_and(|old| {
            old.options.second_pass.enabled != requested.options.second_pass.enabled
                || (old.options.second_pass.enabled
                    && old.options.second_pass.retry != requested.options.second_pass.retry)
        });
        self.pending_reset |= graph_changed || first_changed;
        let observed = self.applied.is_none() || !identical;
        let first_evaluate = observed || self.pending_reset;
        let evaluate = first_evaluate || second_changed;
        let model_recompute = evaluate && !observed;
        let look_recompute = !evaluate && look_changed;
        Ok(Frame {
            id: if observed {
                self.id
                    .checked_add(1)
                    .ok_or("NR source-frame ID exhausted")?
            } else {
                self.id
            },
            evaluate,
            first_evaluate,
            observed,
            model_recompute,
            reset: first_evaluate
                && (self.applied.is_none() || self.pending_reset || model_recompute),
            controls_pending: false,
            reset_consumers: look_recompute
                || (evaluate && (changed || self.applied.is_none() || self.pending_reset)),
            look_recompute,
        })
    }
    /// Commit only after successful submission. A skipped or failed evaluate
    /// cannot publish an observation or claim that requested controls were applied.
    pub fn submitted(&mut self, frame: Frame, requested: Controls) {
        if frame.evaluate {
            self.id = frame.id;
            self.applied = Some(requested);
            self.pending_reset = false;
        } else if frame.look_recompute {
            self.applied = Some(requested);
        } else if !frame.controls_pending {
            // A revision may change without changing NR values.
            self.applied = Some(requested);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inactive_second_controls_do_not_invalidate_active_output_or_hide_look_updates() {
        let mut frames = SourceFrames::default();
        let mut c = controls();
        let first = frames.plan(false, false, false, c).unwrap();
        frames.submitted(first, c);
        c.options.second_pass.intensity = 50;
        c.options.second_pass.retry = 12;
        let hidden = frames.plan(true, false, false, c).unwrap();
        assert!(
            !hidden.evaluate
                && !hidden.look_recompute
                && !hidden.controls_pending
                && !hidden.reset_consumers
        );
        frames.submitted(hidden, c);
        assert_eq!(frames.applied(), Some(c));
        c.options.look.amount = 50;
        let look = frames.plan(true, false, false, c).unwrap();
        assert!(look.look_recompute && !look.evaluate && look.id == first.id);
        c.options.second_pass.enabled = true;
        assert!(frames.plan(true, false, false, c).unwrap().evaluate);
    }
    #[test]
    fn graph_changes_reset_models_without_inventing_source_observations() {
        let mut frames = SourceFrames::default();
        let mut c = controls();
        let first = frames.plan(false, false, false, c).unwrap();
        frames.submitted(first, c);
        c.options.second_pass.enabled = true;
        let two = frames.plan(true, false, false, c).unwrap();
        assert!(two.evaluate && two.reset && two.reset_consumers);
        assert_eq!(two.id, first.id);
        assert!(two.first_evaluate && two.model_recompute && !two.observed);
        frames.submitted(two, c);
        c.options.second_pass.inherit = false;
        c.options.second_pass.intensity = 50;
        let tuning = frames.plan(true, false, false, c).unwrap();
        assert!(tuning.evaluate && !tuning.first_evaluate && !tuning.controls_pending);
        assert!(tuning.model_recompute && !tuning.reset && !tuning.observed);
        frames.submitted(tuning, c);
        c.options.second_pass.retry += 1;
        let retry = frames.plan(true, false, false, c).unwrap();
        assert!(retry.evaluate && retry.reset);
        assert_eq!(retry.id, first.id);
        frames.submitted(retry, c);
        c.options.second_pass.enabled = false;
        let one = frames.plan(true, false, false, c).unwrap();
        assert!(one.evaluate && one.reset);
        assert_eq!(one.id, first.id);
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
                first_evaluate: true,
                observed: true,
                model_recompute: false,
                reset: true,
                controls_pending: false,
                reset_consumers: true,
                look_recompute: false
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
    fn first_model_recompute_resets_both_and_commits_only_on_submission() {
        let mut frames = SourceFrames::default();
        let c = controls();
        let first = frames.plan(false, false, true, c).unwrap();
        frames.submitted(first, c);
        let mut changed = c;
        changed.intensity = 0.5;
        changed.revision = 2;
        let repeated = frames.plan(true, false, true, changed).unwrap();
        assert!(repeated.evaluate && repeated.first_evaluate && repeated.reset);
        assert!(repeated.model_recompute && !repeated.observed);
        assert_eq!(repeated.id, first.id);
        assert_eq!(frames.applied(), Some(c));
        // A discarded recording must retain the changed controls and reset.
        let retry = frames.plan(true, false, false, changed).unwrap();
        assert_eq!(retry, repeated);
        frames.submitted(retry, changed);
        assert_eq!(frames.applied(), Some(changed));
        let next = frames.plan(false, false, false, changed).unwrap();
        assert_eq!(next.id, 2);
        assert!(next.evaluate && !next.reset && next.observed && !next.model_recompute);
        frames.submitted(next, changed);
        assert_eq!(frames.applied(), Some(changed));
    }
    #[test]
    fn second_suffix_retries_keep_prefix_and_next_color_evaluates_both() {
        let mut frames = SourceFrames::default();
        let mut c = controls();
        c.options.second_pass.enabled = true;
        c.options.second_pass.inherit = false;
        let first = frames.plan(false, false, false, c).unwrap();
        frames.submitted(first, c);
        c.options.second_pass.intensity = 50;
        c.revision += 1;
        let suffix = frames.plan(true, false, false, c).unwrap();
        assert!(suffix.evaluate && suffix.model_recompute && suffix.reset_consumers);
        assert!(!suffix.first_evaluate && !suffix.reset && !suffix.observed);
        assert_eq!(suffix.id, first.id);
        assert_eq!(frames.plan(true, false, false, c).unwrap(), suffix);
        assert_ne!(frames.applied(), Some(c));
        frames.submitted(suffix, c);
        assert_eq!(frames.applied(), Some(c));
        let repeat = frames.plan(true, false, false, c).unwrap();
        assert!(!repeat.evaluate && !repeat.reset_consumers);
        let next = frames.plan(false, false, false, c).unwrap();
        assert!(next.first_evaluate && next.observed && !next.model_recompute && !next.reset);
        assert_eq!(next.id, first.id + 1);
    }
    #[test]
    fn look_revision_recomposes_without_advancing_or_resetting_the_model() {
        let mut frames = SourceFrames::default();
        let c = controls();
        let first = frames.plan(false, false, false, c).unwrap();
        frames.submitted(first, c);
        let mut changed = c;
        changed.options.look.brighten = 50;
        changed.revision = 3;
        let repeat = frames.plan(true, false, false, changed).unwrap();
        frames.submitted(repeat, changed);
        assert!(
            !repeat.controls_pending
                && !repeat.evaluate
                && repeat.look_recompute
                && repeat.reset_consumers
        );
        assert_eq!(repeat.id, first.id);
        assert_eq!(frames.applied(), Some(changed));
        let next = frames.plan(false, false, false, changed).unwrap();
        assert!(next.evaluate && !next.reset && !next.reset_consumers);
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
        assert_eq!(resumed.id, first.id);
        assert!(resumed.model_recompute && !resumed.observed);
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
        assert_eq!(frames.plan(true, false, false, c).unwrap().id, first.id);
        let new_color = frames.plan(false, false, false, c).unwrap();
        assert_eq!(new_color.id, first.id + 1);
        assert!(new_color.observed && new_color.first_evaluate && !new_color.reset);
    }
}
