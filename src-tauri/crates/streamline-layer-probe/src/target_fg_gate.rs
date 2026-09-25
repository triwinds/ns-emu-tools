//! Eligibility for the bounded diagnostic. A completed stop is terminal for this chain.
#[derive(Clone, Copy)]
pub(super) struct Inputs {
    pub status: u32,
    pub minimum: u32,
    pub maximum: u32,
    pub width: u32,
    pub height: u32,
    pub warmed_up: bool,
    pub foreground: bool,
    pub window_stop: bool,
    pub on_frames: u32,
    pub frame_budget: Option<u32>,
    pub stopped: Option<&'static str>,
}
impl Inputs {
    pub(super) fn off_reason(self) -> Option<&'static str> {
        if let Some(reason) = self.stopped {
            return Some(reason);
        }
        if self.window_stop {
            return Some("window_operation");
        }
        if self.status != 0 {
            return Some("sdk_status");
        }
        if self.minimum == 0 {
            return Some("minimum_unavailable");
        }
        if self.maximum == 0 {
            return Some("multiplier_unsupported");
        }
        if self.width < self.minimum || self.height < self.minimum {
            return Some("below_minimum_extent");
        }
        if self
            .frame_budget
            .is_some_and(|limit| self.on_frames >= limit)
        {
            return Some("frame_budget");
        }
        if !self.foreground {
            return Some("background");
        }
        if !self.warmed_up {
            return Some("warmup");
        }
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn ready() -> Inputs {
        Inputs {
            status: 0,
            minimum: 128,
            maximum: 1,
            width: 128,
            height: 128,
            warmed_up: true,
            foreground: true,
            window_stop: false,
            on_frames: 0,
            frame_budget: None,
            stopped: None,
        }
    }
    #[test]
    fn both_dimensions_must_meet_current_sdk_minimum() {
        assert_eq!(ready().off_reason(), None);
        for (width, height) in [(0, 128), (127, 128), (128, 127), (128, 0)] {
            assert_eq!(
                Inputs {
                    width,
                    height,
                    ..ready()
                }
                .off_reason(),
                Some("below_minimum_extent")
            );
        }
        assert_eq!(
            Inputs {
                minimum: 129,
                ..ready()
            }
            .off_reason(),
            Some("below_minimum_extent")
        );
    }
    #[test]
    fn unavailable_or_error_state_never_enables() {
        for status in [1, 2, u32::MAX] {
            assert_eq!(
                Inputs { status, ..ready() }.off_reason(),
                Some("sdk_status")
            );
        }
        assert_eq!(
            Inputs {
                minimum: 0,
                ..ready()
            }
            .off_reason(),
            Some("minimum_unavailable")
        );
        assert_eq!(
            Inputs {
                maximum: 0,
                ..ready()
            }
            .off_reason(),
            Some("multiplier_unsupported")
        );
    }
    #[test]
    fn warmup_foreground_and_budget_are_required() {
        assert_eq!(
            Inputs {
                warmed_up: false,
                ..ready()
            }
            .off_reason(),
            Some("warmup")
        );
        assert_eq!(
            Inputs {
                foreground: false,
                ..ready()
            }
            .off_reason(),
            Some("background")
        );
        assert_eq!(
            Inputs {
                on_frames: 599,
                ..ready()
            }
            .off_reason(),
            None
        );
        assert_eq!(
            Inputs {
                on_frames: 600,
                frame_budget: Some(600),
                ..ready()
            }
            .off_reason(),
            Some("frame_budget")
        );
    }
    #[test]
    fn ordinary_gameplay_has_no_frame_budget() {
        assert_eq!(
            Inputs {
                on_frames: u32::MAX,
                ..ready()
            }
            .off_reason(),
            None
        );
    }
    #[test]
    fn stop_is_terminal_even_after_focus_and_size_recover() {
        assert_eq!(
            Inputs {
                window_stop: true,
                ..ready()
            }
            .off_reason(),
            Some("window_operation")
        );
        for reason in [
            "window_operation",
            "background",
            "frame_budget",
            "below_minimum_extent",
        ] {
            assert_eq!(
                Inputs {
                    stopped: Some(reason),
                    ..ready()
                }
                .off_reason(),
                Some(reason)
            );
        }
    }
}

// One bounded A/B/A experiment; normal runs always leave the driver limiter disabled.
pub(super) fn frame_limit_us(ab: bool, completed_on_frames: u32) -> u32 {
    if ab && (200..400).contains(&completed_on_frames) {
        16667
    } else {
        0
    }
}
#[cfg(test)]
mod pacing_tests {
    use super::*;
    #[test]
    fn only_middle_segment_enables_the_experimental_limiter() {
        for (frame, expected) in [
            (0, 0),
            (199, 0),
            (200, 16667),
            (399, 16667),
            (400, 0),
            (600, 0),
        ] {
            assert_eq!(frame_limit_us(true, frame), expected);
            assert_eq!(frame_limit_us(false, frame), 0);
        }
    }
}

// Focus changes pause ordinary sessions; explicit diagnostics retain terminal stops.
pub(super) fn terminal_stop(reason: Option<&'static str>, bounded: bool) -> Option<&'static str> {
    if matches!(reason, Some("background" | "user_disabled")) && !bounded {
        None
    } else {
        reason
    }
}
#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    #[test]
    fn focus_can_resume_but_window_and_sdk_failures_remain_terminal() {
        assert_eq!(terminal_stop(Some("background"), false), None);
        assert_eq!(terminal_stop(Some("user_disabled"), false), None);
        assert_eq!(terminal_stop(Some("background"), true), Some("background"));
        for reason in ["window_operation", "sdk_status", "frame_budget"] {
            assert_eq!(terminal_stop(Some(reason), false), Some(reason));
        }
    }
}
