//! Integer UI settings shared by the toolbox, launcher and Vulkan layer.
//! Missing NR tone values retain the legacy intensity-linked behavior.
use serde::{Deserialize, Deserializer, Serialize};

pub const CAPABILITY_MARKER: &[u8] = b"NS_EMU_GRAPHICS_ADVANCED_V1";
pub const NR_LOOK_MARKER: &[u8] = b"NS_EMU_NR_LOOK_V1";
pub const NR_SPATIAL_LOOK_MARKER: &[u8] = b"NS_EMU_NR_SPATIAL_LOOK_V1";
pub const NR_TWO_PASS_MARKER: &[u8] = b"NS_EMU_NR_TWO_PASS_V1";
pub const NR_TEMPORAL_LOOK_MARKER: &[u8] = b"NS_EMU_NR_TEMPORAL_LOOK_V1";
pub const NR_LOOK_SCOPE_MARKER: &[u8] = b"NS_EMU_NR_LOOK_SCOPE_V1";
pub const NR_TEMPORAL_MODES_MARKER: &[u8] = b"NS_EMU_NR_TEMPORAL_MODES_V1";
pub const NR_PERSISTENCE_MARKER: &[u8] = b"NS_EMU_NR_PERSISTENCE_V1";
pub const NR_LOOK_EXPERIMENTS_MARKER: &[u8] = b"NS_EMU_NR_LOOK_EXPERIMENTS_V1";
pub const NR_CONSOLIDATED_MARKER: &[u8] = b"NS_EMU_NR_CONSOLIDATED_V1";
pub const NR_INFERENCE_SCALE_MARKER: &[u8] = b"NS_EMU_NR_INFERENCE_SCALE_V1";
pub const NR_INFERENCE_CAP_MARKER: &[u8] = b"NS_EMU_NR_INFERENCE_CAP_V1";
pub const INPUT_SCALING_MARKER: &[u8] = b"NS_EMU_INPUT_SCALING_V1";
pub const INPUT_SCALING_LIVE_MARKER: &[u8] = b"NS_EMU_INPUT_SCALING_LIVE_V1";

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputSizing {
    pub scale_percent: u32,
    pub max_edge: u32,
}
impl Default for InputSizing {
    fn default() -> Self {
        Self {
            scale_percent: 100,
            max_edge: 0,
        }
    }
}
impl InputSizing {
    pub fn valid(self) -> bool {
        (50..=100).contains(&self.scale_percent) && valid_nr_inference_max_edge(self.max_edge)
    }
}

