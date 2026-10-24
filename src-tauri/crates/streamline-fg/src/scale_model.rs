//! Diagnostic reconstruction only; never authorizes GPU replacement.
use ash::vk;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

#[allow(dead_code)] // The diagnostic launcher shares this model without online hooks.
pub(crate) enum FastCommand<'a> {
    Pipeline(u64),
    Sets(u32, &'a [u64]),
    Viewport(u32, &'a [vk::Viewport]),
    Scissor(u32, &'a [vk::Rect2D]),
    Draw(bool, [u32; 4]),
    Barriers(&'a [vk::ImageMemoryBarrier<'a>]),
}
struct Barrier {
    seq: u64,
    image: u64,
    old: i32,
    new: i32,
    src_family: u32,
    dst_family: u32,
    aspect: u32,
    mip: u32,
    layer: u32,
}
type Key = (u64, u64);
#[derive(Default)]
struct Command {
    framebuffer: u64,
    renderpass: u64,
    pipeline: u64,
    ended: bool,
    generation: u64,
    operations: Vec<Value>,
    barriers: Vec<Barrier>,
    sets: BTreeMap<u64, u64>,
    viewport: Value,
    scissor: Value,
    draws: Vec<Value>,
}
fn num(v: &Value, k: &str) -> u64 {
    v[k].as_u64().unwrap_or(0)
}
fn arr(v: &Value) -> &[Value] {
    v.as_array().map_or(&[], Vec::as_slice)
}
fn ids(v: &Value) -> Vec<u64> {
    arr(v).iter().filter_map(Value::as_u64).collect()
}
const BLIT_VERTEX: &str = "b559ef9b2f2c0796bd8e9706595690edc091fcbee127832ab5a91e48c9cd8625";
const BLIT_ALPHA: &str = "c51c90364dfef8663959e0fa05f48e2142507973722a527627ddb06eb9783b39";
fn coordinates(v: &Value) -> Option<[f32; 4]> {
    let bytes: Vec<u8> = arr(v)
        .iter()
        .map(|b| b.as_u64().and_then(|b| u8::try_from(b).ok()))
        .collect::<Option<_>>()?;
    if bytes.len() != 16 {
        return None;
    }
    let value =
        std::array::from_fn(|i| f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()));
    (value
        .iter()
        .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        && value[0] != value[1]
        && value[2] != value[3])
        .then_some(value)
}
fn contract(draw: &Value) -> Value {
    let stages = arr(&draw["pipeline"]["stages"]);
    let shader_match = stages.len() == 2
        && stages.iter().any(|s| {
            s["stage"] == 1 && s["shader"]["sha256"] == BLIT_VERTEX && s["entry"] == "main"
        })
        && stages.iter().any(|s| {
            s["stage"] == 16 && s["shader"]["sha256"] == BLIT_ALPHA && s["entry"] == "main"
        });
    let uv = arr(&draw["parameters"])
        .iter()
        .find(|p| {
            p["slot"] == 0
                && p["binding"] == 1
                && p["element"] == 0
                && p["buffer"]["descriptor_type"] == 6
        })
        .and_then(|p| coordinates(&p["buffer"]["host_snapshot"]));
    let sources = arr(&draw["sources"]);
    let source_match = sources.len() == 1
        && sources[0]["slot"] == 2
        && sources[0]["binding"] == 0
        && sources[0]["element"] == 0
        && matches!(sources[0]["image"]["format"].as_u64(), Some(37 | 43))
        && sources[0]["view"]["format"] == 37
        && sources[0]["image"]["samples"] == 1
        && sources[0]["view"]["base_mip"] == 0
        && sources[0]["view"]["base_layer"] == 0;
    let attachments = arr(&draw["renderpass"]["attachments"]);
    let general_pass =
        attachments.len() == 1 && attachments[0]["initial"] == 1 && attachments[0]["final"] == 1;
    let post_pass = arr(&draw["operations"])
        .iter()
        .any(|o| o["call"] == "vkCmdEndRenderPass" && num(o, "seq") > num(draw, "seq"));
    let destination = arr(&draw["destinations"]).first().and_then(Value::as_u64);
    let present_barrier = arr(&draw["operations"]).iter().any(|o| {
        num(o, "seq") > num(draw, "seq")
            && arr(&o["data"]["images"]).iter().any(|b| {
                Some(num(b, "image")) == destination && b["old"] == 1 && b["new"] == 1000001002
            })
    });
    let basic = shader_match
        && uv.is_some()
        && source_match
        && general_pass
        && post_pass
        && present_barrier
        && draw["end_command_buffer_succeeded"] == true
        && draw["draw"]["vertices"] == 4
        && draw["draw"]["instances"] == 1;
    json!({"known_blit_shaders":shader_match,"normalized_source_endpoints":uv,"source_matches_observed_profile":source_match,"renderpass_preserves_general":general_pass,"renderpass_ends_after_draw":post_pass,"destination_transitions_to_present":present_barrier,"recorded_contract_matches":basic,"gpu_replacement_safe":false,"reason":"CPU snapshots at descriptor update and observed command structure only; no complete access/lifetime tracking or GPU replacement validation"})
}
#[derive(Default)]
pub(crate) struct Model {
    samplers: HashMap<Key, Value>,
    sampler_sets: HashMap<Key, BTreeMap<(u64, u64), u64>>,
    images: HashMap<Key, Value>,
    views: HashMap<Key, Value>,
    fbs: HashMap<Key, Value>,
    chains: HashMap<Key, Vec<u64>>,
    descriptors: HashMap<Key, BTreeMap<(u64, u64), Value>>,
    commands: HashMap<Key, Command>,
    submitted: Vec<Value>,
    frames: Vec<Value>,
    unknown_templates: usize,
    descriptor_copies: usize,
    shaders: HashMap<Key, Value>,
    pipelines: HashMap<Key, Value>,
    renderpasses: HashMap<Key, Value>,
    uniforms: HashMap<Key, BTreeMap<(u64, u64), Value>>,
}
impl Model {
    #[allow(dead_code)]
    pub(crate) fn fast(&mut self, device: u64, object: u64, seq: u64, event: FastCommand<'_>) {
        use ash::vk::Handle;
        let c = self.commands.entry((device, object)).or_default();
        match event {
            FastCommand::Pipeline(p) => c.pipeline = p,
            FastCommand::Sets(first, sets) => {
                for (i, set) in sets.iter().enumerate() {
                    c.sets.insert(first as u64 + i as u64, *set);
                }
            }
            FastCommand::Viewport(first, v) => {
                c.viewport = json!({"first":first,"viewports":v.iter().map(|v|[v.x,v.y,v.width,v.height,v.min_depth,v.max_depth]).collect::<Vec<_>>()})
            }
            FastCommand::Scissor(first, v) => {
                c.scissor = json!({"first":first,"scissors":v.iter().map(|v|[v.offset.x,v.offset.y,v.extent.width as i32,v.extent.height as i32]).collect::<Vec<_>>()})
            }
            FastCommand::Barriers(images) => {
                c.barriers.extend(images.iter().map(|b| Barrier {
                    seq,
                    image: b.image.as_raw(),
                    old: b.old_layout.as_raw(),
                    new: b.new_layout.as_raw(),
                    src_family: b.src_queue_family_index,
                    dst_family: b.dst_queue_family_index,
                    aspect: b.subresource_range.aspect_mask.as_raw(),
                    mip: b.subresource_range.base_mip_level,
                    layer: b.subresource_range.base_array_layer,
                }));
            }
            FastCommand::Draw(indexed, p) => {
                let output = self.fbs.get(&(device, c.framebuffer)).is_some_and(|fb| {
                    ids(&fb["views"]).iter().any(|v| {
                        self.views.get(&(device, *v)).is_some_and(|v| {
                            self.chains.iter().any(|((dev, _), images)| {
                                *dev == device && images.contains(&num(v, "image"))
                            })
                        })
                    })
                });
                if output {
                    self.observe(&json!({"device":device,"object":object,"seq":seq,"call":if indexed {"vkCmdDrawIndexed"}else{"vkCmdDraw"},"data":if indexed {json!({"indices":p[0],"instances":p[1],"first_index":p[2],"first_instance":p[3]})}else{json!({"vertices":p[0],"instances":p[1],"first_vertex":p[2],"first_instance":p[3]})}}));
                }
            }
        }
    }
    #[allow(dead_code)] // Online tracker only; the launcher shares the offline model.
    pub(crate) fn clear_frame_state(&mut self) {
        self.commands.clear();
        self.descriptors.clear();
        self.sampler_sets.clear();
        self.uniforms.clear();
        self.submitted.clear();
        self.frames.clear();
    }
    pub(crate) fn observe(&mut self, r: &Value) {
        let Self {
            samplers,
            sampler_sets,
            images,
            views,
            fbs,
            chains,
            descriptors,
            commands,
            submitted,
            frames,
            unknown_templates,
            descriptor_copies,
            shaders,
            pipelines,
            renderpasses,
            uniforms,
        } = self;
        for r in std::iter::once(r) {
            let device = num(r, "device");
            let object = num(r, "object");
            let d = &r["data"];
            let key = (device, object);
            match r["call"].as_str().unwrap_or("") {
                "vkCreateSampler" if d["sampler"].is_u64() => {
                    samplers.insert((device, num(d, "sampler")), d.clone());
                }
                "vkDestroySampler" => {
                    samplers.remove(&(device, num(d, "sampler")));
                }

                "vkCreateShaderModule" if d["module"].is_u64() => {
                    shaders.insert((device, num(d, "module")), d.clone());
                }
                "vkCreateGraphicsPipelines" => {
                    for p in arr(&d["pipelines"]) {
                        let mut p = p.clone();
                        for stage in p["stages"].as_array_mut().into_iter().flatten() {
                            stage["shader"] = shaders
                                .get(&(device, num(stage, "module")))
                                .cloned()
                                .unwrap_or(Value::Null);
                        }
                        pipelines.insert((device, num(&p, "pipeline")), p);
                    }
                }
                "vkCreateRenderPass" if d["renderpass"].is_u64() => {
                    renderpasses.insert((device, num(d, "renderpass")), d.clone());
                }
                "vkDestroyImage" => {
                    images.remove(&(device, num(d, "image")));
                }
                "vkDestroyImageView" => {
                    views.remove(&(device, num(d, "view")));
                }
                "vkDestroyFramebuffer" => {
                    fbs.remove(&(device, num(d, "framebuffer")));
                }
                "vkCreateImage" if d["image"].is_u64() => {
                    let mut image = d.clone();
                    image["created_seq"] = r["seq"].clone();
                    images.insert((device, num(d, "image")), image);
                }
                "vkCreateImageView" if d["view"].is_u64() => {
                    views.insert((device, num(d, "view")), d.clone());
                }
                "vkCreateFramebuffer" if d["framebuffer"].is_u64() => {
                    fbs.insert((device, num(d, "framebuffer")), d.clone());
                }
                "vkGetSwapchainImagesKHR" if !arr(&d["images"]).is_empty() => {
                    chains.insert((device, num(d, "chain")), ids(&d["images"]));
                }
                "vkUpdateDescriptorSets"
                | "vkUpdateDescriptorSetWithTemplate"
                | "vkUpdateDescriptorSetWithTemplateKHR" => {
                    if d["unknown_template"].is_u64() {
                        *unknown_templates += 1;
                    }
                    *descriptor_copies += arr(&d["copies"]).len();
                    for w in arr(&d["writes"]) {
                        for (index, sampler) in ids(&w["samplers"]).iter().enumerate() {
                            sampler_sets
                                .entry((device, num(w, "set")))
                                .or_default()
                                .insert(
                                    (num(w, "binding"), num(w, "element") + index as u64),
                                    *sampler,
                                );
                        }

                        for (index, b) in arr(&w["buffers"]).iter().enumerate() {
                            let mut b = b.clone();
                            b["descriptor_type"] = w["type"].clone();
                            uniforms
                                .entry((device, num(w, "set")))
                                .or_default()
                                .insert((num(w, "binding"), num(w, "element") + index as u64), b);
                        }
                        let set = descriptors.entry((device, num(w, "set"))).or_default();
                        // Only sampled image descriptors are source candidates.
                        if ![1, 2].contains(&num(w, "type")) {
                            continue;
                        }
                        for (index, image) in arr(&w["images"]).iter().enumerate() {
                            set.insert(
                                (num(w, "binding"), num(w, "element") + index as u64),
                                image.clone(),
                            );
                        }
                    }
                }
                "vkBeginCommandBuffer" | "vkResetCommandBuffer"
                    if d["result"].as_i64() == Some(0) =>
                {
                    let generation = commands.get(&key).map_or(1, |c| c.generation + 1);
                    commands.insert(
                        key,
                        Command {
                            generation,
                            ..Default::default()
                        },
                    );
                }
                "vkEndCommandBuffer" => {
                    commands.entry(key).or_default().ended = d["result"].as_i64() == Some(0);
                }
                "vkCmdBindPipeline" if num(d, "bind_point") == 0 => {
                    commands.entry(key).or_default().pipeline = num(d, "pipeline")
                }
                "vkCmdPipelineBarrier" | "vkCmdClearAttachments" => commands
                    .entry(key)
                    .or_default()
                    .operations
                    .push(json!({"seq":r["seq"],"call":r["call"],"data":d})),
                "vkCmdBeginRenderPass" => {
                    let c = commands.entry(key).or_default();
                    c.framebuffer = num(d, "framebuffer");
                    c.renderpass = num(d, "renderpass");
                    c.operations
                        .push(json!({"seq":r["seq"],"call":r["call"],"data":d}));
                }
                "vkCmdEndRenderPass" => {
                    let c = commands.entry(key).or_default();
                    c.framebuffer = 0;
                    c.operations
                        .push(json!({"seq":r["seq"],"call":r["call"],"data":d}));
                }
                "vkCmdBindDescriptorSets" if num(d, "bind_point") == 0 => {
                    let c = commands.entry(key).or_default();
                    for (i, s) in ids(&d["sets"]).into_iter().enumerate() {
                        c.sets.insert(num(d, "first") + i as u64, s);
                    }
                }
                "vkCmdSetViewport" => commands.entry(key).or_default().viewport = d.clone(),
                "vkCmdSetScissor" => commands.entry(key).or_default().scissor = d.clone(),
                "vkCmdDraw" | "vkCmdDrawIndexed" => {
                    let c = commands.entry(key).or_default();
                    if let Some(fb) = fbs.get(&(device, c.framebuffer)) {
                        let destinations: Vec<_> = ids(&fb["views"])
                            .into_iter()
                            .filter_map(|v| views.get(&(device, v)))
                            .map(|v| num(v, "image"))
                            .filter(|image| {
                                chains.iter().any(|((dev, _), images)| {
                                    *dev == device && images.contains(image)
                                })
                            })
                            .collect();
                        if destinations.is_empty() {
                            continue;
                        }
                        let mut sampling = vec![];
                        let mut sources = vec![];
                        let mut parameters = vec![];
                        for (slot, set) in &c.sets {
                            if let Some(bindings) = sampler_sets.get(&(device, *set)) {
                                for ((binding, element), id) in bindings {
                                    sampling.push(json!({"slot":slot,"binding":binding,"element":element,"sampler":samplers.get(&(device,*id))}));
                                }
                            }

                            if let Some(bindings) = uniforms.get(&(device, *set)) {
                                for ((binding, element), b) in bindings {
                                    parameters.push(json!({"slot":slot,"binding":binding,"element":element,"buffer":b}));
                                }
                            }
                            if let Some(bindings) = descriptors.get(&(device, *set)) {
                                for ((binding, element), i) in bindings {
                                    if let Some(v) = views.get(&(device, num(i, "view"))) {
                                        if let Some(image) = images.get(&(device, num(v, "image")))
                                        {
                                            sources.push(json!({"slot":slot,"binding":binding,"element":element,"view":v,"image":image,"descriptor_layout":i["layout"]}));
                                        }
                                    }
                                }
                            }
                        }
                        c.draws.push(json!({"seq":r["seq"],"command_buffer":object,"generation":c.generation,"pipeline":pipelines.get(&(device,c.pipeline)),"renderpass":renderpasses.get(&(device,c.renderpass)),"parameters":parameters,"sampling":sampling,"destinations":destinations,"sources":sources,"viewport":c.viewport,"scissor":c.scissor,"draw":d,"output_extent":fb["extent"]}));
                    }
                }
                "vkQueueSubmit" if d["result"].as_i64() == Some(0) => {
                    for s in arr(&d["submits"]) {
                        for cb in ids(&s["commands"]) {
                            if let Some(c) = commands.get(&(device, cb)) {
                                for draw in &c.draws {
                                    let mut draw = draw.clone();
                                    draw["submit_seq"] = r["seq"].clone();
                                    draw["last_command"] =
                                        json!(ids(&s["commands"]).last() == Some(&cb));
                                    draw["operations"] = json!(c.operations);
                                    let source = num(&draw["sources"][0]["image"], "image");
                                    let destinations = ids(&draw["destinations"]);
                                    for b in &c.barriers {
                                        if b.image == source || destinations.contains(&b.image) {
                                            draw["operations"].as_array_mut().unwrap().push(json!({"seq":b.seq,"call":"vkCmdPipelineBarrier","data":{"images":[{"image":b.image,"old":b.old,"new":b.new,"src_family":b.src_family,"dst_family":b.dst_family,"aspect":b.aspect,"mip":b.mip,"layer":b.layer}]}}));
                                        }
                                    }
                                    draw["end_command_buffer_succeeded"] = json!(c.ended);
                                    draw["contract"] = contract(&draw);
                                    submitted.push(json!({"device":device,"queue":object,"signals":s["signals"],"draw":draw}));
                                }
                            }
                        }
                    }
                }
                "present" => {
                    for (index, chain) in ids(&d["chains"]).iter().enumerate() {
                        let image_index = arr(&d["indices"])
                            .get(index)
                            .and_then(Value::as_u64)
                            .unwrap_or(u64::MAX);
                        let image = chains
                            .get(&(device, *chain))
                            .and_then(|v| v.get(image_index as usize))
                            .copied();
                        let waits = ids(&d["waits"]);
                        let candidates: Vec<_> = submitted
                            .iter()
                            .filter(|s| {
                                num(s, "device") == device
                                    && num(s, "queue") == object
                                    && image.is_some_and(|i| {
                                        ids(&s["draw"]["destinations"]).contains(&i)
                                    })
                                    && ids(&s["signals"]).iter().any(|s| waits.contains(s))
                            })
                            .map(|s| s["draw"].clone())
                            .collect();
                        frames.push(
                        json!({"frame":r["frame"],"presented_image":image,"candidates":candidates}),
                    );
                    }
                    submitted.retain(|s| num(s, "device") != device || num(s, "queue") != object);
                }
                _ => {}
            }
        }
    }
    #[allow(dead_code)]
    pub(crate) fn source_alive(&self, device: u64, image: &Value) -> bool {
        self.images.get(&(device, num(image, "image"))) == Some(image)
    }
    #[allow(dead_code)]
    pub(crate) fn within_budget(&self) -> bool {
        self.images.len()
            + self.views.len()
            + self.fbs.len()
            + self.descriptors.len()
            + self.commands.len()
            + self.shaders.len()
            + self.pipelines.len()
            + self.uniforms.len()
            < 65536
            && self.submitted.len() < 256
            && self.commands.values().all(|c| {
                c.operations.len() < 4096 && c.barriers.len() < 65536 && c.draws.len() < 64
            })
    }
    #[allow(dead_code)]
    pub(crate) fn take_frames(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.frames)
    }
    #[allow(dead_code)]
    pub(crate) fn unresolved(&self) -> bool {
        self.unknown_templates != 0 || self.descriptor_copies != 0
    }
}
pub(crate) fn analyze(rows: &[Value], complete: bool) -> Value {
    let mut model = Model::default();
    for r in rows {
        model.observe(r);
    }
    let Model {
        frames,
        unknown_templates,
        descriptor_copies,
        ..
    } = model;
    let unique = frames
        .iter()
        .filter(|f| {
            let c = arr(&f["candidates"]);
            c.len() == 1 && arr(&c[0]["sources"]).len() == 1
        })
        .count();
    let matching = frames
        .iter()
        .filter(|f| {
            arr(&f["candidates"]).len() == 1
                && f["candidates"][0]["contract"]["recorded_contract_matches"] == true
        })
        .count();
    json!({"schema":2,"capture_complete":complete,"present_count":frames.len(),"unique_submitted_draw_and_source_count":unique,"recorded_contract_match_count":matching,"unknown_templates":unknown_templates,"descriptor_copies_unresolved":descriptor_copies,"replacement_verified":false,"limits":["Known shader fingerprints and CPU coordinate snapshots only; no GPU pixel validation","Legacy render passes and direct queue submit-to-present semaphore links only","No secondary command buffers, dynamic rendering, descriptor copies, pipeline-layout compatibility or post-recording descriptor updates resolved","Recorded barriers are partial evidence, not a complete access/lifetime proof","No GPU completion, image quality or performance claim"],"frames":frames})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_draw_and_barriers_preserve_source_and_order() {
        use ash::vk::Handle;
        let rows = fixture(8, 0, true);
        let mut model = Model::default();
        for r in &rows[..8] {
            model.observe(r);
        }
        model.fast(10, 6, 8, FastCommand::Sets(2, &[5]));
        let barriers = [
            vk::ImageMemoryBarrier::default()
                .image(vk::Image::from_raw(1))
                .new_layout(vk::ImageLayout::GENERAL),
            vk::ImageMemoryBarrier::default().image(vk::Image::from_raw(777)),
        ];
        model.fast(10, 6, 9, FastCommand::Barriers(&barriers));
        model.fast(10, 6, 10, FastCommand::Draw(false, [4, 1, 0, 0]));
        model.fast(10, 6, 11, FastCommand::Barriers(&barriers[..1]));
        for r in &rows[10..] {
            model.observe(r);
        }
        let frames = model.take_frames();
        let c = &frames[0]["candidates"][0];
        assert_eq!(c["sources"][0]["image"]["image"], 1);
        let b: Vec<_> = arr(&c["operations"])
            .iter()
            .filter(|o| o["call"] == "vkCmdPipelineBarrier")
            .collect();
        assert_eq!(b.len(), 2); // Unrelated image 777 is excluded; the later source barrier survives.
        assert_eq!(b[0]["seq"], 9);
        assert_eq!(b[1]["seq"], 11);
        model.clear_frame_state();
        for r in &rows[10..] {
            model.observe(r);
        }
        assert!(arr(&model.take_frames()[0]["candidates"]).is_empty());
    }
    #[test]
    fn coordinate_snapshot_preserves_crop_and_flip_rejects_invalid_data() {
        for input in [[0.0_f32, 1.0, 1.0, 0.0], [0.75, 0.25, 0.1, 0.9]] {
            let bytes: Vec<_> = input.into_iter().flat_map(f32::to_le_bytes).collect();
            assert_eq!(coordinates(&json!(bytes)), Some(input));
        }
        for input in [
            [f32::NAN, 1.0, 0.0, 1.0],
            [-0.1, 1.0, 0.0, 1.0],
            [0.0, f32::INFINITY, 0.0, 1.0],
            [0.0, 0.0, 0.0, 1.0],
        ] {
            let bytes: Vec<_> = input.into_iter().flat_map(f32::to_le_bytes).collect();
            assert_eq!(coordinates(&json!(bytes)), None);
        }
        assert_eq!(coordinates(&json!(vec![0; 15])), None);
    }
    #[test]
    fn observed_profile_never_authorizes_replacement_and_rejects_shader_drift() {
        let bytes: Vec<_> = [0.0_f32, 1.0, 1.0, 0.0]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        let d = json!({"seq":10,"pipeline":{"stages":[{"stage":1,"entry":"main","shader":{"sha256":BLIT_VERTEX}},{"stage":16,"entry":"main","shader":{"sha256":BLIT_ALPHA}}]},"parameters":[{"slot":0,"binding":1,"element":0,"buffer":{"descriptor_type":6,"host_snapshot":bytes}}],"sources":[{"slot":2,"binding":0,"element":0,"image":{"format":37,"samples":1},"view":{"format":37,"base_mip":0,"base_layer":0}}],"renderpass":{"attachments":[{"initial":1,"final":1}]},"destinations":[42],"operations":[{"call":"vkCmdEndRenderPass","seq":11},{"seq":12,"data":{"images":[{"image":42,"old":1,"new":1000001002}]}}],"end_command_buffer_succeeded":true,"draw":{"vertices":4,"instances":1}});
        assert_eq!(contract(&d)["recorded_contract_matches"], true);
        assert_eq!(contract(&d)["gpu_replacement_safe"], false);
        let mut srgb = d.clone();
        srgb["sources"][0]["image"]["format"] = json!(43);
        assert_eq!(contract(&srgb)["recorded_contract_matches"], true);
        srgb["sources"][0]["view"]["format"] = json!(43);
        assert_eq!(contract(&srgb)["recorded_contract_matches"], false);
        for (pointer, replacement) in [
            ("/pipeline/stages/1/shader/sha256", json!("changed")),
            ("/parameters/0/buffer/descriptor_type", json!(8)),
            ("/sources/0/image/format", json!(44)),
            ("/operations/1/data/images/0/new", json!(1)),
            ("/end_command_buffer_succeeded", json!(false)),
        ] {
            let mut changed = d.clone();
            *changed.pointer_mut(pointer).unwrap() = replacement;
            assert_eq!(
                contract(&changed)["recorded_contract_matches"],
                false,
                "{pointer}"
            );
        }
    }
    #[test]
    fn destruction_or_command_reset_invalidates_source_evidence() {
        for (call, data) in [
            ("vkDestroyImage", json!({"image":1})),
            ("vkDestroyImageView", json!({"view":11})),
            ("vkDestroyFramebuffer", json!({"framebuffer":3})),
        ] {
            let mut rows = fixture(8, 0, true);
            rows.insert(9, json!({"device":10,"object":10,"call":call,"data":data}));
            assert_eq!(
                analyze(&rows, true)["unique_submitted_draw_and_source_count"],
                0
            );
        }
        let mut rows = fixture(8, 0, true);
        rows.insert(
            10,
            json!({"device":10,"object":6,"call":"vkResetCommandBuffer","data":{"result":0}}),
        );
        assert_eq!(
            analyze(&rows, true)["unique_submitted_draw_and_source_count"],
            0
        );
    }
    fn fixture(signal: u64, index: u64, submit: bool) -> Vec<Value> {
        let events = vec![
            (
                "vkCreateImage",
                0,
                json!({"image":1,"extent":[1920,1080,1]}),
            ),
            ("vkCreateImageView", 0, json!({"view":11,"image":1})),
            ("vkCreateImageView", 0, json!({"view":12,"image":2})),
            (
                "vkCreateFramebuffer",
                0,
                json!({"framebuffer":3,"views":[12],"extent":[2560,1440]}),
            ),
            (
                "vkGetSwapchainImagesKHR",
                0,
                json!({"chain":4,"images":[2,99]}),
            ),
            (
                "vkUpdateDescriptorSets",
                0,
                json!({"writes":[{"set":5,"binding":0,"element":0,"type":2,"images":[{"view":11,"layout":1}]}]}),
            ),
            ("vkBeginCommandBuffer", 6, json!({"result":0})),
            ("vkCmdBeginRenderPass", 6, json!({"framebuffer":3})),
            (
                "vkCmdBindDescriptorSets",
                6,
                json!({"bind_point":0,"first":2,"sets":[5]}),
            ),
            ("vkCmdDraw", 6, json!({"vertices":4})),
            (
                if submit { "vkQueueSubmit" } else { "ignored" },
                7,
                json!({"result":0,"submits":[{"commands":[6],"signals":[signal]}]}),
            ),
            (
                "present",
                7,
                json!({"chains":[4],"indices":[index],"waits":[8]}),
            ),
        ];
        events.into_iter().map(|(call,object,data)|json!({"device":10,"object":object,"call":call,"data":data,"frame":0})).collect()
    }
    #[test]
    fn recorded_draw_requires_matching_submit_semaphore_and_presented_image() {
        let good = analyze(&fixture(8, 0, true), true);
        assert_eq!(good["unique_submitted_draw_and_source_count"], 1);
        assert_eq!(good["replacement_verified"], false);
        for rows in [
            fixture(9, 0, true),
            fixture(8, 1, true),
            fixture(8, 0, false),
        ] {
            assert_eq!(
                analyze(&rows, true)["unique_submitted_draw_and_source_count"],
                0
            );
        }
        for result in [-4, -1, 1] {
            let mut rows = fixture(8, 0, true);
            rows[10]["data"]["result"] = json!(result);
            assert_eq!(
                analyze(&rows, true)["unique_submitted_draw_and_source_count"],
                0
            );
        }
    }
}
