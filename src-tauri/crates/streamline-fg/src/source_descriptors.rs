// Native descriptor decoding for the online path. Never constructs JSON.
unsafe fn online_shader(info: &vk::ShaderModuleCreateInfo) -> u8 {
    use sha2::{Digest, Sha256};
    let hash = format!(
        "{:x}",
        Sha256::digest(std::slice::from_raw_parts(
            info.p_code.cast::<u8>(),
            info.code_size
        ))
    );
    match hash.as_str() {
        "b559ef9b2f2c0796bd8e9706595690edc091fcbee127832ab5a91e48c9cd8625" => 1,
        "c51c90364dfef8663959e0fa05f48e2142507973722a527627ddb06eb9783b39" => 2,
        _ => 0,
    }
}
unsafe fn online_template_created(
    device: u64,
    i: *const vk::DescriptorUpdateTemplateCreateInfo,
    o: *mut vk::DescriptorUpdateTemplate,
    r: vk::Result,
) {
    if r != vk::Result::SUCCESS {
        return;
    }
    templates().insert(
        (device, (*o).as_raw()),
        items(
            (*i).p_descriptor_update_entries,
            (*i).descriptor_update_entry_count,
        )
        .iter()
        .filter(|e| online_entry(e))
        .map(|e| {
            let mut e = *e;
            e.descriptor_count = 1;
            e
        })
        .collect(),
    );
}
#[derive(Clone, Copy)]
enum OnlineWrite {
    Image(u64, i32),
    Sampler(u64),
    Coordinates(Option<[f32; 4]>),
}
fn apply_online_write(
    m: &mut crate::source_model::Model,
    device: u64,
    set: u64,
    write: OnlineWrite,
) {
    let descriptor = m.descriptors.entry((device, set)).or_default();
    match write {
        OnlineWrite::Image(view, layout) => {
            descriptor.view = view;
            descriptor.layout = layout;
        }
        OnlineWrite::Sampler(sampler) => descriptor.sampler = sampler,
        OnlineWrite::Coordinates(uv) => descriptor.uv = uv,
    }
}
unsafe fn decode_online(
    device: u64,
    ty: vk::DescriptorType,
    p: *const u8,
    mut emit: impl FnMut(OnlineWrite),
) {
    if matches!(
        ty,
        vk::DescriptorType::SAMPLER | vk::DescriptorType::COMBINED_IMAGE_SAMPLER
    ) {
        let i = p.cast::<vk::DescriptorImageInfo>();
        emit(OnlineWrite::Sampler(
            std::ptr::addr_of!((*i).sampler).read_unaligned().as_raw(),
        ));
    }
    if matches!(
        ty,
        vk::DescriptorType::SAMPLED_IMAGE | vk::DescriptorType::COMBINED_IMAGE_SAMPLER
    ) {
        let i = p.cast::<vk::DescriptorImageInfo>();
        emit(OnlineWrite::Image(
            std::ptr::addr_of!((*i).image_view)
                .read_unaligned()
                .as_raw(),
            std::ptr::addr_of!((*i).image_layout)
                .read_unaligned()
                .as_raw(),
        ));
    }
    if ty == vk::DescriptorType::UNIFORM_BUFFER {
        emit(OnlineWrite::Coordinates(detail::coordinates(
            device,
            p.cast::<vk::DescriptorBufferInfo>().read_unaligned(),
        )));
    }
}
unsafe fn online_template_write(
    device: u64,
    set: vk::DescriptorSet,
    t: vk::DescriptorUpdateTemplate,
    p: *const std::ffi::c_void,
) {
    let guard = templates();
    let Some(entries) = guard.get(&(device, t.as_raw())) else {
        crate::source_auto::update(|m| m.invalid = true);
        return;
    };
    // Decode while memory is mapped, then update the model once. No array-tail
    // copies, JSON values or descriptor-template cloning on every update.
    crate::source_auto::update(|m| {
        for e in entries {
            decode_online(
                device,
                e.descriptor_type,
                p.cast::<u8>().add(e.offset),
                |w| apply_online_write(m, device, set.as_raw(), w),
            );
        }
    });
}
unsafe fn online_writes(device: u64, writes: &[vk::WriteDescriptorSet], copies: u32) {
    crate::source_auto::update(|m| {
        if copies != 0 {
            m.invalid = true;
            return;
        }
        for w in writes {
            let entry = vk::DescriptorUpdateTemplateEntry::default()
                .dst_binding(w.dst_binding)
                .dst_array_element(w.dst_array_element)
                .descriptor_count(w.descriptor_count)
                .descriptor_type(w.descriptor_type);
            if !online_entry(&entry) {
                continue;
            }
            let p = if w.descriptor_type == vk::DescriptorType::UNIFORM_BUFFER {
                w.p_buffer_info.cast::<u8>()
            } else {
                w.p_image_info.cast::<u8>()
            };
            if p.is_null() {
                m.invalid = true;
                return;
            }
            decode_online(device, w.descriptor_type, p, |value| {
                apply_online_write(m, device, w.dst_set.as_raw(), value)
            });
        }
    });
}

