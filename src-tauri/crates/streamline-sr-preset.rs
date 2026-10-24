//! Presets supported by the bundled Streamline headers. Deprecated presets are omitted.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamlineSrPreset {
    Default,
    #[default]
    J,
    K,
    L,
    M,
}
impl StreamlineSrPreset {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::J => "j",
            Self::K => "k",
            Self::L => "l",
            Self::M => "m",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "default" => Some(Self::Default),
            "j" => Some(Self::J),
            "k" => Some(Self::K),
            "l" => Some(Self::L),
            "m" => Some(Self::M),
            _ => None,
        }
    }
    #[allow(dead_code)] // Only the injected layer passes numeric values to the SDK.
    pub fn sdk_value(self) -> u32 {
        match self {
            Self::Default => 0,
            Self::J => 10,
            Self::K => 11,
            Self::L => 12,
            Self::M => 13,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presets_roundtrip_and_reject_unsupported_values() {
        for (name, id) in [("default", 0), ("j", 10), ("k", 11), ("l", 12), ("m", 13)] {
            let preset = StreamlineSrPreset::parse(name).unwrap();
            assert_eq!(preset.as_str(), name);
            assert_eq!(preset.sdk_value(), id);
            assert_eq!(
                serde_json::from_str::<StreamlineSrPreset>(&format!("\"{name}\"")).unwrap(),
                preset
            );
        }
        assert!(StreamlineSrPreset::parse("e").is_none());
        assert!(serde_json::from_str::<StreamlineSrPreset>("\"invalid\"").is_err());
    }
}
