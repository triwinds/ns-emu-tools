//! Portable NR-only presets. Metadata describes a verified installed snapshot,
//! never the binaries already loaded by a running game.
use super::advanced_settings::NrOptions;
use serde::{Deserialize, Deserializer, Serialize};

pub const MAX_JSON_BYTES: usize = 64 * 1024;
pub const MAX_PRESETS: usize = 64;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NrPreset {
    #[serde(deserialize_with = "schema")]
    pub schema_version: u16,
    #[serde(deserialize_with = "name")]
    pub name: String,
    pub emulator: Emulator,
    #[serde(default, deserialize_with = "label")]
    pub game: String,
    #[serde(default, deserialize_with = "label")]
    pub display_mode: String,
    pub settings: NrSettings,
    #[serde(default)]
    pub environment: Environment,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Emulator {
    Eden,
    Citron,
    Yuzu,
    Ryujinx,
    Other,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NrSettings {
    pub enabled: bool,
    #[serde(deserialize_with = "intensity")]
    pub intensity: u16,
    /// V1 presets without advanced controls migrate to the neutral defaults.
    #[serde(default)]
    pub options: NrOptions,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Environment {
    #[serde(default, deserialize_with = "label")]
    pub toolbox_version: String,
    #[serde(default, deserialize_with = "label")]
    pub component_version: String,
    #[serde(default, deserialize_with = "hash")]
    pub component_sha256: Option<String>,
    #[serde(default, deserialize_with = "label")]
    pub model_version: String,
    #[serde(default, deserialize_with = "hash")]
    pub model_sha256: Option<String>,
    #[serde(default, deserialize_with = "hash")]
    pub target_sha256: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub preset: NrPreset,
    pub migrated_from: Option<u16>,
}

fn schema<'de, D: Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    match u16::deserialize(d)? {
        1 | 2 => Ok(2),
        _ => Err(serde::de::Error::custom("不支持的 NR 预设版本")),
    }
}
fn label<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if value.chars().count() > 160 || value.chars().any(char::is_control) {
        return Err(serde::de::Error::custom(
            "预设标签最多 160 字符，不能包含控制字符",
        ));
    }
    Ok(value)
}
fn name<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = label(d)?;
    if value.trim().is_empty() {
        return Err(serde::de::Error::custom("预设名称不能为空"));
    }
    Ok(value)
}
fn intensity<'de, D: Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    let value = u16::deserialize(d)?;
    if value > 200 {
        return Err(serde::de::Error::custom("NR 强度必须为 0～200 的整数"));
    }
    Ok(value)
}
fn hash<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let value = Option::<String>::deserialize(d)?;
    if value
        .as_ref()
        .is_some_and(|v| v.len() != 64 || !v.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(serde::de::Error::custom(
            "预设 SHA-256 必须是 64 位十六进制",
        ));
    }
    Ok(value.map(|v| v.to_ascii_lowercase()))
}

pub fn deserialize_library<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<NrPreset>, D::Error> {
    let presets = Vec::<NrPreset>::deserialize(d)?;
    if presets.len() > MAX_PRESETS {
        return Err(serde::de::Error::custom("最多保存 64 个 NR 预设"));
    }
    Ok(presets)
}

