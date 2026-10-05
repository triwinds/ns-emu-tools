use super::{native_nr, streamline_fg::file_digest, streamline_install};
use crate::config::{nr_presets::Environment, CURRENT_VERSION};
use std::path::Path;

/// Missing/damaged installations remain unknown. Existing game sessions retain
/// private copies, so this read-only report deliberately describes the install.
pub fn environment(executable: &Path) -> Environment {
    let nr = native_nr::status();
    let mut report = Environment {
        toolbox_version: CURRENT_VERSION.into(),
        model_version: if nr.installed {
            nr.runtime_version.into()
        } else {
            String::new()
        },
        model_sha256: nr.runtime_sha256,
        ..Environment::default()
    };
    if let Ok(exe) = executable.canonicalize() {
        report.target_sha256 = file_digest(&exe).ok();
        if let Ok(package) = streamline_install::verified_preset_package(&exe) {
            report.component_version = package.version;
            report.component_sha256 = package
                .files
                .iter()
                .find(|f| f.name == "streamline_probe_layer.dll")
                .map(|f| f.sha256.clone());
        }
    }
    report
}
