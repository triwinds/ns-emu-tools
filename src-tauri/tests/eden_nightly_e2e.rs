//! Opt-in checks against Eden's official nightly release and package servers.

use ns_emu_tools_lib::commands::yuzu::get_all_yuzu_versions;
use ns_emu_tools_lib::models::yuzu_branch::EDEN_NIGHTLY_BRANCH;
use ns_emu_tools_lib::repositories::yuzu::{
    get_latest_change_log, get_yuzu_release_info_by_version, yuzu_release_api_for_branch,
};
use ns_emu_tools_lib::services::yuzu::select_current_platform_yuzu_asset;

#[tokio::test]
#[ignore = "requires access to Eden's official servers"]
async fn eden_nightly_release_and_download_e2e() {
    assert!(yuzu_release_api_for_branch(EDEN_NIGHTLY_BRANCH)
        .unwrap()
        .contains("eden-ci/nightly/releases"));
    let response = get_all_yuzu_versions(EDEN_NIGHTLY_BRANCH.to_string())
        .await
        .unwrap();
    let versions = response.data.unwrap();
    assert!(!versions.is_empty());
    let release = get_yuzu_release_info_by_version(&versions[0], EDEN_NIGHTLY_BRANCH)
        .await
        .unwrap();
    assert_eq!(release.tag_name, versions[0]);
    assert!(!release.assets.is_empty());
    assert_eq!(
        get_latest_change_log(EDEN_NIGHTLY_BRANCH).await.unwrap(),
        release.description
    );

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        let download_url = select_current_platform_yuzu_asset(&release, EDEN_NIGHTLY_BRANCH)
            .expect("nightly release should include a compatible package");
        let response = ns_emu_tools_lib::services::network::create_client()
            .unwrap()
            .head(&download_url)
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success(), "{download_url}");
        println!("Eden Nightly {}: {download_url}", versions[0]);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    assert!(select_current_platform_yuzu_asset(&release, EDEN_NIGHTLY_BRANCH).is_none());
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
#[tokio::test]
#[ignore = "downloads and installs Eden nightly into a temporary directory"]
async fn eden_nightly_install_and_redetect_e2e() {
    use ns_emu_tools_lib::config::{get_config, Config, CONFIG};
    use ns_emu_tools_lib::models::yuzu_branch::EDEN_BRANCH;
    use ns_emu_tools_lib::services::yuzu::{detect_yuzu_version, get_yuzu_exe_path, install_yuzu};

    struct ConfigRestoreGuard(Config);
    impl Drop for ConfigRestoreGuard {
        fn drop(&mut self) {
            let mut config = CONFIG.write();
            *config = self.0.clone();
            config.save().expect("restore config after nightly test");
        }
    }

    let _restore = ConfigRestoreGuard(get_config());
    let directory = tempfile::tempdir().unwrap();
    {
        let mut config = CONFIG.write();
        config.yuzu.yuzu_path = directory.path().join("eden");
        config.yuzu.yuzu_version = None;
        config.yuzu.branch = EDEN_NIGHTLY_BRANCH.to_string();
        config.setting.download.backend = "rust".to_string();
        config.setting.download.auto_delete_after_install = true;
        config.setting.other.rename_yuzu_to_cemu = false;
    }
    let versions = get_all_yuzu_versions(EDEN_NIGHTLY_BRANCH.to_string())
        .await
        .unwrap()
        .data
        .unwrap();
    let version = &versions[0];
    install_yuzu(version, EDEN_NIGHTLY_BRANCH, |_| {})
        .await
        .unwrap();
    assert!(get_yuzu_exe_path().is_file());
    assert_eq!(get_config().yuzu.branch, EDEN_NIGHTLY_BRANCH);
    assert_eq!(get_config().yuzu.yuzu_version.as_ref(), Some(version));

    // Simulate a restart without an in-memory installed version.
    CONFIG.write().yuzu.yuzu_version = None;
    assert_eq!(detect_yuzu_version().await.unwrap().as_ref(), Some(version));
    assert_eq!(get_config().yuzu.branch, EDEN_NIGHTLY_BRANCH);

    // A manually extracted package has no installation record. Its embedded
    // commit/branch version still identifies nightly even with a stale config.
    std::fs::remove_file(
        get_config()
            .yuzu
            .yuzu_path
            .join(".ns-emu-tools-install.json"),
    )
    .unwrap();
    {
        let mut config = CONFIG.write();
        config.yuzu.yuzu_version = None;
        config.yuzu.branch = EDEN_BRANCH.to_string();
    }
    let binary_version = detect_yuzu_version().await.unwrap().unwrap();
    let commit = version.rsplit('.').next().unwrap();
    assert!(binary_version.starts_with(commit), "{binary_version}");
    assert_eq!(get_config().yuzu.branch, EDEN_NIGHTLY_BRANCH);
}
