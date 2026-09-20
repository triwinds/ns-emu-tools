//! 官方稳定版 full addon 来源；前端不提供 URL 或版本拼接片段。
use crate::models::graphics_components::GraphicsComponentVersion;

pub const OFFICIAL_SITE: &str = "https://reshade.me/";

pub fn parse_release(html: &str) -> Result<GraphicsComponentVersion, String> {
    let pattern = regex::Regex::new(r#"(?i)href\s*=\s*["'](?:https://reshade\.me)?(/downloads/ReShade_Setup_([0-9]+\.[0-9]+\.[0-9]+)_Addon\.exe)["']"#).map_err(|e| e.to_string())?;
    let mut versions = std::collections::BTreeMap::new();
    for capture in pattern.captures_iter(html) {
        let parts = capture[2]
            .split('.')
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "官方版本号无效")?;
        versions.insert(parts, (capture[2].to_string(), capture[1].to_string()));
    }
    let (_, (version, path)) = versions
        .pop_last()
        .ok_or("官网没有可识别的 full addon 稳定版链接")?;
    Ok(GraphicsComponentVersion {
        version,
        source_url: format!("https://reshade.me{path}"),
        channel: "stable-addon".into(),
    })
}

pub async fn latest() -> Result<GraphicsComponentVersion, String> {
    let client = crate::services::network::create_client().map_err(|e| e.to_string())?;
    let mut response = client
        .get(OFFICIAL_SITE)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    if response.url().scheme() != "https" || response.url().host_str() != Some("reshade.me") {
        return Err("官网请求重定向到非官方地址".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err("官网响应过大".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    parse_release(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_official_addon_links_and_numeric_version_order() {
        let html = r#"<a href="https://evil.test/downloads/ReShade_Setup_99.0.0_Addon.exe"></a>
        <a href="/downloads/ReShade_Setup_6.9.0_Addon.exe"></a>
        <a href='https://reshade.me/downloads/ReShade_Setup_6.10.0_Addon.exe'></a>
        <a href="/downloads/ReShade_Setup_99.0.0.exe"></a>"#;
        assert_eq!(parse_release(html).unwrap().version, "6.10.0");
        assert!(parse_release(
            r#"<a href="https://evil.test/downloads/ReShade_Setup_99.0.0_Addon.exe">"#
        )
        .is_err());
        assert!(parse_release("/downloads/ReShade_Setup_6.8.0_Addon.exe").is_err());
    }
}
