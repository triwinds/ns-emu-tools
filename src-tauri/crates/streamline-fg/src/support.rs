//! Shared launch-time file validation, independent of the diagnostic host.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
};
pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub(crate) fn hash(path: &Path) -> Result<String> {
    let mut source = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = source.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
#[cfg(feature = "sdk-bridge")]
pub(crate) fn verify(path: &Path, name: &str) -> Result<String> {
    let profile: Value = serde_json::from_str(include_str!("../sdk/baseline.json"))?;
    let expected = profile["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["path"] == name)
        .ok_or("missing frozen hash")?["sha256"]
        .as_str()
        .unwrap();
    let actual = hash(path)?;
    if actual != expected {
        return Err(format!("frozen hash mismatch: {}", path.display()).into());
    }
    Ok(actual)
}
pub(crate) fn write_json(path: &Path, value: &Value) -> Result<()> {
    let mut output = OpenOptions::new().create_new(true).write(true).open(path)?;
    serde_json::to_writer_pretty(&mut output, value)?;
    writeln!(output)?;
    Ok(())
}
