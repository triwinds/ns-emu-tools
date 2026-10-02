//! Per-present selection for the frozen emulator profile. No retained GPU handles.
use crate::source_model::Model;
use ash::vk::{self, Handle};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex, OnceLock,
};
static DISABLED: AtomicBool = AtomicBool::new(false);
static REQUESTED: OnceLock<AtomicBool> = OnceLock::new();
fn requested() -> &'static AtomicBool {
    REQUESTED.get_or_init(|| {
        AtomicBool::new(
            track_only()
                || fg_tracking()
                || std::env::var("NS_STREAMLINE_TARGET_SR_MODE").is_ok_and(|m| m != "off"),
        )
    })
}
fn fg_tracking() -> bool {
    // FG needs the presentation viewport even when SR starts off.
    std::env::var("NS_STREAMLINE_TARGET_FG").as_deref() == Ok("1")
        || (cfg!(feature = "native-nr")
            && std::env::var("NS_STREAMLINE_NATIVE_NR").as_deref() == Ok("1"))
}
fn track_only() -> bool {
    static VALUE: OnceLock<bool> = OnceLock::new();
    *VALUE.get_or_init(|| {
        measuring() && std::env::var("NS_STREAMLINE_SOURCE_TRACK_ONLY").as_deref() == Ok("1")
    })
}
#[allow(dead_code)]
pub(crate) fn set_requested(on: bool) {
    let on = on || track_only();
    if requested().swap(on, Ordering::AcqRel) != on {
        let mut t = TRACKER.get_or_init(Default::default).lock().unwrap();
        t.model.clear_frame_state();
    }
}
pub(crate) fn wants_call(name: &str) -> bool {
    if !enabled() {
        return true;
    }
    // Resource lifetimes remain tracked so live SR/FG can start safely. Per-frame
    // state is rebuilt from fresh commands/descriptors after the next enable.
    if !requested().load(Ordering::Acquire) {
        return !(name.starts_with("vkCmd")
            || name.starts_with("vkUpdateDescriptor")
            || matches!(
                name,
                "vkQueueSubmit"
                    | "vkBeginCommandBuffer"
                    | "vkEndCommandBuffer"
                    | "vkResetCommandBuffer"
            ));
    }
    !matches!(
        name,
        "vkCmdPushConstants" | "vkCmdCopyImage" | "vkCmdBlitImage"
    )
}
pub(crate) fn measuring() -> bool {
    static VALUE: OnceLock<bool> = OnceLock::new();
    *VALUE.get_or_init(|| std::env::var("NS_STREAMLINE_SOURCE_MEASURE").as_deref() == Ok("1"))
}
static OBSERVER_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static OBSERVER_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static CATEGORIES: [std::sync::atomic::AtomicU64; 5] =
    [const { std::sync::atomic::AtomicU64::new(0) }; 5];
pub(crate) fn measured(name: &str, start: Option<std::time::Instant>) {
    if let Some(start) = start {
        let ns = start.elapsed().as_nanos() as u64;
        OBSERVER_NS.fetch_add(ns, Ordering::Relaxed);
        let category = if name.contains("Descriptor") {
            0
        } else if name.contains("Barrier") {
            1
        } else if name.contains("Draw") {
            2
        } else if name.contains("RenderPass") {
            3
        } else {
            4
        };
        CATEGORIES[category].fetch_add(ns, Ordering::Relaxed);
        OBSERVER_CALLS.fetch_add(1, Ordering::Relaxed);
    }
}
#[allow(dead_code)]
pub(crate) fn measurement() -> Value {
    json!({"observer_ns":OBSERVER_NS.load(Ordering::Relaxed),"observer_calls":OBSERVER_CALLS.load(Ordering::Relaxed),"category_ns":CATEGORIES.each_ref().map(|n|n.load(Ordering::Relaxed)),"stopped":DISABLED.load(Ordering::Relaxed)})
}
#[allow(dead_code)]
pub(crate) fn stop_measurement_tracking() {
    if measuring() {
        DISABLED.store(true, Ordering::Relaxed);
        let mut t = TRACKER.get_or_init(Default::default).lock().unwrap();
        t.disabled = true;
        t.model = Model::default();
    }
}
pub(crate) fn collecting() -> bool {
    enabled() && !DISABLED.load(Ordering::Relaxed)
}

pub(crate) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        cfg!(all(windows, feature = "sdk-bridge"))
            && std::env::var("NS_STREAMLINE_SOURCE_BENCH_OFF").as_deref() != Ok("1")
            && std::env::var("NS_STREAMLINE_SOURCE_AUTO").as_deref() == Ok("1")
            && std::env::var("NS_STREAMLINE_TARGET_SDK").as_deref() == Ok("1")
            && crate::trace::authorized()
    })
}
#[derive(Default)]
struct Tracker {
    model: Model,
    disabled: bool,
    events: u64,
}
static TRACKER: OnceLock<Mutex<Tracker>> = OnceLock::new();
pub(crate) fn update(action: impl FnOnce(&mut Model)) {
    let mut t = TRACKER.get_or_init(Default::default).lock().unwrap();
    if t.disabled {
        return;
    }
    t.model.serial = t.model.serial.wrapping_add(1);
    action(&mut t.model);
    t.events += 1;
    if t.model.invalid || (t.events % 512 == 0 && !t.model.within_budget()) {
        t.disabled = true;
        DISABLED.store(true, Ordering::Relaxed);
        t.model = Model::default();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Source {
    pub image: vk::Image,
    /// Allocation generation checked against the submitted presentation draw.
    pub generation: u64,
    /// Presentation stream epoch, stable only for already observed live images
    /// with the same validated blit path and color/mapping contract.
    pub history_identity: u64,
    pub history_members: u32,
    pub history_paths: u32,
    pub history_update: &'static str,
    pub history_route: [u64; 4],
    pub usage: u32,
    pub extent: vk::Extent2D,
    pub raw_copy: bool,
    pub offsets: [vk::Offset3D; 2],
    pub viewport: [f32; 4],
}
pub(crate) fn retire_swapchain(device: u64, chain: u64) {
    if collecting() {
        update(|m| m.retire_swapchain(device, chain));
    }
}
#[allow(dead_code)] // Only the SDK build uses the selection; transparent builds share observers.
pub(crate) unsafe fn select(
    queue: vk::Queue,
    info: &vk::PresentInfoKHR,
) -> Result<Source, &'static str> {
    if !enabled() {
        return Err("当前构建使用窗口画面");
    }
    if !requested().load(Ordering::Acquire) {
        return Err("SR/FG 未请求画面区域，已暂停逐帧识别");
    }
    if info.swapchain_count != 1
        || !info.p_next.is_null()
        || info.p_swapchains.is_null()
        || info.p_image_indices.is_null()
        || info.wait_semaphore_count == 0
        || info.p_wait_semaphores.is_null()
    {
        return Err("呈现结构不匹配");
    }
    let device = crate::device(queue).ok_or("缺少设备")?.handle.as_raw();
    let mut t = TRACKER.get_or_init(Default::default).lock().unwrap();
    if t.disabled {
        return Err("识别记录不完整或已达上限");
    }
    t.model.select(
        device,
        queue.as_raw(),
        (*info.p_swapchains).as_raw(),
        *info.p_image_indices,
        std::slice::from_raw_parts(info.p_wait_semaphores, info.wait_semaphore_count as usize),
    )
}