/// Zero selects the legacy percentage mode; otherwise cap the longest edge.
pub fn valid_nr_inference_max_edge(value: u32) -> bool {
    value == 0 || (320..=8192).contains(&value)
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ColorAlgorithm {
    #[default]
    LogDelta,
    Oklab,
}
impl ColorAlgorithm {
    pub fn is_default(&self) -> bool {
        *self == Self::LogDelta
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ColorProtection {
    #[serde(deserialize_with = "protection_strength")]
    pub hue: u16,
    #[serde(deserialize_with = "protection_strength")]
    pub shadows: u16,
    #[serde(deserialize_with = "protection_strength")]
    pub highlights: u16,
    #[serde(deserialize_with = "protection_strength")]
    pub overcorrection: u16,
}
impl ColorProtection {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}
fn protection_strength<'de, D: Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    bounded(d, 0, 100, false)
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LookDiagnostic {
    #[default]
    Off,
    Original,
    FirstPass,
    ModelOutput,
    RawDelta,
    ControlledDelta,
    LowFrequency,
    HighFrequency,
    Protection,
    HistoryValidity,
    HistoryWeight,
}
impl LookDiagnostic {
    pub fn is_default(&self) -> bool {
        *self == Self::Off
    }
    pub fn code(self) -> u16 {
        match self {
            Self::Off => 0,
            Self::Original => 1,
            Self::FirstPass => 2,
            Self::ModelOutput => 3,
            Self::RawDelta => 4,
            Self::ControlledDelta => 5,
            Self::LowFrequency => 6,
            Self::HighFrequency => 7,
            Self::Protection => 8,
            Self::HistoryValidity => 9,
            Self::HistoryWeight => 10,
        }
    }
    pub fn needs_band(self) -> bool {
        matches!(self, Self::LowFrequency | Self::HighFrequency)
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LookScope {
    #[default]
    FinalPass,
    ChainTotal,
}
impl LookScope {
    pub fn is_default(&self) -> bool {
        *self == Self::FinalPass
    }
    pub fn uses_original(self, successful_passes: u8) -> bool {
        self == Self::ChainTotal || successful_passes != 2
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct AdvancedSettings {
    pub nr: NrOptions,
    pub sr: SrOptions,
    pub fg: FgOptions,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct AdvancedUpdate {
    pub nr: Option<NrOptions>,
    pub sr: Option<SrOptions>,
    pub fg: Option<FgOptions>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NrStyle {
    #[default]
    A,
    B,
    C,
}
impl NrStyle {
    pub fn sdk_value(self) -> i32 {
        match self {
            Self::A => 0,
            Self::B => 1,
            Self::C => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct NrOptions {
    #[serde(skip_serializing_if = "SecondPassOptions::is_default")]
    pub second_pass: SecondPassOptions,
    #[serde(skip_serializing_if = "LookOptions::is_default")]
    pub look: LookOptions,
    pub style: NrStyle,
    #[serde(deserialize_with = "optional_strength")]
    pub global_tone: Option<u16>,
    #[serde(deserialize_with = "optional_strength")]
    pub local_tone: Option<u16>,
    #[serde(deserialize_with = "optional_strength")]
    pub local_structure: Option<u16>,
    #[serde(deserialize_with = "strength")]
    pub skin_structure: u16,
    pub auto_mask: bool,
}
impl NrOptions {
    /// External Look controls never change the model's parameter/history contract.
    pub fn model_only(mut self) -> Self {
        self.look = LookOptions::default();
        self.second_pass = SecondPassOptions::default();
        self
    }
    pub fn strengths(self, intensity: f32) -> [f32; 4] {
        let value = |v: Option<u16>| v.map_or(intensity, |v| f32::from(v) / 100.0);
        [
            value(self.global_tone),
            value(self.local_tone),
            value(self.local_structure),
            f32::from(self.skin_structure) / 100.0,
        ]
    }
}

/// Two passes are opt-in. Stored overrides are used only with inherit=false.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct SecondPassOptions {
    pub enabled: bool,
    pub inherit: bool,
    /// Explicit recreation request; normal tuning never retries a failed pass.
    pub retry: u16,
    #[serde(deserialize_with = "strength")]
    pub intensity: u16,
    pub style: NrStyle,
    #[serde(deserialize_with = "optional_strength")]
    pub global_tone: Option<u16>,
    #[serde(deserialize_with = "optional_strength")]
    pub local_tone: Option<u16>,
    #[serde(deserialize_with = "optional_strength")]
    pub local_structure: Option<u16>,
    #[serde(deserialize_with = "strength")]
    pub skin_structure: u16,
    pub auto_mask: bool,
}
impl Default for SecondPassOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            inherit: true,
            retry: 0,
            intensity: 100,
            style: NrStyle::A,
            global_tone: None,
            local_tone: None,
            local_structure: None,
            skin_structure: 0,
            auto_mask: false,
        }
    }
}
impl SecondPassOptions {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub fn resolve(self, intensity: f32, first: NrOptions) -> (f32, NrOptions) {
        if self.inherit {
            return (intensity, first.model_only());
        }
        (
            f32::from(self.intensity) / 100.0,
            NrOptions {
                style: self.style,
                global_tone: self.global_tone,
                local_tone: self.local_tone,
                local_structure: self.local_structure,
                skin_structure: self.skin_structure,
                auto_mask: self.auto_mask,
                ..NrOptions::default()
            },
        )
    }
    pub fn execution_eq(self, other: Self, intensity: f32, first: NrOptions) -> bool {
        self.enabled == other.enabled
            && (!self.enabled
                || (self.retry == other.retry
                    && self.resolve(intensity, first) == other.resolve(intensity, first)))
    }
}

/// P2 SDR Look, applied after NR. Gains are percentages; caps are hundredths
/// of a stop (0 disables the cap). Integer settings exclude nonfinite imports.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct LookOptions {
    #[serde(skip_serializing_if = "ColorAlgorithm::is_default")]
    pub algorithm: ColorAlgorithm,
    #[serde(skip_serializing_if = "ColorProtection::is_default")]
    pub protection: ColorProtection,
    #[serde(skip_serializing_if = "LookDiagnostic::is_default")]
    pub diagnostic: LookDiagnostic,
    #[serde(skip_serializing_if = "LookScope::is_default")]
    pub scope: LookScope,
    #[serde(skip_serializing_if = "TemporalLook::is_default")]
    pub temporal: TemporalLook,
    #[serde(skip_serializing_if = "SpatialLook::is_default")]
    pub spatial: SpatialLook,
    pub enabled: bool,
    #[serde(deserialize_with = "look_schema")]
    pub schema_version: u16,
    #[serde(deserialize_with = "strength")]
    pub amount: u16,
    #[serde(deserialize_with = "strength")]
    pub brighten: u16,
    #[serde(deserialize_with = "strength")]
    pub darken: u16,
    #[serde(deserialize_with = "look_cap")]
    pub brighten_cap: u16,
    #[serde(deserialize_with = "look_cap")]
    pub darken_cap: u16,
    #[serde(deserialize_with = "strength")]
    pub color: u16,
    #[serde(deserialize_with = "strength")]
    pub hue: u16,
    #[serde(deserialize_with = "strength")]
    pub shadows: u16,
    #[serde(deserialize_with = "strength")]
    pub midtones: u16,
    #[serde(deserialize_with = "strength")]
    pub highlights: u16,
}
impl Default for LookOptions {
    fn default() -> Self {
        Self {
            algorithm: ColorAlgorithm::default(),
            protection: ColorProtection::default(),
            diagnostic: LookDiagnostic::default(),
            scope: LookScope::default(),
            spatial: SpatialLook::default(),
            temporal: TemporalLook::default(),
            enabled: true,
            schema_version: 1,
            amount: 100,
            brighten: 100,
            darken: 100,
            brighten_cap: 0,
            darken_cap: 0,
            color: 100,
            hue: 100,
            shadows: 100,
            midtones: 100,
            highlights: 100,
        }
    }
}
impl LookOptions {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub fn bypass(self) -> bool {
        !self.enabled
            || (self.basic_neutral()
                && self.spatial.bypass()
                && !self.temporal_active()
                && self.diagnostic.is_default())
    }
    pub fn basic_neutral(mut self) -> bool {
        self.algorithm = ColorAlgorithm::default();
        self.diagnostic = LookDiagnostic::default();
        self.scope = LookScope::default();
        self.spatial = SpatialLook::default();
        self.temporal = TemporalLook::default();
        self.is_default()
    }
    pub fn spatial_active(self) -> bool {
        self.enabled && self.amount != 0 && !self.spatial.bypass()
    }
    pub fn band_required(self) -> bool {
        self.spatial_active() || (self.enabled && self.diagnostic.needs_band())
    }
    pub fn needs_experiments(self) -> bool {
        !self.algorithm.is_default()
            || !self.protection.is_default()
            || !self.diagnostic.is_default()
    }
    pub fn temporal_active(self) -> bool {
        self.enabled && self.amount != 0 && self.temporal.enabled && self.temporal.strength != 0
    }
    /// Matches the shader push-constant ABI; no exposure/tone-mapping step.
    pub fn constants(self) -> [f32; 24] {
        let mut values = [
            self.amount,
            self.brighten,
            self.darken,
            self.brighten_cap,
            self.darken_cap,
            self.color,
            self.hue,
            self.shadows,
            self.midtones,
            self.highlights,
            if self.spatial_active() {
                100
            } else if self.band_required() {
                200
            } else {
                0
            },
            0,
            self.spatial.lighting,
            self.spatial.detail,
            0,
            self.spatial.halo,
            self.protection.hue,
            self.protection.shadows,
            self.protection.highlights,
            self.protection.overcorrection,
            if self.algorithm == ColorAlgorithm::Oklab {
                100
            } else {
                0
            },
            self.diagnostic.code() * 100,
            0,
            0,
        ]
        .map(|v| f32::from(v) / 100.0);
        values[14] = f32::from(self.spatial.radius);
        values
    }
}
/// Raw log-delta history. Motion is required; defaults allocate no resources.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct TemporalLook {
    #[serde(skip_serializing_if = "TemporalMode::is_default")]
    pub mode: TemporalMode,
    #[serde(skip_serializing_if = "HistorySampling::is_default")]
    pub sampling: HistorySampling,
    pub enabled: bool,
    #[serde(deserialize_with = "temporal_time")]
    pub time_ms: u16,
    #[serde(deserialize_with = "temporal_strength")]
    pub strength: u16,
    #[serde(deserialize_with = "temporal_rejection")]
    pub rejection: u16,
}
impl Default for TemporalLook {
    fn default() -> Self {
        Self {
            mode: TemporalMode::default(),
            sampling: HistorySampling::default(),
            enabled: false,
            time_ms: 80,
            strength: 75,
            rejection: 50,
        }
    }
}
impl TemporalLook {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub fn needs_extended_support(self) -> bool {
        !self.mode.is_default() || !self.sampling.is_default()
    }
}
/// `enabled` retains the legacy on/off contract; missing mode keeps NVOF.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TemporalMode {
    Static,
    #[default]
    OpticalFlow,
    OpticalFlowPlus,
}
impl TemporalMode {
    pub fn persistent(self) -> bool {
        self == Self::OpticalFlowPlus
    }
    pub fn is_default(&self) -> bool {
        *self == Self::OpticalFlow
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HistorySampling {
    #[default]
    Bilinear,
    PerTap,
}
impl HistorySampling {
    pub fn is_default(&self) -> bool {
        *self == Self::Bilinear
    }
}
fn temporal_time<'de, D: Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    bounded(d, 1, 500, false)
}
fn temporal_strength<'de, D: Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    bounded(d, 0, 90, false)
}
fn temporal_rejection<'de, D: Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    bounded(d, 10, 400, false)
}
/// Spatial processing remains default-off; radius is in NR working pixels.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct SpatialLook {
    pub enabled: bool,
    #[serde(deserialize_with = "strength")]
    pub lighting: u16,
    #[serde(deserialize_with = "strength")]
    pub detail: u16,
    #[serde(deserialize_with = "spatial_radius")]
    pub radius: u16,
    #[serde(deserialize_with = "spatial_halo")]
    pub halo: u16,
}
impl Default for SpatialLook {
    fn default() -> Self {
        Self {
            enabled: false,
            lighting: 100,
            detail: 100,
            radius: 8,
            halo: 0,
        }
    }
}
impl SpatialLook {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub fn bypass(self) -> bool {
        !self.enabled || (self.lighting == 100 && self.detail == 100 && self.halo == 0)
    }
}
fn spatial_radius<'de, D: Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    bounded(d, 1, 32, false)
}
fn spatial_halo<'de, D: Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    bounded(d, 0, 100, false)
}
fn look_schema<'de, D: Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    bounded(d, 1, 1, false)
}
fn look_cap<'de, D: Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    bounded(d, 0, 1600, false)
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct SrOptions {
    pub auto_exposure: bool,
    #[serde(deserialize_with = "exposure")]
    pub exposure: u16,
}
impl Default for SrOptions {
    fn default() -> Self {
        Self {
            auto_exposure: true,
            exposure: 100,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FgMode {
    #[default]
    Fixed,
    Dynamic,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReflexMode {
    #[default]
    LowLatency,
    Boost,
}
impl ReflexMode {
    pub fn sdk_value(self) -> u32 {
        match self {
            Self::LowLatency => 1,
            Self::Boost => 2,
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct FgOptions {
    pub mode: FgMode,
    #[serde(deserialize_with = "multiplier")]
    pub multiplier: u8,
    #[serde(deserialize_with = "target_fps")]
    pub target_fps: u16,
    pub reflex: ReflexMode,
    #[serde(deserialize_with = "input_fps")]
    pub input_fps: u16,
}
impl Default for FgOptions {
    fn default() -> Self {
        Self {
            mode: FgMode::Fixed,
            multiplier: 2,
            target_fps: 0,
            reflex: ReflexMode::LowLatency,
            input_fps: 0,
        }
    }
}
impl FgOptions {
    pub fn frame_limit_us(self) -> u32 {
        if self.input_fps == 0 {
            0
        } else {
            1_000_000_u32.div_ceil(u32::from(self.input_fps))
        }
    }
    /// The SDK reports generated frames; the UI includes the original frame.
    pub fn supported(self, maximum_generated: u32) -> bool {
        u32::from(self.multiplier - 1) <= maximum_generated
    }
}

fn bounded<'de, D: Deserializer<'de>>(
    d: D,
    min: u16,
    max: u16,
    zero: bool,
) -> Result<u16, D::Error> {
    let value = u16::deserialize(d)?;
    if (min..=max).contains(&value) || (zero && value == 0) {
        Ok(value)
    } else {
        Err(serde::de::Error::custom(format!(
            "value must be {min}..{max}{}",
            if zero { " or 0" } else { "" }
        )))
    }
}
fn strength<'de, D: Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    bounded(d, 0, 200, false)
}
fn optional_strength<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u16>, D::Error> {
    let value = Option::<u16>::deserialize(d)?;
    if value.is_some_and(|v| v > 200) {
        Err(serde::de::Error::custom("NR strength must be 0..200"))
    } else {
        Ok(value)
    }
}
fn exposure<'de, D: Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    bounded(d, 25, 400, false)
}
fn multiplier<'de, D: Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
    bounded(d, 2, 6, false).map(|v| v as u8)
}
fn target_fps<'de, D: Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    bounded(d, 30, 360, true)
}
fn input_fps<'de, D: Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    bounded(d, 15, 240, true)
}

#[cfg(test)]
mod tests {
    #[test]
    fn color_experiments_preserve_legacy_neutral_and_validate_protection_bounds() {
        let legacy: super::LookOptions = serde_json::from_str(r#"{"amount":100}"#).unwrap();
        assert!(legacy.bypass() && !legacy.needs_experiments());
        let lab: super::LookOptions = serde_json::from_str(r#"{"algorithm":"oklab"}"#).unwrap();
        assert!(lab.bypass() && lab.needs_experiments());
        for invalid in [
            r#"{"protection":{"hue":101}}"#,
            r#"{"protection":{"shadows":-1}}"#,
            r#"{"algorithm":"hdr"}"#,
            r#"{"diagnostic":"unknown"}"#,
        ] {
            assert!(serde_json::from_str::<super::LookOptions>(invalid).is_err());
        }
        let diagnostic: super::LookOptions =
            serde_json::from_str(r#"{"diagnostic":"low_frequency"}"#).unwrap();
        assert!(!diagnostic.bypass() && diagnostic.band_required() && !diagnostic.spatial_active());
        assert_eq!(&lab.constants()[20..22], &[1.0, 0.0]);
    }
    use super::*;
    use serde_json::json;
    #[test]
    fn scope_and_temporal_extensions_preserve_legacy_semantics() {
        let legacy: LookOptions = serde_json::from_value(
            json!({"temporal":{"enabled":true,"timeMs":123,"strength":80,"rejection":70}}),
        )
        .unwrap();
        assert_eq!(legacy.scope, LookScope::FinalPass);
        assert_eq!(legacy.temporal.mode, TemporalMode::OpticalFlow);
        assert_eq!(legacy.temporal.sampling, HistorySampling::Bilinear);
        assert_eq!(legacy.temporal.time_ms, 123);
        let wire = serde_json::to_value(legacy).unwrap();
        assert!(wire.get("scope").is_none());
        assert!(wire["temporal"].get("mode").is_none());
        assert!(wire["temporal"].get("sampling").is_none());
        for scope in [LookScope::FinalPass, LookScope::ChainTotal] {
            for passes in [1, 2] {
                assert_eq!(
                    scope.uses_original(passes),
                    scope == LookScope::ChainTotal || passes == 1
                );
            }
            let options = LookOptions {
                scope,
                ..Default::default()
            };
            assert!(
                options.bypass(),
                "neutral scope must not dispatch or convert colors"
            );
            assert!(!LookOptions {
                amount: 0,
                ..options
            }
            .bypass());
            assert_eq!(
                NrOptions {
                    look: options,
                    ..Default::default()
                }
                .model_only(),
                NrOptions::default()
            );
        }
        let extended: LookOptions = serde_json::from_value(json!({"scope":"chain_total","temporal":{"enabled":true,"mode":"static","sampling":"per_tap"}})).unwrap();
        assert!(extended.temporal.needs_extended_support());
        assert_eq!(
            serde_json::from_value::<LookOptions>(serde_json::to_value(extended).unwrap()).unwrap(),
            extended
        );
        for invalid in [
            json!({"scope":"total"}),
            json!({"temporal":{"mode":"off"}}),
            json!({"temporal":{"sampling":"nearest"}}),
        ] {
            assert!(serde_json::from_value::<LookOptions>(invalid).is_err());
        }
    }
    #[test]
    fn temporal_defaults_ranges_and_model_history_contract() {
        let mut options = LookOptions::default();
        assert!(serde_json::to_value(options)
            .unwrap()
            .get("temporal")
            .is_none());
        assert!(options.bypass());
        options.temporal.enabled = true;
        assert!(!options.bypass() && options.basic_neutral());
        let nr = NrOptions {
            look: options,
            ..Default::default()
        };
        assert_eq!(nr.model_only(), NrOptions::default());
        assert_eq!(
            serde_json::from_value::<LookOptions>(serde_json::to_value(options).unwrap()).unwrap(),
            options
        );
        for invalid in [
            json!({"timeMs":0}),
            json!({"timeMs":501}),
            json!({"strength":91}),
            json!({"strength":1.5}),
            json!({"rejection":9}),
            json!({"rejection":401}),
            json!({"motion":"none"}),
        ] {
            assert!(serde_json::from_value::<LookOptions>(json!({"temporal":invalid})).is_err());
        }
        options.temporal.strength = 0;
        assert!(options.bypass());
    }
    #[test]
    fn second_pass_defaults_inheritance_overrides_and_wire_compatibility() {
        let mut first: NrOptions =
            serde_json::from_value(json!({"style":"b","localTone":150})).unwrap();
        assert!(serde_json::to_value(first)
            .unwrap()
            .get("secondPass")
            .is_none());
        assert!(!first.second_pass.enabled && first.second_pass.inherit);
        first.look.brighten = 50;
        first.second_pass.enabled = true;
        assert_eq!(
            first.second_pass.resolve(0.4, first),
            (0.4, first.model_only())
        );
        first.second_pass.inherit = false;
        first.second_pass.intensity = 25;
        first.second_pass.style = NrStyle::C;
        first.second_pass.local_structure = Some(200);
        let (intensity, second) = first.second_pass.resolve(0.4, first);
        assert_eq!(intensity, 0.25);
        assert_eq!(second.style, NrStyle::C);
        assert_eq!(second.strengths(intensity), [0.25, 0.25, 2.0, 0.0]);
        assert!(second.look.bypass() && second.second_pass.is_default());
        assert_eq!(
            serde_json::from_value::<NrOptions>(serde_json::to_value(first).unwrap()).unwrap(),
            first
        );
        for invalid in [
            json!({"intensity":201}),
            json!({"style":"d"}),
            json!({"localTone":-1}),
            json!({"skinStructure":201}),
            json!({"retry":65536}),
            json!({"retry":1.5}),
            json!({"passes":3}),
        ] {
            assert!(serde_json::from_value::<NrOptions>(json!({"secondPass":invalid})).is_err());
        }
    }
    #[test]
    fn spatial_defaults_remain_compatible_and_imports_are_bounded() {
        let basic: LookOptions = serde_json::from_value(json!({"amount":50})).unwrap();
        assert!(serde_json::to_value(basic)
            .unwrap()
            .get("spatial")
            .is_none());
        assert_eq!(basic.spatial, SpatialLook::default());
        for value in [
            json!({"radius":0}),
            json!({"radius":33}),
            json!({"halo":101}),
            json!({"detail":201}),
            json!({"lighting":-1}),
            json!({"radius":1.5}),
            json!({"motion":true}),
        ] {
            assert!(serde_json::from_value::<LookOptions>(json!({"spatial":value})).is_err());
        }
        let neutral: LookOptions =
            serde_json::from_value(json!({"spatial":{"enabled":true,"radius":32}})).unwrap();
        assert!(neutral.bypass() && !neutral.spatial_active());
        let active: LookOptions = serde_json::from_value(
            json!({"spatial":{"enabled":true,"lighting":0,"radius":4,"halo":50}}),
        )
        .unwrap();
        assert!(!active.bypass() && active.spatial_active());
        assert_eq!(&active.constants()[10..16], &[1.0, 0.0, 0.0, 1.0, 4.0, 0.5]);
        assert_eq!(
            serde_json::from_value::<LookOptions>(serde_json::to_value(active).unwrap()).unwrap(),
            active
        );
        assert!(
            LookOptions {
                amount: 0,
                ..active
            }
            .spatial_active()
                == false
        );
        assert!(LookOptions {
            spatial: SpatialLook {
                lighting: 0,
                ..Default::default()
            },
            ..Default::default()
        }
        .bypass());
    }
    #[test]
    fn look_import_validates_schema_ranges_and_preserves_legacy_wire_format() {
        let legacy: NrOptions =
            serde_json::from_value(json!({"style":"b","localTone":150})).unwrap();
        assert!(legacy.look.bypass());
        assert!(serde_json::to_value(legacy).unwrap().get("look").is_none());
        for look in [
            json!({"schemaVersion":2}),
            json!({"amount":201}),
            json!({"color":-1}),
            json!({"hue":1.5}),
            json!({"brightenCap":1601}),
            json!({"shadows":null}),
            json!({"passes":2}),
        ] {
            assert!(serde_json::from_value::<NrOptions>(json!({"look":look})).is_err());
        }
        let options: NrOptions =
            serde_json::from_value(json!({"look":{"amount":0,"darkenCap":125}})).unwrap();
        assert_eq!(options.look.constants()[4], 1.25);
        assert!(!options.look.bypass());
        assert_eq!(
            serde_json::from_value::<NrOptions>(serde_json::to_value(options).unwrap()).unwrap(),
            options
        );
        assert_eq!(options.model_only(), NrOptions::default());
        assert!(LookOptions {
            enabled: false,
            amount: 0,
            ..Default::default()
        }
        .bypass());
    }
    #[test]
    fn legacy_defaults_preserve_intensity_link_and_fixed_two_times() {
        let settings: AdvancedSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings, AdvancedSettings::default());
        assert_eq!(settings.nr.strengths(0.4), [0.4, 0.4, 0.4, 0.0]);
        assert_eq!(settings.fg.multiplier, 2);
        assert_eq!(settings.fg.frame_limit_us(), 0);
        assert_eq!(settings.sr.exposure, 100);
    }
    #[test]
    fn rejects_invalid_sdk_settings_before_any_application() {
        for value in [
            json!({"nr":{"style":"d"}}),
            json!({"nr":{"localTone":201}}),
            json!({"nr":{"skinStructure":-1}}),
            json!({"sr":{"exposure":0}}),
            json!({"sr":{"exposure":401}}),
            json!({"fg":{"multiplier":1}}),
            json!({"fg":{"multiplier":7}}),
            json!({"fg":{"targetFps":29}}),
            json!({"fg":{"inputFps":241}}),
            json!({"fg":{"reflex":"off"}}),
        ] {
            assert!(serde_json::from_value::<AdvancedSettings>(value).is_err());
        }
        let value = json!({"nr":{"style":"c","localTone":200,"autoMask":true},"sr":{"autoExposure":false,"exposure":25},"fg":{"mode":"dynamic","multiplier":4,"targetFps":120,"reflex":"boost","inputFps":60}});
        let settings: AdvancedSettings = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(
            serde_json::from_value::<AdvancedSettings>(serde_json::to_value(settings).unwrap())
                .unwrap(),
            settings
        );
        assert_eq!(settings.nr.strengths(0.4), [0.4, 2.0, 0.4, 0.0]);
        assert_eq!(settings.fg.frame_limit_us(), 16667);
        assert!(!settings.fg.supported(1));
        assert!(settings.fg.supported(3));
        let maximum: FgOptions = serde_json::from_value(json!({"multiplier":6})).unwrap();
        assert!(!maximum.supported(4));
        assert!(maximum.supported(5));
    }
}
