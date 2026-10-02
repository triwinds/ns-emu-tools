//! The first native experiment accepts only the previously GPU-tested binaries.
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};
#[path = "../../streamline-nr-contract.rs"]
mod contract;
pub(crate) use contract::{BRIDGE, RUNTIMES};
pub(crate) fn verify(path: &Path, accepted: &[&str]) -> Result<String, String> {
    if !path.is_absolute() {
        return Err("NR component path must be absolute".into());
    }
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut bytes = [0; 65536];
    loop {
        let count = file.read(&mut bytes).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&bytes[..count]);
    }
    let digest = format!("{:x}", hash.finalize());
    if !accepted.contains(&digest.as_str()) {
        return Err(format!(
            "unverified native NR component: {}",
            path.display()
        ));
    }
    Ok(digest)
}
