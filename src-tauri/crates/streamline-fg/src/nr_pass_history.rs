//! Independent history of the second model, acknowledged only on submission.
use crate::advanced_settings::{InterpassOptions, NrOptions};
#[derive(Default)]
pub struct History {
    previous: Option<(u64, f32, NrOptions, InterpassOptions)>,
}
impl History {
    pub fn reset(
        &self,
        frame: u64,
        intensity: f32,
        options: NrOptions,
        upstream_reset: bool,
    ) -> bool {
        self.reset_with_input(
            frame,
            intensity,
            options,
            upstream_reset,
            InterpassOptions::default(),
        )
    }
    pub fn reset_with_input(
        &self,
        frame: u64,
        intensity: f32,
        options: NrOptions,
        upstream_reset: bool,
        input: InterpassOptions,
    ) -> bool {
        upstream_reset
            || self
                .previous
                .is_none_or(|(old_frame, old_intensity, old_options, old_input)| {
                    old_frame.checked_add(1) != Some(frame)
                        || old_intensity != intensity
                        || old_options != options.model_only()
                        || old_input != input
                })
    }
    pub fn submitted(&mut self, frame: u64, intensity: f32, options: NrOptions) {
        self.submitted_with_input(frame, intensity, options, InterpassOptions::default());
    }
    pub fn submitted_with_input(
        &mut self,
        frame: u64,
        intensity: f32,
        options: NrOptions,
        input: InterpassOptions,
    ) {
        self.previous = Some((frame, intensity, options.model_only(), input));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prepared_input_changes_reset_only_the_second_model() {
        let mut h = History::default();
        let options = NrOptions::default();
        let input = InterpassOptions {
            clamp: true,
            feedback: 75,
        };
        h.submitted_with_input(7, 1.0, options, input);
        assert!(!h.reset_with_input(8, 1.0, options, false, input));
        assert!(h.reset_with_input(
            8,
            1.0,
            options,
            false,
            InterpassOptions {
                feedback: 50,
                ..input
            }
        ));
        assert!(h.reset_with_input(
            8,
            1.0,
            options,
            false,
            InterpassOptions {
                clamp: false,
                ..input
            }
        ));
        // The acknowledgement follows submission, not a failed/discarded recording.
        assert!(!h.reset_with_input(8, 1.0, options, false, input));
    }
    #[test]
    fn suffix_recompute_resets_same_observation_and_next_color_remains_sequential() {
        let mut h = History::default();
        let options = NrOptions::default();
        h.submitted(7, 1.0, options);
        assert!(h.reset(7, 0.5, options, false));
        h.submitted(7, 0.5, options);
        assert!(h.reset(7, 0.5, options, false));
        assert!(!h.reset(8, 0.5, options, false));
    }
    #[test]
    fn independent_parameters_and_gaps_reset_but_look_does_not() {
        let mut h = History::default();
        let mut options = NrOptions::default();
        assert!(h.reset(1, 1.0, options, false));
        h.submitted(1, 1.0, options);
        assert!(!h.reset(2, 1.0, options, false));
        options.look.brighten = 50;
        assert!(!h.reset(2, 1.0, options, false));
        options.style = crate::advanced_settings::NrStyle::B;
        assert!(h.reset(2, 1.0, options, false));
        h.submitted(2, 1.0, options);
        assert!(!h.reset(3, 1.0, options, false));
        assert!(h.reset(3, 0.5, options, false));
        assert!(h.reset(3, 1.0, options, true));
        assert!(h.reset(4, 1.0, options, false));
        // A failed/non-submitted frame cannot consume history.
        assert!(!h.reset(3, 1.0, options, false));
    }
}
