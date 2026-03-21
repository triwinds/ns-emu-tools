//! 配置数据访问层
//!
//! 提供静态镜像配置和游戏数据等仓库能力。

use crate::error::{AppError, AppResult};
use crate::services::network::{self, get_durable_cached_client, get_final_url};
use serde_json::Value;
use std::collections::HashMap;
use tracing::{info, warn};

const REMOTE_GAME_DATA_URL: &str =
    "https://raw.githubusercontent.com/triwinds/ns-emu-tools/main/game_data.json";
const LOCAL_GAME_DATA_JSON: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../game_data.json"));

/// 获取可用的 GitHub 镜像列表。
pub fn get_github_mirrors() -> Vec<(String, String, String)> {
    network::get_github_mirrors()
        .into_iter()
        .map(|mirror| (mirror.url, mirror.region, mirror.description))
        .collect()
}

fn parse_game_data(contents: &str) -> AppResult<HashMap<String, Value>> {
    serde_json::from_str(contents).map_err(AppError::from)
}

fn load_local_game_data() -> AppResult<HashMap<String, Value>> {
    parse_game_data(LOCAL_GAME_DATA_JSON)
}

fn fallback_to_local_game_data(reason: &str) -> AppResult<HashMap<String, Value>> {
    warn!("获取远程游戏数据失败，回退到本地数据: {}", reason);
    let data = load_local_game_data()?;
    info!("已从本地游戏数据加载 {} 个条目", data.len());
    Ok(data)
}

/// 获取游戏数据映射 (Title ID -> Game metadata)。
pub async fn get_game_data() -> AppResult<HashMap<String, Value>> {
    info!("获取游戏数据映射");

    let final_url = get_final_url(REMOTE_GAME_DATA_URL);
    let client = get_durable_cached_client();

    match client.get(&final_url).send().await {
        Ok(response) => {
            let status = response.status();
            if !status.is_success() {
                return fallback_to_local_game_data(&format!("HTTP {}", status));
            }

            match response.json::<HashMap<String, Value>>().await {
                Ok(data) => {
                    info!("成功获取 {} 个游戏数据条目", data.len());
                    Ok(data)
                }
                Err(e) => fallback_to_local_game_data(&format!("invalid JSON: {}", e)),
            }
        }
        Err(e) => fallback_to_local_game_data(&e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_github_mirrors() {
        let mirrors = get_github_mirrors();
        assert!(mirrors.len() > 2);
        assert_eq!(mirrors[0].0, "cloudflare_load_balance");
    }

    #[test]
    fn test_load_local_game_data() {
        let data = load_local_game_data().expect("expected embedded game data");
        assert!(!data.is_empty());
    }

    #[test]
    fn test_fallback_to_local_game_data_returns_embedded_payload() {
        let data = fallback_to_local_game_data("unit-test").expect("fallback should succeed");
        assert!(!data.is_empty());
    }

    #[test]
    fn test_parse_game_data_rejects_invalid_json() {
        assert!(parse_game_data("{ invalid json }").is_err());
    }
}
