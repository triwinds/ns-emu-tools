//! Frame-boundary NR policy. Missing motion pauses NR and preserves pending
//! resets until each downstream consumer actually consumes the next output.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Controls {
    pub enabled: bool,
    pub intensity: f32,
    pub options: crate::advanced_settings::NrOptions,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Source {
    /// Logical stream/allocation epoch supplied by the validated source owner.
    pub identity: u64,
    pub extent: [u32; 2],
    /// Caller identity must include crop, flip, encoding and input provenance.
    pub mapping: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Disabled,
    MotionUnavailable,
    EvaluationFailed,
    Created,
    Resumed,
    SourceChanged,
    StrengthChanged,
    LookChanged,
    PipelineChanged,
    SecondPassChanged,
    Discontinuity,
    Continuous,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decision {
    pub evaluate: bool,
    pub reset_nr: bool,
    pub reason: Reason,
    pub reset_sr: bool,
    pub reset_fg: bool,
}
#[derive(Default)]
pub struct History {
    controls: Option<Controls>,
    source: Option<Source>,
    frame: Option<u64>,
    running: bool,
    pending_nr: bool,
    pending_sr: bool,
    pending_fg: bool,
}
impl History {
    pub fn recreated(&mut self) {
        self.pending_nr = true;
        self.pending_sr = true;
        self.pending_fg = true;
        self.running = false;
    }
    /// Call only when failure is confirmed safe before GPU submission and this
    /// frame will use the original color. Unknown submitted work is not reusable.
    /// Downstream consumers must reset on this fallback frame and again when NR
    /// resumes; do not wait until the next frame to signal the fallback change.
    pub fn safe_fallback(&mut self) -> Decision {
        self.recreated();
        Decision {
            evaluate: false,
            reset_nr: false,
            reason: Reason::EvaluationFailed,
            reset_sr: true,
            reset_fg: true,
        }
    }
    pub fn next(
        &mut self,
        controls: Controls,
        source: Source,
        frame: u64,
        motion_valid: bool,
    ) -> Result<Decision, &'static str> {
        if !controls.intensity.is_finite() || !(0.0..=2.0).contains(&controls.intensity) {
            return Err("NR intensity must be finite and within 0..2");
        }
        if source.extent.contains(&0) || source.identity == 0 {
            return Err("NR source identity/extent is invalid");
        }
        let active = controls.enabled && motion_valid;
        let first = self.controls.is_none();
        let source_changed = self.source.is_some_and(|old| old != source);
        let strength_changed = self.controls.is_some_and(|old| {
            old.intensity != controls.intensity
                || old.options.model_only() != controls.options.model_only()
        });
        let look_changed = self
            .controls
            .is_some_and(|old| old.options.look != controls.options.look);
        let second_changed = self.controls.is_some_and(|old| {
            !old.options.second_pass.execution_eq(
                controls.options.second_pass,
                controls.intensity,
                controls.options,
            )
        });
        let graph_changed = self.controls.is_some_and(|old| {
            old.options.second_pass.enabled != controls.options.second_pass.enabled
                || (old.options.second_pass.enabled
                    && old.options.second_pass.retry != controls.options.second_pass.retry)
        });
        let toggled = self
            .controls
            .is_some_and(|old| old.enabled != controls.enabled);
        let discontinuity = self
            .frame
            .is_some_and(|old| old.checked_add(1) != Some(frame));
        let activity_changed = self.running != active;
        if first
            || source_changed
            || strength_changed
            || toggled
            || discontinuity
            || activity_changed
            || graph_changed
        {
            self.pending_nr = true;
            self.pending_sr = true;
            self.pending_fg = true;
        }
        if !active {
            self.pending_nr = true;
        }
        if look_changed || second_changed {
            self.pending_sr = true;
            self.pending_fg = true;
        }
        let reason = if !controls.enabled {
            Reason::Disabled
        } else if !motion_valid {
            Reason::MotionUnavailable
        } else if first {
            Reason::Created
        } else if source_changed {
            Reason::SourceChanged
        } else if strength_changed {
            Reason::StrengthChanged
        } else if graph_changed {
            Reason::PipelineChanged
        } else if discontinuity {
            Reason::Discontinuity
        } else if !self.running || self.pending_nr {
            Reason::Resumed
        } else if look_changed {
            Reason::LookChanged
        } else if second_changed {
            Reason::SecondPassChanged
        } else {
            Reason::Continuous
        };
        let decision = Decision {
            evaluate: active,
            reset_nr: active && self.pending_nr,
            reason,
            reset_sr: self.pending_sr,
            reset_fg: self.pending_fg,
        };
        if active {
            self.pending_nr = false;
        }
        self.controls = Some(controls);
        self.source = Some(source);
        self.frame = Some(frame);
        self.running = active;
        Ok(decision)
    }
    /// Acknowledge only after the named consumer actually accepts this frame.
    pub fn sr_consumed(&mut self) {
        self.pending_sr = false;
    }
    pub fn fg_consumed(&mut self) {
        self.pending_fg = false;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inactive_second_tuning_preserves_all_active_histories() {
        let mut h = History::default();
        let mut c = controls();
        h.next(c, source(), 0, true).unwrap();
        h.sr_consumed();
        h.fg_consumed();
        c.options.second_pass.intensity = 50;
        c.options.second_pass.retry = 7;
        let d = h.next(c, source(), 1, true).unwrap();
        assert!(!d.reset_nr && !d.reset_sr && !d.reset_fg);
        assert_eq!(d.reason, Reason::Continuous);
    }
    #[test]
    fn second_tuning_preserves_first_history_but_graph_changes_reset_both() {
        let mut h = History::default();
        let mut c = controls();
        h.next(c, source(), 0, true).unwrap();
        h.sr_consumed();
        h.fg_consumed();
        c.options.second_pass.enabled = true;
        let graph = h.next(c, source(), 1, true).unwrap();
        assert_eq!(graph.reason, Reason::PipelineChanged);
        assert!(graph.reset_nr);
        h.sr_consumed();
        h.fg_consumed();
        c.options.second_pass.inherit = false;
        c.options.second_pass.intensity = 50;
        let independent = h.next(c, source(), 2, true).unwrap();
        assert_eq!(independent.reason, Reason::SecondPassChanged);
        assert!(!independent.reset_nr && independent.reset_sr && independent.reset_fg);
        c.options.second_pass.retry += 1;
        assert!(h.next(c, source(), 3, true).unwrap().reset_nr);
    }
    #[test]
    fn look_changes_keep_model_history_and_reset_only_downstream_consumers() {
        let mut h = History::default();
        let mut c = controls();
        h.next(c, source(), 0, true).unwrap();
        h.sr_consumed();
        h.fg_consumed();
        c.options.look.brighten = 50;
        let changed = h.next(c, source(), 1, true).unwrap();
        assert!(changed.evaluate && !changed.reset_nr && changed.reset_sr && changed.reset_fg);
        assert_eq!(changed.reason, Reason::LookChanged);
        h.sr_consumed();
        let next = h.next(c, source(), 2, true).unwrap();
        assert!(!next.reset_nr && !next.reset_sr && next.reset_fg);
        h.fg_consumed();
        c.options.look.enabled = false;
        let bypass = h.next(c, source(), 3, true).unwrap();
        assert!(!bypass.reset_nr && bypass.reset_sr && bypass.reset_fg);
        c.options.look.enabled = true;
        let paused = h.next(c, source(), 4, false).unwrap();
        assert!(!paused.evaluate && paused.reset_sr && paused.reset_fg);
        assert!(h.next(c, source(), 5, true).unwrap().reset_nr);
        h.sr_consumed();
        h.fg_consumed();
        c.options.look.spatial.enabled = true;
        c.options.look.spatial.halo = 50;
        let spatial = h.next(c, source(), 6, true).unwrap();
        assert_eq!(spatial.reason, Reason::LookChanged);
        assert!(!spatial.reset_nr && spatial.reset_sr && spatial.reset_fg);
    }
    fn source() -> Source {
        Source {
            identity: 7,
            extent: [640, 360],
            mapping: 1,
        }
    }
    fn controls() -> Controls {
        Controls {
            enabled: true,
            intensity: 1.0,
            options: Default::default(),
        }
    }
    #[test]
    fn model_and_independent_strength_changes_reset_every_consumer() {
        let mut history = History::default();
        let mut control = controls();
        history.next(control, source(), 0, true).unwrap();
        history.sr_consumed();
        history.fg_consumed();
        assert!(!history.next(control, source(), 1, true).unwrap().reset_nr);
        control.options.style = crate::advanced_settings::NrStyle::C;
        let changed = history.next(control, source(), 2, true).unwrap();
        assert_eq!(changed.reason, Reason::StrengthChanged);
        assert!(changed.reset_nr && changed.reset_sr && changed.reset_fg);
        history.sr_consumed();
        history.fg_consumed();
        control.options.local_tone = Some(25);
        let paused = history.next(control, source(), 3, false).unwrap();
        assert!(!paused.evaluate && paused.reset_sr && paused.reset_fg);
        let resumed = history.next(control, source(), 4, true).unwrap();
        assert!(resumed.reset_nr && resumed.reset_sr && resumed.reset_fg);
    }
    #[test]
    fn missing_motion_pauses_and_recovery_resets_without_stale_motion() {
        let mut h = History::default();
        assert!(h.next(controls(), source(), 0, true).unwrap().reset_nr);
        h.sr_consumed();
        h.fg_consumed();
        assert!(!h.next(controls(), source(), 1, true).unwrap().reset_nr);
        let paused = h.next(controls(), source(), 2, false).unwrap();
        assert!(!paused.evaluate);
        assert!(!paused.reset_nr);
        assert!(paused.reset_sr);
        h.sr_consumed();
        let still_paused = h.next(controls(), source(), 3, false).unwrap();
        assert!(!still_paused.reset_sr);
        assert!(still_paused.reset_fg);
        let resumed = h.next(controls(), source(), 4, true).unwrap();
        assert!(resumed.evaluate && resumed.reset_nr && resumed.reset_sr && resumed.reset_fg);
        assert_eq!(resumed.reason, Reason::Resumed);
    }
    #[test]
    fn mapping_strength_and_gaps_reset_paused_consumers_independently() {
        let mut h = History::default();
        h.next(controls(), source(), 0, true).unwrap();
        h.sr_consumed();
        h.fg_consumed();
        let mut s = source();
        s.mapping = 2;
        assert_eq!(
            h.next(controls(), s, 1, true).unwrap().reason,
            Reason::SourceChanged
        );
        h.sr_consumed();
        let next = h.next(controls(), s, 2, true).unwrap();
        assert!(!next.reset_sr && next.reset_fg);
        assert_eq!(
            h.next(
                Controls {
                    intensity: 0.5,
                    ..controls()
                },
                s,
                3,
                true
            )
            .unwrap()
            .reason,
            Reason::StrengthChanged
        );
        assert_eq!(
            h.next(
                Controls {
                    intensity: 0.5,
                    ..controls()
                },
                s,
                5,
                true
            )
            .unwrap()
            .reason,
            Reason::Discontinuity
        );
    }
    #[test]
    fn recreation_and_disable_reenable_reset_both_output_consumers() {
        let mut h = History::default();
        h.next(controls(), source(), 0, true).unwrap();
        h.sr_consumed();
        h.fg_consumed();
        h.recreated();
        let d = h.next(controls(), source(), 1, true).unwrap();
        assert!(d.reset_nr && d.reset_sr && d.reset_fg);
        let off = h
            .next(
                Controls {
                    enabled: false,
                    ..controls()
                },
                source(),
                2,
                true,
            )
            .unwrap();
        assert!(!off.evaluate && off.reset_sr && off.reset_fg);
        assert!(h.next(controls(), source(), 3, true).unwrap().reset_nr);
    }
    #[test]
    fn invalid_controls_do_not_advance_history() {
        let mut h = History::default();
        h.next(controls(), source(), 0, true).unwrap();
        h.sr_consumed();
        h.fg_consumed();
        assert!(h
            .next(
                Controls {
                    intensity: f32::NAN,
                    ..controls()
                },
                source(),
                1,
                true
            )
            .is_err());
        assert_eq!(
            h.next(controls(), source(), 1, true).unwrap().reason,
            Reason::Continuous
        );
    }
    #[test]
    fn safe_failure_resets_fallback_and_resume_with_a_paused_consumer() {
        let mut h = History::default();
        h.next(controls(), source(), 0, true).unwrap();
        h.sr_consumed();
        h.fg_consumed();
        assert!(!h.next(controls(), source(), 1, true).unwrap().reset_nr);
        let fallback = h.safe_fallback();
        assert!(!fallback.evaluate && fallback.reset_sr && fallback.reset_fg);
        assert_eq!(fallback.reason, Reason::EvaluationFailed);
        // SR consumes the fallback now; FG is paused and must retain its reset.
        h.sr_consumed();
        let resumed = h.next(controls(), source(), 2, true).unwrap();
        assert!(resumed.reset_nr && resumed.reset_sr && resumed.reset_fg);
        assert_eq!(resumed.reason, Reason::Resumed);
        h.sr_consumed();
        assert!(h.next(controls(), source(), 3, true).unwrap().reset_fg);
        h.fg_consumed();
        let continuous = h.next(controls(), source(), 4, true).unwrap();
        assert!(!continuous.reset_nr && !continuous.reset_sr && !continuous.reset_fg);
    }
}
