//! Identify the layer entry point issuing a validation message, without filtering.
use std::cell::Cell;
thread_local! {
    static CALL: Cell<Option<&'static str>> = const { Cell::new(None) };
}
pub(super) fn current() -> Option<&'static str> {
    CALL.with(Cell::get)
}
pub(super) fn application<T>(name: &'static str, call: impl FnOnce() -> T) -> T {
    struct Reset(Option<&'static str>);
    impl Drop for Reset {
        fn drop(&mut self) {
            CALL.with(|v| v.set(self.0));
        }
    }
    let _reset = Reset(CALL.with(|v| v.replace(Some(name))));
    call()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_context_restores_on_unwind_and_stays_thread_local() {
        application("outer", || {
            let _ = std::panic::catch_unwind(|| application("inner", || panic!("test")));
            assert_eq!(current(), Some("outer"));
            assert_eq!(std::thread::spawn(current).join().unwrap(), None);
        });
        assert_eq!(current(), None);
    }
}
