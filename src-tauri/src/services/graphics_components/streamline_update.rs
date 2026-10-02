//! Discover compatible layer releases without changing an installed target.
use super::{
    streamline_download,
    streamline_install::{self as install, Package},
};
use crate::services::network;
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::Notify;

const REPOSITORY: &str = "https://github.com/triwinds/ns-emu-tools-runtimes/releases/download/";
const RELEASES: &str =
    "https://api.github.com/repos/triwinds/ns-emu-tools-runtimes/releases?per_page=100";
const LIMIT: usize = 131072;
const MUTABLE: [&str; 3] = [
    "streamline-layer-probe.exe",
    "streamline_probe_layer.dll",
    "nvngx.dll",
];
static CANDIDATE: RwLock<Option<Package>> = RwLock::new(None);
const FOREGROUND_WAIT: Duration = Duration::from_millis(1500);
const REFRESH_TIMEOUT: Duration = Duration::from_secs(8);
const CACHE_TTL: Duration = Duration::from_secs(60);
const FAILURE_TTL: Duration = Duration::from_secs(10);
static UPDATE_CACHE: Lazy<Arc<UpdateCache>> = Lazy::new(|| Arc::new(UpdateCache::default()));

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    published_at: Option<String>,
    assets: Vec<ReleaseAsset>,
}
#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}
#[derive(Clone)]
pub(super) struct Update {
    pub package: Option<Package>,
    pub message: String,
    pub fresh: bool,
}
#[derive(Default)]
struct RefreshState {
    result: Option<(Instant, Update)>,
    running: bool,
}
#[derive(Default)]
struct UpdateCache {
    state: Mutex<RefreshState>,
    done: Notify,
}
impl UpdateCache {
    async fn check(
        self: &Arc<Self>,
        query: impl std::future::Future<Output = Update> + Send + 'static,
        wait: Duration,
    ) -> Update {
        let notified = self.done.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        let (start, cached) = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((when, result)) = &state.result {
                let ttl = if result.fresh { CACHE_TTL } else { FAILURE_TTL };
                if when.elapsed() < ttl {
                    return result.clone();
                }
            }
            let start = !state.running;
            state.running = true;
            (
                start,
                state.result.as_ref().map(|(_, result)| result.clone()),
            )
        };
        if start {
            let cache = self.clone();
            tokio::spawn(async move {
                let result = query.await;
                {
                    let mut state = cache.state.lock().unwrap_or_else(|e| e.into_inner());
                    state.running = false;
                    state.result = Some((Instant::now(), result));
                }
                cache.done.notify_waiters();
            });
        }
        // An existing validated manifest can be shown immediately while a refresh
        // continues. A first check waits only briefly; it never cancels the refresh.
        if let Some(mut cached) = cached {
            cached.fresh = false;
            cached.message = "正在后台刷新组件版本；当前显示缓存的版本信息。".into();
            return cached;
        }
        if tokio::time::timeout(wait, notified).await.is_ok() {
            if let Some((_, result)) = &self.state.lock().unwrap_or_else(|e| e.into_inner()).result
            {
                return result.clone();
            }
        }
        Update {
            package: None,
            fresh: false,
            message: "远端版本仍在后台查询，本地检查已完成；稍后重新检查即可查看更新结果。".into(),
        }
    }
}
pub(super) fn candidate() -> Package {
    CANDIDATE
        .read()
        .ok()
        .and_then(|p| p.clone())
        .unwrap_or_else(install::package)
}
pub(super) fn candidate_for(exe: &std::path::Path) -> Package {
    let installed = install::installed_package(exe).ok();
    let candidate = CANDIDATE.read().ok().and_then(|p| p.clone());
    candidate
        .filter(|next| {
            installed
                .as_ref()
                .is_none_or(|current| current.version != next.version || same_files(current, next))
        })
        .or(installed)
        .unwrap_or_else(install::package)
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn release_url(url: &str, name: &str) -> bool {
    let Some(tail) = url.strip_prefix(REPOSITORY) else {
        return false;
    };
    let Some((tag, filename)) = tail.split_once('/') else {
        return false;
    };
    !tag.is_empty()
        && tag
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && filename == name
        && !filename.contains(['/', '\\', '?', '#', '%', ':'])
}
pub(super) fn validate_selected(package: &Package) -> Result<(), String> {
    let embedded = install::package();
    if package.version == embedded.version && same_files(package, &embedded) {
        return Ok(());
    }
    if !package.online() && package.version.starts_with("local-") {
        if package.version.contains(['/', '\\', ':'])
            || package.files.is_empty()
            || package.files.len() > 64
        {
            return Err("本地组件清单无效".into());
        }
        let mut names = std::collections::HashSet::new();
        if package.files.iter().any(|f| {
            f.name.is_empty()
                || f.name.contains(['/', '\\', ':'])
                || matches!(f.name.as_str(), "." | "..")
                || !digest(&f.sha256)
                || !names.insert(f.name.to_ascii_lowercase())
        }) {
            return Err("本地组件文件清单无效".into());
        }
        return Ok(());
    }
    validate_contract(package, false)
}
pub(super) fn same_files(a: &Package, b: &Package) -> bool {
    a.files.len() == b.files.len()
        && a.files.iter().all(|f| {
            b.files
                .iter()
                .any(|other| f.name == other.name && f.sha256 == other.sha256)
        })
}
pub(super) fn validate(package: &Package) -> Result<(), String> {
    validate_contract(package, true)
}
fn validate_contract(package: &Package, require_current_runtime: bool) -> Result<(), String> {
    if package.schema_version != 1
        || package.launcher_protocol != 1
        || !package.native_nr
        || !package.version.starts_with("streamline-layer-")
        || package.parts.len() != 2
        || package.files.len() > 64
    {
        return Err("组件清单协议不兼容，请更新工具箱".into());
    }
    streamline_download::downloads(package)?;
    if package.files.iter().any(|f| !digest(&f.sha256)) {
        return Err("组件文件校验值无效".into());
    }
    for part in &package.parts {
        if !digest(&part.download.sha256) || !release_url(&part.download.url, &part.download.name) {
            return Err("组件包必须来自官方 runtimes Release".into());
        }
    }
    let layer_name = format!("{}.zip", package.version);
    let layer = package
        .parts
        .iter()
        .find(|p| p.download.name == layer_name)
        .ok_or("组件小包版本与清单不一致")?;
    if layer.download.url != format!("{REPOSITORY}{}/{layer_name}", package.version)
        || MUTABLE
            .iter()
            .any(|name| !layer.files.iter().any(|n| n == name))
    {
        return Err("组件小包缺少启动器或调用桥".into());
    }
    let embedded = install::package();
    for baseline in embedded.files.iter().filter(|f| {
        require_current_runtime && f.name.ends_with(".dll") && !MUTABLE.contains(&f.name.as_str())
    }) {
        if !package
            .files
            .iter()
            .any(|f| f.name == baseline.name && f.sha256 == baseline.sha256)
        {
            return Err("稳定运行库与当前工具箱不兼容，请更新工具箱".into());
        }
    }
    if !super::native_nr::supported_package(package) {
        return Err("NR 运行库不在当前工具箱实测范围内".into());
    }
    if package.files.iter().any(|f| {
        (f.name.ends_with(".dll") || f.name.ends_with(".exe"))
            && !embedded.files.iter().any(|old| old.name == f.name)
            && f.name != "nvngx_dlssnr.dll"
    }) {
        return Err("组件清单包含未知二进制文件，请更新工具箱".into());
    }
    Ok(())
}
fn manifests(bytes: serde_json::Value) -> Result<Vec<(String, String)>, String> {
    let mut releases: Vec<Release> = serde_json::from_value(bytes).map_err(|e| e.to_string())?;
    releases.sort_by(|a, b| b.published_at.cmp(&a.published_at));
    let mut result = Vec::new();
    // These experimental components may be marked prerelease. Runtime-only and
    // unrelated AIO releases must never hide the newest layer publication.
    for release in releases.into_iter().filter(|r| {
        !r.draft && r.tag_name.starts_with("streamline-layer-") && r.published_at.is_some()
    }) {
        let wanted = format!("{}-manifest.json", release.tag_name);
        if let Some(asset) = release.assets.into_iter().find(|a| a.name == wanted) {
            if asset.size == 0
                || asset.size > LIMIT as u64
                || asset.browser_download_url
                    != format!("{REPOSITORY}{}/{wanted}", release.tag_name)
            {
                return Err("Release 组件清单地址或大小无效".into());
            }
            result.push((release.tag_name, asset.browser_download_url));
        }
    }
    Ok(result)
}
async fn fetch_manifest(tag: &str, url: &str) -> Result<Package, String> {
    let target = network::resolve_github_download_target(url);
    let client =
        network::create_client_with_timeout(Duration::from_secs(6)).map_err(|e| e.to_string())?;
    fetch_manifest_from(&client, tag, &target.url, url).await
}
async fn fetch_manifest_from(
    client: &reqwest::Client,
    tag: &str,
    preferred: &str,
    official: &str,
) -> Result<Package, String> {
    let read = |url| async move { parse_manifest(tag, &read_manifest(client, url).await?) };
    if preferred == official {
        return read(official).await;
    }
    let preferred_read = read(preferred);
    let official_read = read(official);
    tokio::pin!(preferred_read, official_read);
    tokio::select! {
        result = &mut preferred_read => match result {
            Ok(package) => Ok(package),
            Err(error) => official_read.await.map_err(|fallback| format!("镜像清单请求失败：{error}；官方源失败：{fallback}")),
        },
        result = &mut official_read => match result {
            Ok(package) => Ok(package),
            Err(error) => preferred_read.await.map_err(|fallback| format!("官方清单请求失败：{error}；镜像失败：{fallback}")),
        },
    }
}
async fn read_manifest(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    if response.content_length().is_some_and(|n| n > LIMIT as u64) {
        return Err("组件清单过大".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > LIMIT {
            return Err("组件清单过大".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
fn parse_manifest(tag: &str, bytes: &[u8]) -> Result<Package, String> {
    let package: Package = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if package.version != tag {
        return Err("Release 标签与组件清单版本不一致".into());
    }
    validate(&package)?;
    Ok(package)
}
async fn latest() -> Result<Option<Package>, String> {
    let started = Instant::now();
    let releases = network::request_github_api(RELEASES)
        .await
        .map_err(|e| e.to_string())?;
    let candidates = manifests(releases)?;
    let Some((tag, url)) = candidates.first() else {
        return Ok(None);
    };
    tracing::info!(
        elapsed_ms = started.elapsed().as_millis(),
        "画面增强组件 Release 列表已返回"
    );
    if let Some(package) = CANDIDATE
        .read()
        .ok()
        .and_then(|p| p.clone())
        .filter(|p| &p.version == tag)
    {
        return Ok(Some(package)); // Immutable release manifests never need redownloading.
    }
    let package = fetch_manifest(tag, url).await?;
    tracing::info!(elapsed_ms = started.elapsed().as_millis(), version = %package.version, "画面增强组件远端清单已校验");
    Ok(Some(package))
}
pub(super) async fn check() -> Update {
    UPDATE_CACHE.check(refresh(), FOREGROUND_WAIT).await
}
async fn refresh() -> Update {
    let result = tokio::time::timeout(REFRESH_TIMEOUT, latest()).await;
    match result {
        Ok(Ok(package)) => {
            if let Ok(mut cached) = CANDIDATE.write() {
                *cached = package.clone();
            }
            Update {
                fresh: true,
                message: if package.is_some() {
                    "已检查远端组件版本".into()
                } else {
                    "远端尚未发布支持独立更新的小包清单，继续使用现有组件".into()
                },
                package,
            }
        }
        other => {
            let error = match other {
                Ok(Err(e)) => e,
                _ => "检查超时".into(),
            };
            Update {
                package: None,
                fresh: false,
                message: format!("未能检查远端组件更新：{error}。已有安装仍可使用。"),
            }
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    pub(crate) fn fixture() -> Package {
        let embedded = install::package();
        let mut files = embedded.files;
        if !files.iter().any(|f| f.name == "nvngx_dlssnr.dll") {
            files.push(install::Artifact {
                name: "nvngx_dlssnr.dll".into(),
                sha256: "e16bcf15e16e13f527491cdf7845b2fe6521a738d8f7c9c721866a8496e1fc8e".into(),
            });
        }
        let version = "streamline-layer-test-v1";
        serde_json::from_value(serde_json::json!({"version":version,"native_nr":true,"schema_version":1,"launcher_protocol":1,"files":files,"parts":[
            {"files":MUTABLE,"download":{"name":format!("{version}.zip"),"url":format!("{REPOSITORY}{version}/{version}.zip"),"sha256":"a".repeat(64)}},
            {"files":files.iter().filter(|f| !MUTABLE.contains(&f.name.as_str())).map(|f| &f.name).collect::<Vec<_>>(),"download":{"name":"stable.zip","url":format!("{REPOSITORY}runtime-v1/stable.zip"),"sha256":"b".repeat(64)}}
        ]})).unwrap()
    }
    #[test]
    fn validates_protocol_origin_membership_and_stable_runtime_contract() {
        let mut package = fixture();
        validate(&package).unwrap();
        package
            .files
            .iter_mut()
            .find(|f| f.name == "nvngx.dll")
            .unwrap()
            .sha256 = "c".repeat(64);
        validate(&package).unwrap(); // bridge upgrades use the matching launcher in the same ZIP.
        package.launcher_protocol = 2;
        assert!(validate(&package).is_err());
        package = fixture();
        package.parts[0].download.url = "https://example.com/malware.zip".into();
        assert!(validate(&package).is_err());
        package = fixture();
        package
            .files
            .iter_mut()
            .find(|f| f.name == "sl.common.dll")
            .unwrap()
            .sha256 = "d".repeat(64);
        assert!(validate(&package).is_err());
        package = fixture();
        package.parts[1].files.push("nvngx.dll".into());
        assert!(validate(&package).is_err());
    }
    #[test]
    fn release_discovery_uses_layer_publication_dates_and_accepts_prereleases() {
        let release = |tag: &str, date: &str, draft: bool| serde_json::json!({"tag_name":tag,"published_at":date,"draft":draft,"prerelease":true,"assets":[{"name":format!("{tag}-manifest.json"),"browser_download_url":format!("{REPOSITORY}{tag}/{tag}-manifest.json"),"size":4096}]});
        let found = manifests(serde_json::json!([
            release("streamline-runtime-new", "2026-10-04T00:00:00Z", false),
            release("streamline-layer-old", "2026-10-01T00:00:00Z", false),
            release("streamline-layer-draft", "2026-10-05T00:00:00Z", true),
            release("streamline-layer-new", "2026-10-02T00:00:00Z", false)
        ]))
        .unwrap();
        assert_eq!(
            found
                .iter()
                .map(|(tag, _)| tag.as_str())
                .collect::<Vec<_>>(),
            ["streamline-layer-new", "streamline-layer-old"]
        );
    }
    #[test]
    fn persisted_remote_packages_keep_their_contract_after_toolbox_runtime_changes() {
        let mut package = fixture();
        package
            .files
            .iter_mut()
            .find(|f| f.name == "sl.common.dll")
            .unwrap()
            .sha256 = "e".repeat(64);
        assert!(validate(&package).is_err()); // not an update compatible with this toolbox baseline.
        validate_selected(&package).unwrap(); // already installed against its own full file manifest.
    }
    #[tokio::test]
    async fn manifest_http_errors_oversize_and_release_mismatch_are_rejected() {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };
        let server = MockServer::start().await;
        let package = fixture();
        Mock::given(method("GET"))
            .and(path("/valid"))
            .respond_with(
                ResponseTemplate::new(200).set_body_bytes(serde_json::to_vec(&package).unwrap()),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/oversize"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b'x'; LIMIT + 1]))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/missing"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let bytes = read_manifest(&client, &format!("{}/valid", server.uri()))
            .await
            .unwrap();
        assert_eq!(
            parse_manifest(&package.version, &bytes).unwrap().version,
            package.version
        );
        assert!(parse_manifest("streamline-layer-wrong", &bytes).is_err());
        assert!(
            read_manifest(&client, &format!("{}/oversize", server.uri()))
                .await
                .is_err()
        );
        assert!(read_manifest(&client, &format!("{}/missing", server.uri()))
            .await
            .is_err());
    }
    #[tokio::test]
    #[ignore = "Read-only live query of the runtimes Release list"]
    async fn published_layer_discovery() {
        let package = latest().await.unwrap();
        println!(
            "Discovered layer: {}",
            package
                .as_ref()
                .map(|p| p.version.as_str())
                .unwrap_or("no split layer release published")
        );
    }
    #[tokio::test]
    async fn slow_refresh_is_bounded_shared_and_cached_without_canceling_work() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let cache = Arc::new(UpdateCache::default());
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        let query = async move {
            counted.fetch_add(1, Ordering::SeqCst);
            receive.await.unwrap();
            Update {
                package: Some(fixture()),
                message: "checked".into(),
                fresh: true,
            }
        };
        let started = Instant::now();
        let pending = cache.check(query, Duration::from_millis(20)).await;
        assert!(!pending.fresh && pending.package.is_none());
        assert!(started.elapsed() < Duration::from_millis(500));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        send.send(()).unwrap();
        let completed = cache
            .check(
                async { panic!("duplicate refresh") },
                Duration::from_secs(1),
            )
            .await;
        assert!(completed.fresh && completed.package.is_some());
        let cached = cache
            .check(async { panic!("cached refresh") }, Duration::ZERO)
            .await;
        assert!(cached.fresh && cached.package.is_some());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        cache.state.lock().unwrap().result.as_mut().unwrap().0 =
            Instant::now() - CACHE_TTL - Duration::from_secs(1);
        let (send, receive) = tokio::sync::oneshot::channel();
        let stale = cache
            .check(
                async move {
                    receive.await.unwrap();
                    Update {
                        package: None,
                        message: "offline".into(),
                        fresh: false,
                    }
                },
                Duration::ZERO,
            )
            .await;
        assert!(!stale.fresh && stale.package.is_some());
        // Wait for the background completion, then verify retries respect the failure cooldown.
        let done = cache.done.notified();
        tokio::pin!(done);
        done.as_mut().enable();
        send.send(()).unwrap();
        done.await;
        let failed = cache
            .check(async { panic!("retry before cooldown") }, Duration::ZERO)
            .await;
        assert!(!failed.fresh && failed.message == "offline");
    }
    #[tokio::test]
    async fn slow_or_invalid_mirrors_do_not_block_the_official_manifest() {
        use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        let package = fixture();
        let body = serde_json::to_vec(&package).unwrap();
        Mock::given(path("/slow"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(body.clone())
                    .set_delay(Duration::from_secs(3)),
            )
            .mount(&server)
            .await;
        Mock::given(path("/official"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .mount(&server)
            .await;
        Mock::given(path("/invalid"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not a manifest"))
            .mount(&server)
            .await;
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        for path in ["/slow", "/invalid"] {
            let found = tokio::time::timeout(
                Duration::from_secs(1),
                fetch_manifest_from(
                    &client,
                    &package.version,
                    &format!("{}{path}", server.uri()),
                    &format!("{}/official", server.uri()),
                ),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(found.version, package.version);
        }
    }
}
