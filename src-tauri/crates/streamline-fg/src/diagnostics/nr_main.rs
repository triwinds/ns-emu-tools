#[cfg(all(windows, feature = "nr-coexistence"))]
#[path = "../fg_api.rs"]
// Reuse its resource helpers; this binary deliberately does not activate FG.
#[allow(dead_code)]
mod fg_api;
#[cfg(all(windows, target_arch = "x86_64"))]
#[path = "../nr_abi.rs"]
mod nr_abi;
#[cfg(all(windows, target_arch = "x86_64"))]
#[path = "../nr_api.rs"]
mod nr_api;
#[cfg(all(windows, target_arch = "x86_64"))]
#[path = "../nr_history.rs"]
pub mod nr_history;
#[cfg(all(windows, target_arch = "x86_64"))]
#[path = "../nr_layout.rs"]
mod nr_layout;
#[cfg(all(windows, target_arch = "x86_64"))]
mod sdk_nr;
#[cfg(all(windows, feature = "nr-coexistence"))]
mod sdk_nr_coexist;
#[cfg(all(windows, target_arch = "x86_64"))]
mod sdk_nr_gpu;
#[cfg(all(windows, feature = "nr-coexistence"))]
mod host {
    pub(super) use crate::nr_api::Result;
    pub(super) use crate::sdk_nr::{hash, write_json};
}
#[cfg(all(windows, feature = "nr-coexistence"))]
mod sdk_sr;

fn main() {
    #[cfg(all(windows, target_arch = "x86_64"))]
    if let Err(error) = sdk_nr::run() {
        eprintln!("NR diagnostic failed: {error}");
        std::process::exit(1);
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        eprintln!("NR diagnostic requires Windows x64.");
        std::process::exit(1);
    }
}
