//! Frame-boundary selection; swapchain identity stays owned by Streamline.
use crate::advanced_settings::FgBackend;
use std::sync::atomic::{AtomicBool, Ordering};
static XESS: AtomicBool = AtomicBool::new(false);
pub(super) fn xess() -> bool {
    XESS.load(Ordering::Acquire)
}
pub(super) fn select(backend: FgBackend) -> bool {
    let next = backend == FgBackend::Xess;
    XESS.swap(next, Ordering::AcqRel) != next
}
