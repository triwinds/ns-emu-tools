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
    m.pipelines.insert(
        (DEVICE, 7),
        Known {
            valid: true,
            generation: 1,
        },
    );
    m.renderpasses.insert(
        (DEVICE, 8),
        Known {
            valid: true,
            generation: 1,
        },
    );
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
    record_path(m, 7, 8);
}
fn record_path(m: &mut Model, pipeline: u64, renderpass: u64) {
    let source_image = m
        .descriptors
        .get(&(DEVICE, 12))
        .and_then(|d| m.views.get(&(DEVICE, d.view)))
        .map_or(42, |v| v.image);
    m.begin(DEVICE, COMMAND, true);
    m.pipeline(DEVICE, COMMAND, pipeline);
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
            source_image,
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::GENERAL,
        )],
    );
    m.begin_pass(DEVICE, COMMAND, 4, renderpass);
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
                m.pipelines.get_mut(&(DEVICE, 7)).unwrap().valid = false;
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

fn add_source(m: &mut Model, image: u64, generation: u64) {
    let original = m.images[&(DEVICE, 42)];
    m.images.insert(
        (DEVICE, image),
        Image {
            generation,
            ..original
        },
    );
    m.views.insert(
        (DEVICE, image + 1),
        View {
            image,
            format: 37,
            ..Default::default()
        },
    );
}
fn present_source(m: &mut Model, image: u64) -> Source {
    m.descriptors.get_mut(&(DEVICE, 12)).unwrap().view = image + 1;
    record(m);
    submit(m);
    select(m).unwrap()
}
#[test]
fn observed_live_rotation_preserves_nr_history_and_new_members_reset_once() {
    use crate::nr_history::{Controls, History, Reason, Source as TemporalSource};
    let mut m = fixture();
    add_source(&mut m, 52, 2);
    add_source(&mut m, 62, 3);
    let mut history = History::default();
    let controls = Controls {
        enabled: true,
        intensity: 1.0,
        options: Default::default(),
    };
    let mut identities = Vec::new();
    for (frame, image, expected) in [
        (0, 42, Reason::Created),
        (1, 52, Reason::SourceChanged),
        (2, 42, Reason::Continuous),
        (3, 52, Reason::Continuous),
        (4, 62, Reason::SourceChanged),
        (5, 42, Reason::Continuous),
        (6, 62, Reason::Continuous),
    ] {
        let s = present_source(&mut m, image);
        identities.push(s.history_identity);
        let decision = history
            .next(
                controls,
                TemporalSource {
                    identity: s.history_identity,
                    extent: [1920, 1080],
                    mapping: 1,
                },
                frame,
                true,
            )
            .unwrap();
        assert_eq!(decision.reason, expected);
        assert_eq!(decision.reset_nr, expected != Reason::Continuous);
        history.sr_consumed();
        history.fg_consumed();
        assert_eq!(s.image.as_raw(), image);
        assert_eq!(s.generation, m.images[&(DEVICE, image)].generation);
    }
    assert_eq!(identities[1], identities[2]);
    assert_ne!(identities[3], identities[4]);
    assert_eq!(m.cohorts[&(DEVICE, QUEUE, 5)].members.len(), 3);
}
#[test]
fn equivalent_rotating_paths_and_extra_usage_preserve_the_observed_stream() {
    let mut m = fixture();
    add_source(&mut m, 52, 2);
    m.images.get_mut(&(DEVICE, 52)).unwrap().usage |= 2 | 4;
    m.pipelines.insert(
        (DEVICE, 17),
        Known {
            valid: true,
            generation: 2,
        },
    );
    m.renderpasses.insert(
        (DEVICE, 18),
        Known {
            valid: true,
            generation: 2,
        },
    );
    let present = |m: &mut Model, image: u64, p: u64, r: u64| {
        m.descriptors.get_mut(&(DEVICE, 12)).unwrap().view = image + 1;
        record_path(m, p, r);
        submit(m);
        select(m).unwrap()
    };
    let first = present(&mut m, 42, 7, 8);
    let learned = present(&mut m, 52, 17, 18);
    assert_ne!(first.history_identity, learned.history_identity);
    assert_eq!(learned.history_update, "new_image_and_path");
    for image in [42, 52, 42, 52] {
        let (p, r) = if image == 42 { (7, 8) } else { (17, 18) };
        let s = present(&mut m, image, p, r);
        assert_eq!(s.history_identity, learned.history_identity);
        assert_eq!((s.history_members, s.history_paths), (2, 2));
        assert_eq!(s.history_update, "continuous");
    }
    // An unselected path retiring must break continuity immediately.
    m.pipelines.remove(&(DEVICE, 17));
    let retired = present(&mut m, 42, 7, 8);
    assert_ne!(retired.history_identity, learned.history_identity);
    assert_eq!(retired.history_update, "path_retired");
    m.pipelines.insert(
        (DEVICE, 17),
        Known {
            valid: true,
            generation: 99,
        },
    );
    assert_ne!(
        present(&mut m, 52, 17, 18).history_identity,
        retired.history_identity
    );
}
#[test]
fn new_path_with_an_observed_image_resets_once_and_path_pool_is_bounded() {
    let mut m = fixture();
    let mut old = present_source(&mut m, 42).history_identity;
    for i in 0..MAX_PRESENT_PATHS {
        let pipeline = 100 + i as u64;
        m.pipelines.insert(
            (DEVICE, pipeline),
            Known {
                valid: true,
                generation: 20 + i as u64,
            },
        );
        record_path(&mut m, pipeline, 8);
        submit(&mut m);
        let first = select(&mut m).unwrap();
        assert_ne!(first.history_identity, old);
        assert_eq!(first.history_members, 1);
        assert!((1..=MAX_PRESENT_PATHS as u32).contains(&first.history_paths));
        record_path(&mut m, pipeline, 8);
        submit(&mut m);
        assert_eq!(
            select(&mut m).unwrap().history_identity,
            first.history_identity
        );
        old = first.history_identity;
    }
    assert_eq!(m.cohorts[&(DEVICE, QUEUE, 5)].paths.len(), 1);
}
#[test]
fn destroyed_or_reallocated_pool_member_breaks_history_before_it_is_reused() {
    let mut m = fixture();
    add_source(&mut m, 52, 2);
    present_source(&mut m, 42);
    let old = present_source(&mut m, 52).history_identity;
    assert_eq!(present_source(&mut m, 42).history_identity, old);
    m.images.remove(&(DEVICE, 52));
    let after_destroy = present_source(&mut m, 42);
    assert_ne!(after_destroy.history_identity, old);
    assert_eq!(after_destroy.history_members, 1);
    add_source(&mut m, 52, 99);
    let recreated = present_source(&mut m, 52);
    assert_ne!(recreated.history_identity, after_destroy.history_identity);
    assert_eq!(recreated.generation, 99);
    // Even a member not used in this frame must keep its allocation generation.
    m.images.get_mut(&(DEVICE, 52)).unwrap().generation += 1;
    assert_ne!(
        present_source(&mut m, 42).history_identity,
        recreated.history_identity
    );
}
#[test]
fn mapping_encoding_and_validated_pipeline_generations_break_history() {
    for fault in 0..4 {
        let mut m = fixture();
        let old = present_source(&mut m, 42).history_identity;
        match fault {
            0 => m.descriptors.get_mut(&(DEVICE, 10)).unwrap().uv = Some([1., 0., 1., 0.]),
            1 => m.images.get_mut(&(DEVICE, 42)).unwrap().format = 37,
            2 => m.pipelines.get_mut(&(DEVICE, 7)).unwrap().generation += 1,
            _ => m.renderpasses.get_mut(&(DEVICE, 8)).unwrap().generation += 1,
        }
        assert_ne!(
            present_source(&mut m, 42).history_identity,
            old,
            "fault {fault}"
        );
    }
    // A path changed after the draw cannot authorize a source at all.
    for pipeline in [true, false] {
        let mut m = fixture();
        record(&mut m);
        submit(&mut m);
        if pipeline {
            m.pipelines.get_mut(&(DEVICE, 7)).unwrap().generation += 1;
        } else {
            m.renderpasses.remove(&(DEVICE, 8));
        }
        assert_eq!(select(&mut m), Err("呈现路径已变化"));
    }
}
#[test]
fn missing_evidence_and_swapchain_retirement_cannot_reuse_an_old_epoch() {
    let mut m = fixture();
    let first = present_source(&mut m, 42).history_identity;
    assert!(select(&mut m).is_err());
    let recovered = present_source(&mut m, 42).history_identity;
    assert_ne!(first, recovered);
    m.retire_swapchain(DEVICE, 5);
    assert!(!m.chains.contains_key(&(DEVICE, 5)));
    assert!(m.cohorts.is_empty());
    m.chains.insert((DEVICE, 5), vec![99]);
    assert_ne!(present_source(&mut m, 42).history_identity, recovered);
    m.clear_frame_state();
    assert!(m.cohorts.is_empty());
}
#[test]
fn source_groups_are_bounded_and_scoped_to_the_presenting_swapchain() {
    let mut m = fixture();
    for i in 0..MAX_SOURCE_IMAGES + 1 {
        let image = 42 + i as u64 * 10;
        add_source(&mut m, image, i as u64 + 1);
        let s = present_source(&mut m, image);
        assert!((1..=MAX_SOURCE_IMAGES as u32).contains(&s.history_members));
    }
    assert_eq!(m.cohorts[&(DEVICE, QUEUE, 5)].members.len(), 1);
    let old = present_source(&mut m, 42).history_identity;
    m.chains.insert((DEVICE, 6), vec![99]);
    record(&mut m);
    submit(&mut m);
    let second = m
        .select(DEVICE, QUEUE, 6, 0, &[vk::Semaphore::from_raw(55)])
        .unwrap();
    assert_ne!(second.history_identity, old);
    assert_eq!(present_source(&mut m, 42).history_identity, old);
}
