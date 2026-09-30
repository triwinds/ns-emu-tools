//! Typed online state. No serialization or diagnostic event reconstruction.
use crate::source_auto::Source;
#[cfg(test)]
#[path = "source_model_tests.rs"]
mod tests;
use ash::vk::{self, Handle};
use std::collections::HashMap;
type Key = (u64, u64);
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Image {
    pub extent: [u32; 3],
    pub format: i32,
    pub usage: u32,
    pub mips: u32,
    pub layers: u32,
    pub samples: u32,
    pub generation: u64,
}
#[derive(Clone, Copy, Default)]
pub(crate) struct View {
    pub image: u64,
    pub format: i32,
    pub mip: u32,
    pub layer: u32,
}
#[derive(Clone, Default)]
pub(crate) struct Framebuffer {
    pub views: Vec<u64>,
    pub size: [u32; 2],
}
#[derive(Clone, Copy, Default)]
pub(crate) struct Descriptor {
    pub view: u64,
    pub layout: i32,
    pub sampler: u64,
    pub uv: Option<[f32; 4]>,
}
#[derive(Clone, Copy, Default)]
struct Barrier {
    seq: u64,
    valid_source: bool,
}
#[derive(Clone)]
struct Draw {
    source: Result<Source, &'static str>,
    image: Option<Image>,
    destination: u64,
    ended: bool,
    presented: bool,
    source_changed: bool,
}
#[derive(Default)]
struct Command {
    framebuffer: u64,
    renderpass: u64,
    pipeline: u64,
    sets: [u64; 3],
    viewport: Option<[f32; 4]>,
    scissor: Option<[i64; 4]>,
    clear: bool,
    ended: bool,
    barriers: HashMap<u64, Barrier>,
    draws: Vec<Draw>,
}
struct Submitted {
    device: u64,
    queue: u64,
    seq: u64,
    last: bool,
    signals: Vec<u64>,
    draw: Draw,
}
#[derive(Default)]
pub(crate) struct Model {
    pub images: HashMap<Key, Image>,
    pub views: HashMap<Key, View>,
    pub framebuffers: HashMap<Key, Framebuffer>,
    pub chains: HashMap<Key, Vec<u64>>,
    pub samplers: HashMap<Key, bool>,
    pub shaders: HashMap<Key, u8>,
    pub pipelines: HashMap<Key, bool>,
    pub renderpasses: HashMap<Key, bool>,
    pub descriptors: HashMap<Key, Descriptor>,
    commands: HashMap<Key, Command>,
    submitted: Vec<Submitted>,
    last_submit: HashMap<u64, u64>,
    pub invalid: bool,
    pub serial: u64,
}
impl Model {
    pub fn clear_frame_state(&mut self) {
        self.commands.clear();
        self.descriptors.clear();
        self.submitted.clear();
        self.last_submit.clear();
    }
    pub fn within_budget(&self) -> bool {
        self.images.len()
            + self.views.len()
            + self.framebuffers.len()
            + self.descriptors.len()
            + self.commands.len()
            + self.samplers.len()
            + self.shaders.len()
            + self.pipelines.len()
            + self.renderpasses.len()
            < 65536
            && self.submitted.len() < 256
            && self
                .commands
                .values()
                .all(|c| c.draws.len() < 64 && c.barriers.len() < 65536)
    }
    pub fn begin(&mut self, device: u64, command: u64, success: bool) {
        self.commands.remove(&(device, command));
        if success {
            self.commands.insert((device, command), Command::default());
        }
    }
    pub fn end(&mut self, device: u64, command: u64, success: bool) {
        self.commands.entry((device, command)).or_default().ended = success;
    }
    pub fn pipeline(&mut self, d: u64, c: u64, p: u64) {
        self.commands.entry((d, c)).or_default().pipeline = p;
    }
    pub fn sets(&mut self, d: u64, c: u64, first: u32, sets: &[vk::DescriptorSet]) {
        let command = self.commands.entry((d, c)).or_default();
        for (index, set) in sets.iter().enumerate() {
            if let Some(slot) = command.sets.get_mut(first as usize + index) {
                *slot = set.as_raw();
            }
        }
    }
    pub fn viewport(&mut self, d: u64, c: u64, first: u32, v: &[vk::Viewport]) {
        let command = self.commands.entry((d, c)).or_default();
        command.viewport = if first == 0 {
            v.first().map(|v| [v.x, v.y, v.width, v.height])
        } else {
            None
        };
    }
    pub fn scissor(&mut self, d: u64, c: u64, first: u32, v: &[vk::Rect2D]) {
        self.commands.entry((d, c)).or_default().scissor = if first == 0 {
            v.first().map(|v| {
                [
                    v.offset.x as i64,
                    v.offset.y as i64,
                    v.extent.width as i64,
                    v.extent.height as i64,
                ]
            })
        } else {
            None
        };
    }
    pub fn begin_pass(&mut self, d: u64, c: u64, framebuffer: u64, renderpass: u64) {
        let command = self.commands.entry((d, c)).or_default();
        command.framebuffer = framebuffer;
        command.renderpass = renderpass;
        command.clear = false;
    }
    pub fn end_pass(&mut self, d: u64, c: u64) {
        let command = self.commands.entry((d, c)).or_default();
        for draw in &mut command.draws {
            draw.ended = true;
        }
        command.framebuffer = 0;
    }
    pub unsafe fn clear(
        &mut self,
        d: u64,
        c: u64,
        a: &[vk::ClearAttachment],
        rects: &[vk::ClearRect],
    ) {
        let command = self.commands.entry((d, c)).or_default();
        if let Some(fb) = self.framebuffers.get(&(d, command.framebuffer)) {
            command.clear |= a.iter().any(|a| {
                a.aspect_mask == vk::ImageAspectFlags::COLOR
                    && a.color_attachment == 0
                    && a.clear_value.color.float32 == [0., 0., 0., 1.]
            }) && rects.iter().any(|r| {
                r.rect.offset == vk::Offset2D { x: 0, y: 0 }
                    && [r.rect.extent.width, r.rect.extent.height] == fb.size
            });
        }
    }
    pub fn barriers(&mut self, d: u64, c: u64, barriers: &[vk::ImageMemoryBarrier]) {
        let command = self.commands.entry((d, c)).or_default();
        for b in barriers {
            let image = b.image.as_raw();
            command.barriers.insert(
                image,
                Barrier {
                    seq: self.serial,
                    valid_source: b.new_layout == vk::ImageLayout::GENERAL
                        && b.src_queue_family_index == u32::MAX
                        && b.dst_queue_family_index == u32::MAX
                        && b.subresource_range.aspect_mask == vk::ImageAspectFlags::COLOR
                        && b.subresource_range.base_mip_level == 0
                        && b.subresource_range.base_array_layer == 0,
                },
            );
            for draw in &mut command.draws {
                if draw.source.is_ok_and(|s| s.image.as_raw() == image) {
                    draw.source_changed = true;
                }
                if draw.destination == image
                    && b.old_layout == vk::ImageLayout::GENERAL
                    && b.new_layout == vk::ImageLayout::PRESENT_SRC_KHR
                {
                    draw.presented = true;
                }
            }
        }
    }
    pub fn draw(&mut self, d: u64, c: u64, indexed: bool, args: [u32; 4]) {
        let Some(command) = self.commands.get(&(d, c)) else {
            return;
        };
        let Some(fb) = self.framebuffers.get(&(d, command.framebuffer)) else {
            return;
        };
        let destinations: Vec<_> = fb
            .views
            .iter()
            .filter_map(|v| self.views.get(&(d, *v)))
            .map(|v| v.image)
            .filter(|image| {
                self.chains
                    .iter()
                    .any(|((dev, _), images)| *dev == d && images.contains(image))
            })
            .collect();
        if destinations.is_empty() {
            return;
        }
        let source = (|| {
            if self.pipelines.get(&(d, command.pipeline)) != Some(&true) {
                return Err("缩放着色器不匹配");
            }
            if self.renderpasses.get(&(d, command.renderpass)) != Some(&true) {
                return Err("渲染通道布局不匹配");
            }
            if indexed || args != [4, 1, 0, 0] {
                return Err("缩放坐标或绘制参数不匹配");
            }
            let descriptor = self
                .descriptors
                .get(&(d, command.sets[2]))
                .ok_or("源纹理缺失")?;
            let view = self.views.get(&(d, descriptor.view)).ok_or("源纹理缺失")?;
            let image = self.images.get(&(d, view.image)).ok_or("源纹理缺失")?;
            if !matches!(image.format, 37 | 43)
                || view.format != 37
                || image.samples != 1
                || view.mip != 0
                || view.layer != 0
            {
                return Err("源纹理格式或绑定不匹配");
            }
            if self.samplers.get(&(d, descriptor.sampler)) != Some(&true) {
                return Err("当前采样方式不匹配");
            }
            if view.image == 0
                || image.usage & 1 == 0
                || image.layers != 1
                || image.mips != 1
                || image.extent[2] != 1
                || descriptor.layout != 1
            {
                return Err("源纹理不支持安全读取");
            }
            if !command
                .barriers
                .get(&view.image)
                .is_some_and(|b| b.valid_source && b.seq < self.serial)
            {
                return Err("源纹理同步条件不匹配");
            }
            let uv = self
                .descriptors
                .get(&(d, command.sets[0]))
                .and_then(|v| v.uv)
                .ok_or("缩放坐标或绘制参数不匹配")?;
            if uv.iter().any(|v| *v != 0. && *v != 1.) || uv[0] == uv[1] || uv[2] == uv[3] {
                return Err("暂不支持源画面裁剪");
            }
            if [image.extent[0], image.extent[1], fb.size[0], fb.size[1]]
                .iter()
                .any(|v| !(1..=8192).contains(v))
            {
                return Err("画面尺寸超出支持范围");
            }
            let viewport = command.viewport.ok_or("输出区域不匹配")?;
            if viewport.iter().any(|v| !v.is_finite() || *v < 0.)
                || viewport[2] < 1.
                || viewport[3] < 1.
                || viewport[0] + viewport[2] > fb.size[0] as f32
                || viewport[1] + viewport[3] > fb.size[1] as f32
            {
                return Err("输出区域不匹配");
            }
            if !command.clear {
                return Err("黑边清除条件不匹配");
            }
            if command.scissor != Some([0, 0, fb.size[0] as i64, fb.size[1] as i64]) {
                return Err("裁剪区域不匹配");
            }
            Ok(Source {
                image: vk::Image::from_raw(view.image),
                raw_copy: image.format == 43,
                viewport,
                offsets: [
                    vk::Offset3D {
                        x: (uv[0] * image.extent[0] as f32) as i32,
                        y: (uv[2] * image.extent[1] as f32) as i32,
                        z: 0,
                    },
                    vk::Offset3D {
                        x: (uv[1] * image.extent[0] as f32) as i32,
                        y: (uv[3] * image.extent[1] as f32) as i32,
                        z: 1,
                    },
                ],
            })
        })();
        let image = source
            .ok()
            .and_then(|s| self.images.get(&(d, s.image.as_raw())).copied());
        for destination in destinations {
            self.commands.get_mut(&(d, c)).unwrap().draws.push(Draw {
                source,
                image,
                destination,
                ended: false,
                presented: false,
                source_changed: false,
            });
        }
    }
    pub unsafe fn submit(&mut self, d: u64, q: u64, submits: &[vk::SubmitInfo], success: bool) {
        if !success {
            return;
        }
        self.last_submit.insert(d, self.serial);
        for submit in submits {
            let commands =
                super::scale_probe::items(submit.p_command_buffers, submit.command_buffer_count);
            for (index, c) in commands.iter().enumerate() {
                if let Some(command) = self.commands.get(&(d, c.as_raw())) {
                    for draw in &command.draws {
                        let mut draw = draw.clone();
                        if !command.ended {
                            draw.source = Err("命令缓冲区未结束");
                        }
                        self.submitted.push(Submitted {
                            device: d,
                            queue: q,
                            seq: self.serial,
                            last: index + 1 == commands.len(),
                            signals: super::scale_probe::items(
                                submit.p_signal_semaphores,
                                submit.signal_semaphore_count,
                            )
                            .iter()
                            .map(|s| s.as_raw())
                            .collect(),
                            draw,
                        });
                    }
                }
            }
        }
    }
    pub fn select(
        &mut self,
        d: u64,
        q: u64,
        chain: u64,
        index: u32,
        waits: &[vk::Semaphore],
    ) -> Result<Source, &'static str> {
        let image = self
            .chains
            .get(&(d, chain))
            .and_then(|images| images.get(index as usize))
            .copied();
        let result = (|| {
            let mut candidates = self.submitted.iter().filter(|s| {
                s.device == d
                    && s.queue == q
                    && Some(s.draw.destination) == image
                    && s.signals
                        .iter()
                        .any(|signal| waits.iter().any(|w| w.as_raw() == *signal))
            });
            let candidate = candidates.next().ok_or("未找到缩放绘制")?;
            if candidates.next().is_some() {
                return Err("缩放绘制不唯一");
            }
            if Some(&candidate.seq) != self.last_submit.get(&d) || !candidate.last {
                return Err("源画面后还有其他 GPU 提交");
            }
            let draw = &candidate.draw;
            let source = draw.source?;
            if self.images.get(&(d, source.image.as_raw())) != draw.image.as_ref() {
                return Err("源纹理已变化");
            }
            if !draw.ended {
                return Err("缺少渲染通道结束记录");
            }
            if !draw.presented {
                return Err("缺少呈现布局转换");
            }
            if draw.source_changed {
                return Err("源纹理同步条件不匹配");
            }
            Ok(source)
        })();
        self.submitted.retain(|s| s.device != d || s.queue != q);
        result
    }
}
