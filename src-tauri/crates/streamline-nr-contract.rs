//! GPU-tested native NR binaries shared by the toolbox and process launcher.
pub const RUNTIMES: [&str; 2] = [
    "e16bcf15e16e13f527491cdf7845b2fe6521a738d8f7c9c721866a8496e1fc8e",
    "4b8d19bc3eff58a084f5eca7489c921501c203450169fb82ff4f649a4482ba05",
];
// Rebuilt bridge: audited identical .text/.data/.pdata/.reloc; only PE build/debug metadata changed.
pub const BRIDGE: &str = "0f23c0fce4bc0a144fc87f9a5706dbe91d3aa6132533fb0adf9f0f03c858a2dc";
