//! Pinned-runtime internal image initialization for owned recording scopes.
//! Used by the isolated diagnostic and the opt-in native experiment, each with
//! one serialized Vulkan device and the same pinned runtime contract.
//! It inserts a real barrier immediately after binding a newly created matching
//! image, before NR can record its first access. It never patches a DLL, guesses
//! a handle, alters validation state, or transitions an already used image.
use ash::vk::{self, Handle};
use serde_json::json;
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    ffi::{c_char, CStr},
    marker::PhantomData,
    rc::Rc,
    sync::{Mutex, OnceLock},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type Event = fn(&str, serde_json::Value) -> Result<()>;

struct Dispatch {
    event: Event,
    gipa: vk::PFN_vkGetInstanceProcAddr,
    gdpa: vk::PFN_vkGetDeviceProcAddr,
    create: vk::PFN_vkCreateImage,
    bind: vk::PFN_vkBindImageMemory,
    barrier: vk::PFN_vkCmdPipelineBarrier,
    clear: vk::PFN_vkCmdClearColorImage,
    destroy: vk::PFN_vkDestroyImage,
    device: vk::Device,
    candidates: Mutex<HashMap<vk::Image, vk::CommandBuffer>>,
    exposure_images: Mutex<HashSet<vk::Image>>,
}
static DISPATCH: OnceLock<Dispatch> = OnceLock::new();
thread_local! {
    static COMMAND: Cell<vk::CommandBuffer> = const { Cell::new(vk::CommandBuffer::null()) };
    static EXTENT: Cell<(u32,u32)> = const { Cell::new((0,0)) };
    static SR:Cell<bool> = const {Cell::new(false)};
}

pub(super) unsafe fn install(
    gipa: vk::PFN_vkGetInstanceProcAddr,
    gdpa: vk::PFN_vkGetDeviceProcAddr,
    device: &ash::Device,
    event: Event,
) -> Result<()> {
    DISPATCH
        .set(Dispatch {
            event,
            gipa,
            gdpa,
            create: device.fp_v1_0().create_image,
            bind: device.fp_v1_0().bind_image_memory,
            barrier: device.fp_v1_0().cmd_pipeline_barrier,
            clear: device.fp_v1_0().cmd_clear_color_image,
            destroy: device.fp_v1_0().destroy_image,
            device: device.handle(),
            candidates: Mutex::new(HashMap::new()),
            exposure_images: Mutex::new(HashSet::new()),
        })
        .map_err(|_| "pinned NR route supports one device lifecycle per process")?;
    event(
        "layout_probe_installed",
        json!({"experimental":true,
        "scope":"pinned NR/SR proc lookup; matching fresh images in an explicit recording scope",
        "transition":"UNDEFINED -> GENERAL after successful bind, before first use"}),
    )
}

// Drop clears thread-local state, so the guard must stay on its creating thread.
pub(super) struct Recording(PhantomData<Rc<()>>);
impl Recording {
    #[cfg(any(feature = "nr-coexistence", feature = "native-nr"))]
    // The standalone NR diagnostic also includes this module with native-nr,
    // but only the coexistence diagnostic records SR scopes.
    #[cfg_attr(not(feature = "nr-coexistence"), allow(dead_code))]
    pub(super) fn sr(command: vk::CommandBuffer, width: u32, height: u32) -> Self {
        let scope = Self::enter(command, width, height);
        SR.with(|sr| sr.set(true));
        scope
    }
    pub(super) fn enter(command: vk::CommandBuffer, width: u32, height: u32) -> Self {
        EXTENT.with(|extent| extent.set((width, height)));
        COMMAND.with(|current| {
            // This diagnostic has one recording thread and no nested NR calls.
            if current.replace(command) != vk::CommandBuffer::null() {
                std::process::abort();
            }
        });
        Self(PhantomData)
    }
}
impl Drop for Recording {
    fn drop(&mut self) {
        SR.with(|sr| sr.set(false));
        let command = COMMAND.with(|current| current.replace(vk::CommandBuffer::null()));
        if let Some(dispatch) = DISPATCH.get() {
            // Never carry unbound/destroyed candidates across an NR call, where
            // handle reuse or another command buffer could make them stale.
            dispatch
                .candidates
                .lock()
                .unwrap()
                .retain(|_, owner| *owner != command);
        }
    }
}

