//! Local UI eligibility. The emulator's actual adapter/runtime remains authoritative.
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Adapter {
    name: String,
    vendor_id: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub has_nvidia: bool,
    pub sr_supported: bool,
    pub nr_supported: bool,
    pub fg_max_multiplier: u8,
    pub adapters: Vec<Adapter>,
}

impl Capabilities {
    pub fn preflight_detail(&self) -> String {
        let mut names = Vec::new();
        for adapter in self.adapters.iter().filter(|a| a.vendor_id == 0x10de) {
            let name = adapter
                .name
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if !name.is_empty() && !names.contains(&name) {
                names.push(name);
            }
        }
        let available = |supported| {
            if supported {
                "可选"
            } else {
                "不可选／型号未识别"
            }
        };
        let fg = match self.fg_max_multiplier {
            0 => "不可选／型号未识别".into(),
            2 => "最高 2× 固定倍数".into(),
            maximum => format!("最高 {maximum}×，可选动态模式"),
        };
        format!(
            "检测到 {}。型号初筛：SR / DLAA {}；NR（DLSS 5）{}；FG {}。驱动与模拟器实际选用设备的运行能力，以专用启动后的游戏运行状态为准。",
            if names.is_empty() { "NVIDIA 显卡（型号未知）".into() } else { names.join("、") },
            available(self.sr_supported), available(self.nr_supported), fg,
        )
    }
}

fn classify(adapters: Vec<Adapter>) -> Capabilities {
    // Require GeForce: RTX 4000/5000 professional names are not consumer series.
    let geforce = regex::Regex::new(r"(?i)\bGeForce\s+RTX\s+(20|30|40|50)\d{2}\b").unwrap();
    let mut result = Capabilities {
        has_nvidia: false,
        sr_supported: false,
        nr_supported: false,
        fg_max_multiplier: 0,
        adapters,
    };
    for adapter in &result.adapters {
        if adapter.vendor_id != 0x10de {
            continue;
        }
        result.has_nvidia = true;
        let Some(series) = geforce.captures(&adapter.name) else {
            continue;
        };
        result.sr_supported = true;
        match &series[1] {
            "50" => {
                result.nr_supported = true;
                result.fg_max_multiplier = 6;
            }
            "40" => result.fg_max_multiplier = result.fg_max_multiplier.max(2),
            _ => {}
        }
    }
    result
}

#[cfg(windows)]
pub fn detect() -> Result<Capabilities, String> {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_ERROR_NOT_FOUND,
    };
    unsafe {
        let factory: IDXGIFactory1 =
            CreateDXGIFactory1().map_err(|e| format!("显卡检测失败：{e}"))?;
        let mut adapters = Vec::new();
        let mut index = 0;
        loop {
            let adapter = match factory.EnumAdapters1(index) {
                Ok(adapter) => adapter,
                Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(e) => return Err(format!("显卡枚举失败：{e}")),
            };
            index += 1;
            let desc = adapter
                .GetDesc1()
                .map_err(|e| format!("显卡信息读取失败：{e}"))?;
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            let end = desc
                .Description
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(desc.Description.len());
            adapters.push(Adapter {
                name: String::from_utf16_lossy(&desc.Description[..end]),
                vendor_id: desc.VendorId,
            });
        }
        Ok(classify(adapters))
    }
}

#[cfg(not(windows))]
pub fn detect() -> Result<Capabilities, String> {
    // Keep classification shared and fail closed on unsupported platforms.
    let _ = classify(Vec::new());
    Err("图形增强显卡检测仅支持 Windows".into())
}
