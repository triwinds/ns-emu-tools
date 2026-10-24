//! Experimental SDR inference resizing. Full P0 is retained for source identity
//! and reconstruction; all model passes share the same reduced extent.
use crate::fg_api::{self, Resource, Result};
use ash::vk::{self, Handle};

pub(super) fn inference_extent(
    size: vk::Extent2D,
    percent: u32,
    max_edge: u32,
) -> Result<vk::Extent2D> {
    if !(50..=100).contains(&percent) {
        return Err("NR inference scale must be 50..100 percent".into());
    }
    if !crate::advanced_settings::valid_nr_inference_max_edge(max_edge) {
        return Err("NR inference max edge must be 0 or 320..8192 pixels".into());
    }
    // Cap mode supersedes the saved percentage and never upscales. Integer
    // rational scaling keeps the long edge at the cap without floating error.
    let (numerator, denominator) = if max_edge == 0 {
        (percent, 100)
    } else {
        let longest = size.width.max(size.height).max(1);
        (max_edge.min(longest), longest)
    };
    let scaled = vk::Extent2D {
        width: (u64::from(size.width) * u64::from(numerator) / u64::from(denominator)) as u32,
        height: (u64::from(size.height) * u64::from(numerator) / u64::from(denominator)) as u32,
    };
    // Conservative experimental boundary; not a claim of a model minimum.
    if scaled != size && (scaled.width < 320 || scaled.height < 180) {
        return Err("scaled NR extent must be at least 320x180".into());
    }
    Ok(scaled)
}

pub(super) struct Reconstruction {
    device: ash::Device,
    pub(super) base: Option<Resource>,
    pub(super) output: Option<Resource>,
    pub(super) bytes: u64,
    layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
}
impl Reconstruction {
    pub(super) unsafe fn new(
        d: &ash::Device,
        memory: &vk::PhysicalDeviceMemoryProperties,
        size: vk::Extent2D,
        low_base: Resource,
    ) -> Result<Self> {
        let mut r = Self {
            device: d.clone(),
            base: None,
            output: None,
            bytes: 0,
            layout: vk::DescriptorSetLayout::null(),
            pool: vk::DescriptorPool::null(),
            set: vk::DescriptorSet::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
        };
        let usage = vk::ImageUsageFlags::STORAGE
            | vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::TRANSFER_DST;
        for slot in [&mut r.base, &mut r.output] {
            *slot = Some(fg_api::texture_with_usage(
                d,
                memory,
                size,
                vk::Format::R16G16B16A16_SFLOAT,
                usage,
            )?);
            r.bytes += d
                .get_image_memory_requirements(vk::Image::from_raw(slot.unwrap().image))
                .size;
        }
        let bindings: Vec<_> = (0..4)
            .map(|i| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(i)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
            })
            .collect();
        r.layout = d.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
            None,
        )?;
        r.pool = d.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(1)
                .pool_sizes(&[vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::STORAGE_IMAGE,
                    descriptor_count: 4,
                }]),
            None,
        )?;
        r.set = d.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(r.pool)
                .set_layouts(&[r.layout]),
        )?[0];
        for (binding, image) in [(0, r.base.unwrap()), (1, low_base), (3, r.output.unwrap())] {
            r.bind(binding, image);
        }
        r.pipeline_layout = d.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default().set_layouts(&[r.layout]),
            None,
        )?;
        let code = ash::util::read_spv(&mut std::io::Cursor::new(include_bytes!(
            "../shaders/nr_reconstruct.spv"
        )))?;
        let shader =
            d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)?;
        let pipeline = d.create_compute_pipelines(
            vk::PipelineCache::null(),
            &[vk::ComputePipelineCreateInfo::default()
                .layout(r.pipeline_layout)
                .stage(
                    vk::PipelineShaderStageCreateInfo::default()
                        .stage(vk::ShaderStageFlags::COMPUTE)
                        .module(shader)
                        .name(c"main"),
                )],
            None,
        );
        d.destroy_shader_module(shader, None);
        r.pipeline = pipeline.map_err(|(partial, error)| {
            for p in partial {
                d.destroy_pipeline(p, None);
            }
            error
        })?[0];
        Ok(r)
    }
    unsafe fn bind(&self, binding: u32, resource: Resource) {
        self.device.update_descriptor_sets(
            &[vk::WriteDescriptorSet::default()
                .dst_set(self.set)
                .dst_binding(binding)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .image_info(&[vk::DescriptorImageInfo::default()
                    .image_view(vk::ImageView::from_raw(resource.view))
                    .image_layout(vk::ImageLayout::GENERAL)])],
            &[],
        );
    }
    /// Owner has retired the previous frame before descriptor updates.
    pub(super) unsafe fn record(
        &self,
        cmd: vk::CommandBuffer,
        low_output: Resource,
        initialized: bool,
    ) -> Resource {
        self.bind(2, low_output);
        let output = self.output.unwrap();
        fg_api::transition(
            &self.device,
            cmd,
            vk::Image::from_raw(output.image),
            if initialized {
                vk::ImageLayout::GENERAL
            } else {
                vk::ImageLayout::UNDEFINED
            },
            vk::ImageLayout::GENERAL,
        );
        self.device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::MEMORY_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)],
            &[],
            &[],
        );
        self.device
            .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        self.device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[self.set],
            &[],
        );
        self.device
            .cmd_dispatch(cmd, output.width.div_ceil(8), output.height.div_ceil(8), 1);
        self.device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::DependencyFlags::empty(),
            &[vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::MEMORY_READ)],
            &[],
            &[],
        );
        output
    }
}
impl Drop for Reconstruction {
    fn drop(&mut self) {
        unsafe {
            let d = &self.device;
            d.destroy_pipeline(self.pipeline, None);
            d.destroy_pipeline_layout(self.pipeline_layout, None);
            d.destroy_descriptor_pool(self.pool, None);
            d.destroy_descriptor_set_layout(self.layout, None);
            for r in self.base.take().into_iter().chain(self.output.take()) {
                d.destroy_image_view(vk::ImageView::from_raw(r.view), None);
                d.destroy_image(vk::Image::from_raw(r.image), None);
                d.free_memory(vk::DeviceMemory::from_raw(r.memory), None);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scaling_preserves_original_extent_and_rejects_unproven_small_inputs() {
        let size = vk::Extent2D {
            width: 1724,
            height: 962,
        };
        assert_eq!(inference_extent(size, 100, 0).unwrap(), size);
        assert_eq!(
            inference_extent(size, 50, 0).unwrap(),
            vk::Extent2D {
                width: 862,
                height: 481
            }
        );
        assert_eq!(
            inference_extent(
                vk::Extent2D {
                    width: 640,
                    height: 360
                },
                50,
                0
            )
            .unwrap(),
            vk::Extent2D {
                width: 320,
                height: 180
            }
        );
        for percent in [0, 25, 49, 101, 200] {
            assert!(inference_extent(size, percent, 0).is_err());
        }
        assert!(inference_extent(
            vk::Extent2D {
                width: 639,
                height: 360
            },
            50,
            0
        )
        .is_err());
    }
}
