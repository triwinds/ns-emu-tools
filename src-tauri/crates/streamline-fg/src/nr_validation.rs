//! Strict core/synchronization evidence for the process-scoped native experiment.
use ash::vk::{self, Handle};
use serde_json::json;
use std::{
    collections::HashMap,
    ffi::{c_void, CStr},
    fs::OpenOptions,
    io::Write,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    sync::{Mutex, OnceLock},
};
static MESSENGERS: OnceLock<
    Mutex<
        HashMap<
            u64,
            (
                vk::DebugUtilsMessengerEXT,
                vk::PFN_vkDestroyDebugUtilsMessengerEXT,
            ),
        >,
    >,
> = OnceLock::new();
static LOG: Mutex<()> = Mutex::new(());
static FAILED: AtomicBool = AtomicBool::new(false);
static ERRORS: AtomicU64 = AtomicU64::new(0);
static WARNINGS: AtomicU64 = AtomicU64::new(0);
static UNREVIEWED: AtomicU64 = AtomicU64::new(0);
pub(super) fn enabled() -> bool {
    (crate::nr_runtime::requested()
        && std::env::var("NS_STREAMLINE_NR_VALIDATION").as_deref() != Ok("0"))
        || std::env::var("NS_STREAMLINE_SDK_VALIDATION").as_deref() == Ok("1")
}
pub(super) fn create_info() -> vk::DebugUtilsMessengerCreateInfoEXT<'static> {
    vk::DebugUtilsMessengerCreateInfoEXT::default()
        .message_severity(
            vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
        )
        .message_type(
            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
        )
        .pfn_user_callback(Some(callback))
}
unsafe extern "system" fn callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    kind: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _: *mut c_void,
) -> vk::Bool32 {
    if data.is_null() || (*data).p_message.is_null() {
        return vk::FALSE;
    }
    let message = CStr::from_ptr((*data).p_message).to_string_lossy();
    let error = severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR);
    let warning = severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::WARNING);
    let reviewed=warning&&kind==vk::DebugUtilsMessageTypeFlagsEXT::GENERAL&&(*data).message_id_number==0&&["VK_LAYER_NV_optimus","VK_LAYER_NV_present","VK_LAYER_AMD_switchable_graphics","VK_LAYER_reshade"].iter().any(|name|message==format!("Layer \"{name}\" forced disabled because name matches filter of env var 'VK_LOADER_LAYERS_DISABLE'."));
    ERRORS.fetch_add(u64::from(error), Ordering::Relaxed);
    WARNINGS.fetch_add(u64::from(warning), Ordering::Relaxed);
    UNREVIEWED.fetch_add(u64::from(warning && !reviewed), Ordering::Relaxed);
    let row = json!({"severity":severity.as_raw(),"type":kind.as_raw(),"id_number":(*data).message_id_number,"message":message,"application_call":crate::validation_context::current(),"review":if reviewed{Some("Expected child-only exclusion of implicit layers")}else{None}});
    let _guard = LOG.lock().unwrap_or_else(|e| e.into_inner());
    let result = (|| -> std::io::Result<()> {
        let dir = std::env::var_os("NS_STREAMLINE_LIVE_DIR")
            .ok_or_else(|| std::io::Error::other("missing validation directory"))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(std::path::PathBuf::from(dir).join("nr-validation.jsonl"))?;
        serde_json::to_writer(&mut file, &row)?;
        writeln!(file)?;
        file.flush()
    })();
    if result.is_err() {
        FAILED.store(true, Ordering::Relaxed);
    }
    vk::FALSE
}
pub(super) unsafe fn created(
    instance: vk::Instance,
    gipa: vk::PFN_vkGetInstanceProcAddr,
) -> Result<(), vk::Result> {
    let create: vk::PFN_vkCreateDebugUtilsMessengerEXT = std::mem::transmute(
        gipa(instance, c"vkCreateDebugUtilsMessengerEXT".as_ptr())
            .ok_or(vk::Result::ERROR_EXTENSION_NOT_PRESENT)?,
    );
    let destroy: vk::PFN_vkDestroyDebugUtilsMessengerEXT = std::mem::transmute(
        gipa(instance, c"vkDestroyDebugUtilsMessengerEXT".as_ptr())
            .ok_or(vk::Result::ERROR_EXTENSION_NOT_PRESENT)?,
    );
    let mut messenger = vk::DebugUtilsMessengerEXT::null();
    create(instance, &create_info(), std::ptr::null(), &mut messenger).result()?;
    MESSENGERS
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .insert(instance.as_raw(), (messenger, destroy));
    crate::trace::event!(
        "target_nr_validation",
        json!({"core":true,"synchronization":true,"messenger":messenger.as_raw()})
    );
    Ok(())
}
pub(super) unsafe fn before_destroy(instance: vk::Instance) {
    let record = MESSENGERS
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .remove(&instance.as_raw());
    if let Some((messenger, destroy)) = record {
        destroy(instance, messenger, std::ptr::null());
    }
}
pub(super) fn report(complete: bool) {
    let errors = ERRORS.load(Ordering::Relaxed);
    let unreviewed = UNREVIEWED.load(Ordering::Relaxed);
    let value = json!({"complete":complete,"errors":errors,"warnings":WARNINGS.load(Ordering::Relaxed),"unreviewed_warnings":unreviewed,"log_failed":FAILED.load(Ordering::Relaxed),"core_validation":true,"synchronization_validation":true,"validation_passed":complete&&errors==0&&unreviewed==0&&!FAILED.load(Ordering::Relaxed),"game_integration_accepted":false});
    crate::trace::event!("target_nr_validation_summary", value.clone());
    if let Some(dir) = std::env::var_os("NS_STREAMLINE_LIVE_DIR") {
        if std::fs::write(
            std::path::PathBuf::from(dir).join("nr-validation-result.json"),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .is_err()
        {
            FAILED.store(true, Ordering::Relaxed);
            crate::trace::event!(
                "target_nr_validation_write_failed",
                json!({"validation_passed":false})
            );
        }
    }
}