#[cfg(test)]
mod online_descriptor_tests {
    use super::*;
    #[test]
    fn unaligned_sampled_image_does_not_overwrite_separate_sampler() {
        let mut m = crate::source_model::Model::default();
        apply_online_write(&mut m, 1, 2, OnlineWrite::Sampler(77));
        let mut bytes = [0u8; 64];
        unsafe {
            let p = bytes.as_mut_ptr().add(1).cast::<vk::DescriptorImageInfo>();
            p.write_unaligned(
                vk::DescriptorImageInfo::default()
                    .sampler(vk::Sampler::from_raw(999))
                    .image_view(vk::ImageView::from_raw(42))
                    .image_layout(vk::ImageLayout::GENERAL),
            );
            decode_online(1, vk::DescriptorType::SAMPLED_IMAGE, p.cast(), |w| {
                apply_online_write(&mut m, 1, 2, w)
            });
        }
        let d = m.descriptors[&(1, 2)];
        assert_eq!((d.sampler, d.view, d.layout), (77, 42, 1));
    }
    #[test]
    fn coordinate_snapshot_is_bounded_and_invalid_write_clears_old_value() {
        let device = 987654;
        let bytes: [u8; 16] = [0., 1., 1., 0.]
            .map(f32::to_le_bytes)
            .concat()
            .try_into()
            .unwrap();
        {
            let mut memory = detail::memory();
            memory.buffers.insert((device, 1), 16);
            memory.bindings.insert((device, 1), (2, 0));
            memory
                .maps
                .insert((device, 2), (bytes.as_ptr() as usize, 0, 16));
        }
        let mut m = crate::source_model::Model::default();
        let mut info = vk::DescriptorBufferInfo::default()
            .buffer(vk::Buffer::from_raw(1))
            .range(16);
        unsafe {
            decode_online(
                device,
                vk::DescriptorType::UNIFORM_BUFFER,
                (&info as *const vk::DescriptorBufferInfo).cast(),
                |w| apply_online_write(&mut m, device, 3, w),
            );
            assert_eq!(m.descriptors[&(device, 3)].uv, Some([0., 1., 1., 0.]));
            info.offset = 1;
            decode_online(
                device,
                vk::DescriptorType::UNIFORM_BUFFER,
                (&info as *const vk::DescriptorBufferInfo).cast(),
                |w| apply_online_write(&mut m, device, 3, w),
            );
        }
        assert_eq!(m.descriptors[&(device, 3)].uv, None);
        let mut memory = detail::memory();
        memory.buffers.remove(&(device, 1));
        memory.bindings.remove(&(device, 1));
        memory.maps.remove(&(device, 2));
    }
}
