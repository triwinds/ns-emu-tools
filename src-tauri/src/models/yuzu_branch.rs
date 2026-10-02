//! Yuzu-family branch identifiers and compatibility helpers.

pub const EDEN_BRANCH: &str = "eden";
pub const EDEN_NIGHTLY_BRANCH: &str = "eden-nightly";
pub const CITRON_STABLE_BRANCH: &str = "citron-stable";
pub const CITRON_NIGHTLY_BRANCH: &str = "citron-nightly";
pub const LEGACY_CITRON_BRANCH: &str = "citron";
pub const YUZU_MAINLINE_BRANCH: &str = "mainline";
pub const YUZU_EA_BRANCH: &str = "ea";
pub const LEGACY_YUZU_BRANCH: &str = "yuzu";

pub const DOWNLOAD_AVAILABLE_BRANCHES: &[&str] = &[
    EDEN_BRANCH,
    EDEN_NIGHTLY_BRANCH,
    CITRON_STABLE_BRANCH,
    CITRON_NIGHTLY_BRANCH,
];

pub fn normalize_yuzu_branch(branch: &str) -> Option<&'static str> {
    match branch {
        EDEN_BRANCH => Some(EDEN_BRANCH),
        EDEN_NIGHTLY_BRANCH => Some(EDEN_NIGHTLY_BRANCH),
        LEGACY_CITRON_BRANCH | CITRON_STABLE_BRANCH => Some(CITRON_STABLE_BRANCH),
        CITRON_NIGHTLY_BRANCH => Some(CITRON_NIGHTLY_BRANCH),
        YUZU_MAINLINE_BRANCH => Some(YUZU_MAINLINE_BRANCH),
        YUZU_EA_BRANCH => Some(YUZU_EA_BRANCH),
        LEGACY_YUZU_BRANCH => Some(LEGACY_YUZU_BRANCH),
        _ => None,
    }
}

pub fn normalize_downloadable_yuzu_branch(branch: &str) -> Option<&'static str> {
    match normalize_yuzu_branch(branch)? {
        EDEN_BRANCH => Some(EDEN_BRANCH),
        EDEN_NIGHTLY_BRANCH => Some(EDEN_NIGHTLY_BRANCH),
        CITRON_STABLE_BRANCH => Some(CITRON_STABLE_BRANCH),
        CITRON_NIGHTLY_BRANCH => Some(CITRON_NIGHTLY_BRANCH),
        _ => None,
    }
}

pub fn yuzu_user_dir_branch(branch: &str) -> Option<&'static str> {
    match normalize_yuzu_branch(branch)? {
        EDEN_BRANCH | EDEN_NIGHTLY_BRANCH => Some(EDEN_BRANCH),
        CITRON_STABLE_BRANCH | CITRON_NIGHTLY_BRANCH => Some(LEGACY_CITRON_BRANCH),
        YUZU_MAINLINE_BRANCH | YUZU_EA_BRANCH | LEGACY_YUZU_BRANCH => Some(LEGACY_YUZU_BRANCH),
        _ => None,
    }
}

pub fn is_citron_branch(branch: &str) -> bool {
    matches!(
        normalize_yuzu_branch(branch),
        Some(CITRON_STABLE_BRANCH | CITRON_NIGHTLY_BRANCH)
    )
}

pub fn is_downloadable_yuzu_branch(branch: &str) -> bool {
    normalize_downloadable_yuzu_branch(branch).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eden_channels_share_user_dir_but_keep_download_channel() {
        assert_eq!(
            normalize_downloadable_yuzu_branch(EDEN_NIGHTLY_BRANCH),
            Some(EDEN_NIGHTLY_BRANCH)
        );
        assert_eq!(yuzu_user_dir_branch(EDEN_NIGHTLY_BRANCH), Some(EDEN_BRANCH));
        assert!(DOWNLOAD_AVAILABLE_BRANCHES.contains(&EDEN_NIGHTLY_BRANCH));
    }

    #[test]
    fn legacy_citron_normalizes_to_stable() {
        assert_eq!(
            normalize_yuzu_branch(LEGACY_CITRON_BRANCH),
            Some(CITRON_STABLE_BRANCH)
        );
        assert_eq!(
            normalize_downloadable_yuzu_branch(LEGACY_CITRON_BRANCH),
            Some(CITRON_STABLE_BRANCH)
        );
    }

    #[test]
    fn citron_channels_share_physical_user_dir() {
        assert_eq!(
            yuzu_user_dir_branch(CITRON_STABLE_BRANCH),
            Some(LEGACY_CITRON_BRANCH)
        );
        assert_eq!(
            yuzu_user_dir_branch(CITRON_NIGHTLY_BRANCH),
            Some(LEGACY_CITRON_BRANCH)
        );
    }
}