unsafe fn replacement(name: *const c_char) -> vk::PFN_vkVoidFunction {
    match CStr::from_ptr(name).to_bytes() {
        b"vkGetDeviceProcAddr" => Some(std::mem::transmute::<
            vk::PFN_vkGetDeviceProcAddr,
            unsafe extern "system" fn(),
        >(gdpa)),
        b"vkGetInstanceProcAddr" => Some(std::mem::transmute::<
            vk::PFN_vkGetInstanceProcAddr,
            unsafe extern "system" fn(),
        >(gipa)),
        b"vkCreateImage" => Some(std::mem::transmute::<
            vk::PFN_vkCreateImage,
            unsafe extern "system" fn(),
        >(create_image)),
        b"vkBindImageMemory" => Some(std::mem::transmute::<
            vk::PFN_vkBindImageMemory,
            unsafe extern "system" fn(),
        >(bind_image)),
        b"vkCmdClearColorImage" => Some(std::mem::transmute::<
            vk::PFN_vkCmdClearColorImage,
            unsafe extern "system" fn(),
        >(clear_image)),
        b"vkDestroyImage" => Some(std::mem::transmute::<
            vk::PFN_vkDestroyImage,
            unsafe extern "system" fn(),
        >(destroy_image)),
        _ => None,
    }
}
pub(super) unsafe extern "system" fn gipa(
    instance: vk::Instance,
    name: *const c_char,
) -> vk::PFN_vkVoidFunction {
    let real = (DISPATCH.get().unwrap().gipa)(instance, name);
    if real.is_some() {
        replacement(name).or(real)
    } else {
        real
    }
}
pub(super) unsafe extern "system" fn gdpa(
    device: vk::Device,
    name: *const c_char,
) -> vk::PFN_vkVoidFunction {
    let real = (DISPATCH.get().unwrap().gdpa)(device, name);
    if real.is_some() {
        replacement(name).or(real)
    } else {
        real
    }
}
unsafe extern "system" fn create_image(
    device: vk::Device,
    info: *const vk::ImageCreateInfo<'_>,
    allocator: *const vk::AllocationCallbacks<'_>,
    image: *mut vk::Image,
) -> vk::Result {
    let d = DISPATCH.get().unwrap();
    let result = (d.create)(device, info, allocator, image);
    if result == vk::Result::SUCCESS && device == d.device {
        let i = &*info;
        let (width, height) = EXTENT.with(Cell::get);
        let sr = SR.with(Cell::get);
        if i.p_next.is_null()
            && i.flags.is_empty()
            && i.image_type == vk::ImageType::TYPE_2D
            && (i.format
                == if sr {
                    vk::Format::R16G16_SFLOAT
                } else {
                    vk::Format::R16G16B16A16_SFLOAT
                })
            && i.extent
                == (vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                })
            && i.mip_levels == 1
            && i.array_layers == 1
            && i.samples == vk::SampleCountFlags::TYPE_1
            && i.tiling == vk::ImageTiling::OPTIMAL
            && i.usage
                == (vk::ImageUsageFlags::TRANSFER_SRC
                    | vk::ImageUsageFlags::TRANSFER_DST
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::STORAGE)
            && i.sharing_mode == vk::SharingMode::EXCLUSIVE
            && i.initial_layout == vk::ImageLayout::UNDEFINED
            && COMMAND.with(|c| c.get() != vk::CommandBuffer::null())
        {
            d.candidates
                .lock()
                .unwrap()
                .insert(*image, COMMAND.with(Cell::get));
        }
        if sr
            && i.image_type == vk::ImageType::TYPE_2D
            && i.format == vk::Format::R16_SFLOAT
            && i.extent
                == (vk::Extent3D {
                    width: 1,
                    height: 1,
                    depth: 1,
                })
            && i.mip_levels == 1
            && i.array_layers == 1
            && i.samples == vk::SampleCountFlags::TYPE_1
            && i.tiling == vk::ImageTiling::OPTIMAL
            && i.p_next.is_null()
            && i.flags.is_empty()
            && i.usage.as_raw() == 15
            && i.sharing_mode == vk::SharingMode::EXCLUSIVE
            && i.initial_layout == vk::ImageLayout::UNDEFINED
            && COMMAND.with(|c| c.get() != vk::CommandBuffer::null())
        {
            d.exposure_images.lock().unwrap().insert(*image);
        }
    }
    result
}
unsafe extern "system" fn clear_image(
    command: vk::CommandBuffer,
    image: vk::Image,
    layout: vk::ImageLayout,
    color: *const vk::ClearColorValue,
    count: u32,
    ranges: *const vk::ImageSubresourceRange,
) {
    let d = DISPATCH.get().unwrap();
    if SR.with(Cell::get)
        && COMMAND.with(Cell::get) == command
        && d.exposure_images.lock().unwrap().contains(&image)
    {
        let barrier = vk::MemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
        (d.barrier)(
            command,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            1,
            &barrier,
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
        );
        if (d.event)(
            "sr_exposure_clear_dependency",
            json!({"image":image.as_raw(),"command":command.as_raw(),"layout_unchanged":true}),
        )
        .is_err()
        {
            std::process::abort();
        }
    }
    (d.clear)(command, image, layout, color, count, ranges);
}
unsafe extern "system" fn destroy_image(
    device: vk::Device,
    image: vk::Image,
    allocator: *const vk::AllocationCallbacks<'_>,
) {
    let d = DISPATCH.get().unwrap();
    if device == d.device {
        d.candidates.lock().unwrap().remove(&image);
        d.exposure_images.lock().unwrap().remove(&image);
    }
    (d.destroy)(device, image, allocator);
}
unsafe extern "system" fn bind_image(
    device: vk::Device,
    image: vk::Image,
    memory: vk::DeviceMemory,
    offset: vk::DeviceSize,
) -> vk::Result {
    let d = DISPATCH.get().unwrap();
    let result = (d.bind)(device, image, memory, offset);
    let command = COMMAND.with(Cell::get);
    let candidate =
        device == d.device && d.candidates.lock().unwrap().remove(&image) == Some(command);
    if result == vk::Result::SUCCESS && candidate && command != vk::CommandBuffer::null() {
        let barrier = vk::ImageMemoryBarrier::default()
            .image(image)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::GENERAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .level_count(1)
                    .layer_count(1),
            );
        (d.barrier)(
            command,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::DependencyFlags::empty(),
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
            1,
            &barrier,
        );
        if (d.event)(
            "layout_probe_transition",
            json!({"image":image.as_raw(),"command":command.as_raw(),
            "old_layout":"UNDEFINED","new_layout":"GENERAL","before_first_use":true}),
        )
        .is_err()
        {
            std::process::abort();
        }
    }
    result
}
