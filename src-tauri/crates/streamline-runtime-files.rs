//! Windows file sharing protects immutable runtimes across process lifetimes.
use std::{fs::File, io, path::PathBuf};

pub enum Access {
    Runtime,
    Removal,
}

pub fn hold(paths: &[PathBuf], access: Access) -> io::Result<Vec<File>> {
    paths
        .iter()
        .map(|path| {
            let mut options = std::fs::OpenOptions::new();
            options.read(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                // Readers coexist. Runtime handles deny modification/deletion;
                // removal handles deny new readers while permitting our delete.
                options.share_mode(match access {
                    Access::Runtime => 1, // FILE_SHARE_READ
                    Access::Removal => 4, // FILE_SHARE_DELETE
                });
            }
            #[cfg(not(windows))]
            let _ = &access;
            options.open(path)
        })
        .collect()
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn readers_prevent_overwrite_and_removal_until_all_handles_close() {
        let dir = std::env::temp_dir().join(format!("runtime-sharing-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("runtime.dll");
        std::fs::write(&path, b"runtime").unwrap();
        let files = vec![path.clone()];
        let first = hold(&files, Access::Runtime).unwrap();
        let second = hold(&files, Access::Runtime).unwrap();
        assert!(hold(&files, Access::Removal).is_err());
        assert!(std::fs::write(&path, b"changed").is_err());
        assert!(std::fs::remove_file(&path).is_err());
        drop(first);
        assert!(hold(&files, Access::Removal).is_err());
        drop(second);
        let removal = hold(&files, Access::Removal).unwrap();
        assert!(hold(&files, Access::Runtime).is_err());
        std::fs::remove_file(&path).unwrap();
        drop(removal);
        std::fs::remove_dir(dir).unwrap();
    }
}
