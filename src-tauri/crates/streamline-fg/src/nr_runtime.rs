//! One pinned, serialized NR device session sharing NGX ownership with Streamline.
use crate::{
    nr_abi::*,
    nr_api::{self, Result},
    nr_package, trace,
};
use ash::vk::{self, Handle};
use serde_json::json;
use std::{
    ffi::{c_char, c_void, CStr},
    mem::ManuallyDrop,
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    sync::{Arc, Mutex, OnceLock},
};

pub(super) fn requested() -> bool {
    std::env::var("NS_STREAMLINE_NATIVE_NR").as_deref() == Ok("1")
}
static SESSION: OnceLock<Arc<Mutex<Session>>> = OnceLock::new();
static FAILURE: OnceLock<String> = OnceLock::new();
static CLOSED: AtomicBool = AtomicBool::new(false);
static FEATURE_ID: AtomicU64 = AtomicU64::new(1);
struct Session {
    api: nr_api::Api,
    device: vk::Device,
    capabilities: usize,
    features: usize,
    _common: Box<nr_api::CommonInfo>,
    _data: Vec<u16>,
}
// Every API/map operation is under this session's mutex. GPU recording is
// serialized by the swapchain owner and its completion fence. P0 verified
// serialized thread migration; this is not a general NGX Send/Sync claim.
unsafe impl Send for Session {}
unsafe extern "C" fn ngx_log(message: *const c_char, level: u32, feature: u32) {
    if !message.is_null() {
        trace::event!(
            "target_nr_ngx",
            json!({"message":CStr::from_ptr(message).to_string_lossy(),"level":level,"feature":feature})
        );
    }
}
pub(super) fn event(stage: &str, details: serde_json::Value) -> Result<()> {
    trace::event!(stage, details);
    Ok(())
}
fn checked(stage: &str, result: u32) -> Result<()> {
    // Per-frame success records are disabled in normal launches; failures must remain visible.
    if let Err(error) = nr_api::checked(result, stage) {
        trace::event!(
            "target_nr_api_failure",
            json!({"stage":stage,"result":result,"error":error.to_string()})
        );
        return Err(error);
    }
    event(
        stage,
        json!({"result":result,"result_hex":format!("0x{result:08x}")}),
    )?;
    Ok(())
}
pub(super) unsafe fn initialize(
    instance: vk::Instance,
    physical: vk::PhysicalDevice,
    device: vk::Device,
) -> Result<()> {
    if !requested() {
        return Ok(());
    }
    let result = (|| -> Result<()> {
        if SESSION.get().is_some() || CLOSED.load(Ordering::Acquire) {
            return Err("native NR supports one device lifecycle".into());
        }
        let runtime = PathBuf::from(
            std::env::var_os("NS_STREAMLINE_NR_RUNTIME").ok_or("NR runtime missing")?,
        );
        let bridge =
            PathBuf::from(std::env::var_os("NS_STREAMLINE_NR_BRIDGE").ok_or("NR bridge missing")?);
        let runtime_hash = nr_package::verify(&runtime, &nr_package::RUNTIMES)?;
        let bridge_hash = nr_package::verify(&bridge, &[nr_package::BRIDGE])?;
        let path = PathBuf::from(
            std::env::var_os("NS_STREAMLINE_LIVE_DIR").ok_or("NR session directory missing")?,
        );
        if !path.is_absolute() || !path.is_dir() {
            return Err("invalid NR data directory".into());
        }
        use std::os::windows::ffi::OsStrExt;
        let common = Box::new(nr_api::CommonInfo {
            paths: std::ptr::null(),
            path_count: 0,
            internal: std::ptr::null_mut(),
            logging: nr_api::LoggingInfo {
                callback: ngx_log,
                minimum_level: 2,
                disable_other_sinks: 1,
                padding: [0; 3],
            },
        });
        // After starting initialization retain all DLL/callback storage on error.
        // Streamline owns shared core cleanup; partial NR init never closes it.
        let mut session = ManuallyDrop::new(Session {
            api: nr_api::Api::load(&runtime)?,
            device,
            capabilities: 0,
            features: 0,
            _common: common,
            _data: path.as_os_str().encode_wide().chain(Some(0)).collect(),
        });
        checked(
            "target_nr_core_init",
            nr_api::NVSDK_NGX_VULKAN_Init_with_ProjectID(
                c"9b633af0-b7c8-4ebe-b3a1-40e6b9d34c1b".as_ptr(),
                0,
                c"0.1.0".as_ptr(),
                session._data.as_ptr(),
                instance.as_raw() as VkHandle,
                physical.as_raw() as VkHandle,
                device.as_raw() as VkHandle,
                crate::nr_layout::gipa as Address,
                crate::nr_layout::gdpa as Address,
                &*session._common,
                NGX_API_VERSION,
            ),
        )?;
        let mut capabilities = std::ptr::null_mut();
        checked(
            "target_nr_capability_parameters",
            nr_api::NVSDK_NGX_VULKAN_GetCapabilityParameters(&mut capabilities),
        )?;
        if capabilities.is_null() {
            return Err("NR capability map is null".into());
        }
        session.capabilities = capabilities as usize;
        session.api.load_bridge(&bridge)?;
        let args = InitArgs {
            application_id: 0x1122334455667788,
            data_path: session._data.as_ptr(),
            instance: instance.as_raw() as VkHandle,
            physical: physical.as_raw() as VkHandle,
            device: device.as_raw() as VkHandle,
            gipa: crate::nr_layout::gipa as Address,
            gdpa: crate::nr_layout::gdpa as Address,
            api_version: NGX_API_VERSION,
            parameters: capabilities,
        };
        let mut call = nr_api::empty_call(0);
        call.init = &args;
        checked("target_nr_snippet_init", session.api.call(call))?;
        event(
            "target_nr_runtime_ready",
            json!({"runtime_sha256":runtime_hash,"bridge_sha256":bridge_hash,"device":device.as_raw(),"shared_core_owner":"Streamline","synthetic_depth":true,"exports":nr_api::export_names(),"addresses":(0..6).map(|i|session.api.address(i) as usize).collect::<Vec<_>>()}),
        )?;
        SESSION
            .set(Arc::new(Mutex::new(ManuallyDrop::into_inner(session))))
            .map_err(|_| "duplicate NR session")?;
        Ok(())
    })();
    if let Err(error) = &result {
        let _ = FAILURE.set(error.to_string());
        trace::event!(
            "target_nr_disabled",
            json!({"stage":"initialize","error":error.to_string(),"shared_core_preserved":true})
        );
    }
    result
}
pub(super) fn ready() -> bool {
    SESSION.get().is_some() && !CLOSED.load(Ordering::Acquire)
}
pub(super) fn failure() -> Option<&'static str> {
    FAILURE.get().map(String::as_str)
}
pub(super) struct Feature {
    session: Arc<Mutex<Session>>,
    parameters: usize,
    handle: usize,
    pass: u8,
    instance: u64,
}
fn feature_checked(pass: u8, operation: &str, result: u32) -> Result<()> {
    checked(
        &format!(
            "target_nr_{}{operation}",
            if pass == 2 { "second_" } else { "" }
        ),
        result,
    )
}
impl Feature {
    pub(super) fn instance(&self) -> u64 {
        self.instance
    }
    pub(super) fn identity(&self) -> (u64, u64) {
        (self.parameters as u64, self.handle as u64)
    }
    pub(super) unsafe fn allocate(pass: u8) -> Result<Self> {
        if !(1..=2).contains(&pass) {
            return Err("invalid NR pass index".into());
        }
        let session = SESSION.get().ok_or("NR session is not ready")?.clone();
        let mut s = session.lock().unwrap();
        if CLOSED.load(Ordering::Acquire) {
            return Err("NR session closed".into());
        }
        let mut params = std::ptr::null_mut();
        feature_checked(
            pass,
            "allocate_parameters",
            nr_api::NVSDK_NGX_VULKAN_AllocateParameters(&mut params),
        )?;
        if params.is_null() {
            return Err("NR parameter map is null".into());
        }
        let mut call = nr_api::empty_call(1);
        call.parameters = params;
        let result = s.api.call(call);
        if let Err(error) = feature_checked(pass, "populate_parameters", result) {
            feature_checked(
                pass,
                "destroy_unused_parameters",
                nr_api::NVSDK_NGX_VULKAN_DestroyParameters(params),
            )?;
            return Err(error);
        }
        s.features += 1;
        drop(s);
        Ok(Self {
            session,
            parameters: params as usize,
            handle: 0,
            pass,
            instance: FEATURE_ID.fetch_add(1, Ordering::Relaxed),
        })
    }
    pub(super) unsafe fn record(
        &mut self,
        command: vk::CommandBuffer,
        resources: &mut [ResourceVk; 4],
        intensity: f32,
        options: crate::advanced_settings::NrOptions,
        reset: bool,
        uv_scale: [f32; 2],
    ) -> Result<()> {
        let s = self.session.lock().unwrap();
        let params = self.parameters as *mut c_void;
        let _scope =
            crate::nr_layout::Recording::enter(command, resources[0].width, resources[0].height);
        if self.handle == 0 {
            nr_api::set_dimensions(params, resources[0].width, resources[0].height);
            let mut handle = std::ptr::null_mut();
            let mut call = nr_api::empty_call(2);
            call.parameters = params;
            call.command = command.as_raw() as VkHandle;
            call.device = s.device.as_raw() as VkHandle;
            call.output = &mut handle;
            let result = s.api.call(call);
            self.handle = handle as usize;
            feature_checked(self.pass, "create_feature", result)?;
            if handle.is_null() {
                return Err("NR feature is null".into());
            }
        }
        let options = options.model_only();
        if options != Default::default() {
            nr_api::set_frame_tuned(params, resources, intensity, reset, uv_scale, options);
        } else if uv_scale == [1.0, 1.0] {
            nr_api::set_frame(params, resources, intensity, reset);
        } else {
            nr_api::set_frame_scaled(params, resources, intensity, reset, uv_scale);
        }
        let mut call = nr_api::empty_call(3);
        call.command = command.as_raw() as VkHandle;
        call.parameters = params;
        call.feature = self.handle as *mut c_void;
        feature_checked(self.pass, "evaluate", s.api.call(call))
    }
}
impl Drop for Feature {
    fn drop(&mut self) {
        unsafe {
            // The frame resource owner drops features only after GPU quiescence.
            let mut s = self.session.lock().unwrap();
            if self.handle != 0 {
                let mut call = nr_api::empty_call(4);
                call.feature = self.handle as *mut c_void;
                feature_checked(self.pass, "release_feature", s.api.call(call))
                    .unwrap_or_else(|_| std::process::abort());
            }
            feature_checked(
                self.pass,
                "destroy_parameters",
                nr_api::NVSDK_NGX_VULKAN_DestroyParameters(self.parameters as *mut c_void),
            )
            .unwrap_or_else(|_| std::process::abort());
            s.features -= 1;
        }
    }
}
pub(super) unsafe fn shutdown(device: vk::Device) {
    let Some(session) = SESSION.get() else {
        return;
    };
    let mut s = session.lock().unwrap();
    if s.device != device || CLOSED.swap(true, Ordering::AcqRel) {
        return;
    }
    if s.features != 0 {
        std::process::abort();
    }
    let mut call = nr_api::empty_call(5);
    call.device = device.as_raw() as VkHandle;
    checked("target_nr_snippet_shutdown", s.api.call(call))
        .unwrap_or_else(|_| std::process::abort());
    checked(
        "target_nr_destroy_capabilities",
        nr_api::NVSDK_NGX_VULKAN_DestroyParameters(s.capabilities as *mut c_void),
    )
    .unwrap_or_else(|_| std::process::abort());
    s.capabilities = 0;
    trace::event!(
        "target_nr_core_deferred",
        json!({"owner":"Streamline","all_features_released":true})
    );
}