pub fn import(json: &str) -> Result<ImportResult, String> {
    if json.len() > MAX_JSON_BYTES {
        return Err("NR 预设文件不能超过 64 KiB".into());
    }
    // Deserialize the original text so duplicate fields also fail validation.
    let preset: NrPreset = serde_json::from_str(json).map_err(|e| format!("NR 预设无效：{e}"))?;
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(ImportResult {
        preset,
        migrated_from: (value["schemaVersion"] == 1).then_some(1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> serde_json::Value {
        json!({"schemaVersion":2,"name":"夜景","emulator":"eden","game":"游戏 A",
            "displayMode":"SDR 1440p","settings":{"enabled":true,"intensity":150,
            "options":{"style":"b","globalTone":0,"look":{"amount":120,
                "temporal":{"enabled":true,"timeMs":100,"strength":75}},
                "secondPass":{"enabled":true,"inherit":false,"intensity":50}}}})
    }
    #[test]
    fn roundtrip_preserves_explicit_zero_and_all_stages() {
        let preset = import(&fixture().to_string()).unwrap().preset;
        assert_eq!(preset.settings.options.global_tone, Some(0));
        assert!(preset.settings.options.second_pass.enabled);
        assert!(preset.settings.options.look.temporal.enabled);
        assert_eq!(
            import(&serde_json::to_string(&preset).unwrap())
                .unwrap()
                .preset,
            preset
        );
    }
    #[test]
    fn v1_migration_fills_missing_controls_and_keeps_explicit_choices() {
        let mut value = fixture();
        value["schemaVersion"] = json!(1);
        let result = import(&value.to_string()).unwrap();
        assert_eq!(result.migrated_from, Some(1));
        assert_eq!(result.preset.schema_version, 2);
        assert_eq!(result.preset.settings.options.global_tone, Some(0));
        value["settings"].as_object_mut().unwrap().remove("options");
        let legacy = import(&value.to_string()).unwrap().preset;
        assert_eq!(legacy.settings.intensity, 150);
        assert_eq!(legacy.settings.options, NrOptions::default());
    }
    #[test]
    fn rejects_invalid_numbers_enums_unknown_and_duplicate_fields() {
        for number in [json!(-1), json!(201), json!(0.5), json!("NaN"), json!(null)] {
            let mut value = fixture();
            value["settings"]["intensity"] = number;
            assert!(import(&value.to_string()).is_err());
        }
        for (pointer, invalid) in [
            ("/schemaVersion", json!(3)),
            ("/emulator", json!("unknown")),
            ("/settings/options/style", json!("d")),
            ("/settings/options/look/amount", json!(201)),
            ("/settings/options/look/temporal/strength", json!(91)),
            ("/settings/options/secondPass/intensity", json!(201)),
        ] {
            let mut value = fixture();
            *value.pointer_mut(pointer).unwrap() = invalid;
            assert!(import(&value.to_string()).is_err(), "{pointer}");
        }
        let mut value = fixture();
        value["settings"]["sr"] = json!(true);
        assert!(import(&value.to_string()).is_err());
        assert!(import(&fixture().to_string().replacen(
            "\"schemaVersion\":2",
            "\"schemaVersion\":2,\"schemaVersion\":1",
            1
        ))
        .is_err());
        assert!(import("{\"settings\":{\"intensity\":NaN}}").is_err());
    }
    #[test]
    fn rejects_oversized_text_empty_names_and_malformed_hashes() {
        assert!(import(&" ".repeat(MAX_JSON_BYTES + 1)).is_err());
        for invalid in ["", "  ", "a\nb"] {
            let mut value = fixture();
            value["name"] = json!(invalid);
            assert!(import(&value.to_string()).is_err());
        }
        let mut value = fixture();
        value["environment"] = json!({"modelSha256":"not a hash"});
        assert!(import(&value.to_string()).is_err());
        value["environment"] = json!({"modelSha256":"A".repeat(64)});
        assert_eq!(
            import(&value.to_string())
                .unwrap()
                .preset
                .environment
                .model_sha256,
            Some("a".repeat(64))
        );
    }
    #[test]
    fn config_library_roundtrips_without_changing_active_settings() {
        let mut settings = crate::config::OtherSetting::default();
        let active = settings.streamline_advanced;
        settings
            .streamline_nr_presets
            .push(import(&fixture().to_string()).unwrap().preset);
        let encoded = serde_json::to_string(&settings).unwrap();
        let decoded: crate::config::OtherSetting = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, settings);
        assert_eq!(decoded.streamline_advanced, active);
        assert!(!decoded.streamline_nr);
        let oversized = json!({"streamline_nr_presets":vec![fixture(); MAX_PRESETS + 1]});
        assert!(serde_json::from_value::<crate::config::OtherSetting>(oversized).is_err());
        let legacy: crate::config::OtherSetting = serde_json::from_str("{}").unwrap();
        assert!(legacy.streamline_nr_presets.is_empty());
    }
}
