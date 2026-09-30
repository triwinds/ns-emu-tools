use super::*;
const DEVICE: u64 = 1;
const COMMAND: u64 = 2;
const QUEUE: u64 = 3;
fn fixture() -> Model {
    let mut m = Model::default();
    m.images.insert(
        (DEVICE, 42),
        Image {
            extent: [1920, 1080, 1],
            format: 43,
            usage: 1,
            mips: 1,
            layers: 1,
            samples: 1,
            generation: 1,
        },
    );
    m.views.insert(
        (DEVICE, 43),
        View {
            image: 42,
            format: 37,
            ..Default::default()
        },
    );
    m.views.insert(
        (DEVICE, 90),
        View {
            image: 99,
            ..Default::default()
        },
    );
    m.framebuffers.insert(
        (DEVICE, 4),
        Framebuffer {
            views: vec![90],
            size: [2560, 1335],
        },
    );
    m.chains.insert((DEVICE, 5), vec![99]);
    m.samplers.insert((DEVICE, 6), true);
    m.pipelines.insert((DEVICE, 7), true);
    m.renderpasses.insert((DEVICE, 8), true);
    m.descriptors.insert(
        (DEVICE, 10),
        Descriptor {
            uv: Some([0., 1., 1., 0.]),
            ..Default::default()
        },
    );
    m.descriptors.insert(
        (DEVICE, 12),
        Descriptor {
            view: 43,
            layout: 1,
            sampler: 6,
            ..Default::default()
        },
    );
    m
}
fn barrier(
    image: u64,
    old: vk::ImageLayout,
    new: vk::ImageLayout,
) -> vk::ImageMemoryBarrier<'static> {
    vk::ImageMemoryBarrier::default()
        .image(vk::Image::from_raw(image))
        .old_layout(old)
        .new_layout(new)
        .src_queue_family_index(u32::MAX)
        .dst_queue_family_index(u32::MAX)
        .subresource_range(
            vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .level_count(1)
                .layer_count(1),
        )
}
fn record(m: &mut Model) {
    m.begin(DEVICE, COMMAND, true);
    m.pipeline(DEVICE, COMMAND, 7);
    m.sets(
        DEVICE,
        COMMAND,
        0,
        &[
            vk::DescriptorSet::from_raw(10),
            vk::DescriptorSet::null(),
            vk::DescriptorSet::from_raw(12),
        ],
    );
    m.serial = 10;
    m.barriers(
        DEVICE,
        COMMAND,
        &[barrier(
            42,
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::GENERAL,
        )],
    );
    m.begin_pass(DEVICE, COMMAND, 4, 8);
    m.viewport(
        DEVICE,
        COMMAND,
        0,
        &[vk::Viewport {
            x: 93.,
            y: 0.,
            width: 2374.,
            height: 1335.,
            min_depth: 0.,
            max_depth: 1.,
        }],
    );
    m.scissor(
        DEVICE,
        COMMAND,
        0,
        &[vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D {
                width: 2560,
                height: 1335,
            },
        }],
    );
    unsafe {
        m.clear(
            DEVICE,
            COMMAND,
            &[vk::ClearAttachment {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                color_attachment: 0,
                clear_value: vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0., 0., 0., 1.],
                    },
                },
            }],
            &[vk::ClearRect {
                rect: vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: vk::Extent2D {
                        width: 2560,
                        height: 1335,
                    },
                },
                base_array_layer: 0,
                layer_count: 1,
            }],
        );
    }
    m.serial = 11;
    m.draw(DEVICE, COMMAND, false, [4, 1, 0, 0]);
    m.end_pass(DEVICE, COMMAND);
    m.serial = 12;
    m.barriers(
        DEVICE,
        COMMAND,
        &[barrier(
            99,
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
        )],
    );
    m.end(DEVICE, COMMAND, true);
}
fn submit(m: &mut Model) {
    m.serial += 1;
    unsafe {
        m.submit(
            DEVICE,
            QUEUE,
            &[vk::SubmitInfo::default()
                .command_buffers(&[vk::CommandBuffer::from_raw(COMMAND)])
                .signal_semaphores(&[vk::Semaphore::from_raw(55)])],
            true,
        );
    }
}
fn select(m: &mut Model) -> Result<Source, &'static str> {
    m.select(DEVICE, QUEUE, 5, 0, &[vk::Semaphore::from_raw(55)])
}
#[test]
fn native_srgb_flip_and_letterbox_survive_record_submit_present() {
    let mut m = fixture();
    record(&mut m);
    submit(&mut m);
    let source = select(&mut m).unwrap();
    assert_eq!(source.image.as_raw(), 42);
    assert!(source.raw_copy);
    assert_eq!(source.viewport, [93., 0., 2374., 1335.]);
    assert_eq!(
        source.offsets,
        [
            vk::Offset3D {
                x: 0,
                y: 1080,
                z: 0
            },
            vk::Offset3D {
                x: 1920,
                y: 0,
                z: 1
            }
        ]
    );
    assert!(select(&mut m).is_err()); // A present consumes its evidence.
}
#[test]
fn rejects_shader_drift_sampler_changes_and_missing_clear() {
    for fault in 0..4 {
        let mut m = fixture();
        match fault {
            0 => {
                m.pipelines.insert((DEVICE, 7), false);
            }
            1 => {
                m.samplers.insert((DEVICE, 6), false);
            }
            2 => {
                m.descriptors.get_mut(&(DEVICE, 10)).unwrap().uv = Some([0., 1., f32::NAN, 0.]);
            }
            _ => {
                m.views.get_mut(&(DEVICE, 43)).unwrap().format = 43;
            }
        }
        record(&mut m);
        submit(&mut m);
        assert!(select(&mut m).is_err());
    }
    let mut m = fixture();
    record(&mut m);
    m.commands
        .get_mut(&(DEVICE, COMMAND))
        .unwrap()
        .draws
        .clear();
    m.begin_pass(DEVICE, COMMAND, 4, 8);
    m.draw(DEVICE, COMMAND, false, [4, 1, 0, 0]);
    submit(&mut m);
    assert_eq!(select(&mut m), Err("黑边清除条件不匹配"));
}
#[test]
fn rejects_source_transition_after_draw_and_recycled_image_handles() {
    let mut m = fixture();
    record(&mut m);
    m.barriers(
        DEVICE,
        COMMAND,
        &[barrier(
            42,
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        )],
    );
    submit(&mut m);
    assert_eq!(select(&mut m), Err("源纹理同步条件不匹配"));
    let mut m = fixture();
    record(&mut m);
    submit(&mut m);
    m.images.get_mut(&(DEVICE, 42)).unwrap().generation += 1;
    assert_eq!(select(&mut m), Err("源纹理已变化"));
}
#[test]
fn rejects_wrong_semaphore_extra_submit_reset_and_missing_present_barrier() {
    let mut m = fixture();
    record(&mut m);
    submit(&mut m);
    assert!(m
        .select(DEVICE, QUEUE, 5, 0, &[vk::Semaphore::from_raw(56)])
        .is_err());
    let mut m = fixture();
    record(&mut m);
    submit(&mut m);
    m.serial += 1;
    unsafe {
        m.submit(DEVICE, QUEUE, &[], true);
    }
    assert_eq!(select(&mut m), Err("源画面后还有其他 GPU 提交"));
    let mut m = fixture();
    record(&mut m);
    m.begin(DEVICE, COMMAND, true);
    submit(&mut m);
    assert!(select(&mut m).is_err());
    let mut m = fixture();
    record(&mut m);
    m.commands.get_mut(&(DEVICE, COMMAND)).unwrap().draws[0].presented = false;
    submit(&mut m);
    assert_eq!(select(&mut m), Err("缺少呈现布局转换"));
}
#[test]
fn reenable_requires_fresh_descriptor_and_command_evidence() {
    let mut m = fixture();
    record(&mut m);
    submit(&mut m);
    m.clear_frame_state();
    assert!(select(&mut m).is_err());
    record(&mut m);
    submit(&mut m);
    assert!(select(&mut m).is_err());
    assert_eq!(m.images.len(), 1);
}
