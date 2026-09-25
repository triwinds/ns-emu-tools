//! Host-owned empty work, using the SDK function table and the reserved host queue.
use crate::host::{write_json, Result};
use ash::vk::{self, Handle};
use serde_json::{json, Value};
use std::path::Path;

pub(super) unsafe fn exercise(device: &ash::Device, family: u32) -> Result<Vec<Value>> {
    let queue = device.get_device_queue(family, 0);
    let queue2 = device.get_device_queue2(
        &vk::DeviceQueueInfo2::default()
            .queue_family_index(family)
            .queue_index(0),
    );
    if queue == vk::Queue::null() || queue != queue2 {
        return Err("queue identity mismatch".into());
    }
    let mut rows = Vec::new();
    for cycle in 0..2 {
        let pool = device.create_command_pool(
            &vk::CommandPoolCreateInfo::default().queue_family_index(family),
            None,
        )?;
        // Error paths abort the isolated child: work might still be pending, so
        // do not unwind into SDK shutdown/device destruction without quiescence.
        let run = || -> Result<Value> {
            let buffers = device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )?;
            let command = buffers[0];
            device.begin_command_buffer(
                command,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
            device.end_command_buffer(command)?;
            let fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
            device.queue_submit(
                queue,
                &[vk::SubmitInfo::default().command_buffers(&buffers)],
                fence,
            )?;
            device.wait_for_fences(&[fence], true, 5_000_000_000)?;
            device.queue_wait_idle(queue)?;
            device.destroy_fence(fence, None);
            device.free_command_buffers(pool, &buffers);
            Ok(
                json!({"cycle":cycle, "queue":queue.as_raw(), "command":command.as_raw(),
                "fence_completed":true, "command_freed":true}),
            )
        };
        let row = match run() {
            Ok(row) => row,
            Err(error) => {
                eprintln!("command lifecycle failed: {error}");
                std::process::abort();
            }
        };
        device.destroy_command_pool(pool, None);
        rows.push(row);
    }
    Ok(rows)
}

pub(super) fn validate(events: &[Value]) -> Result<()> {
    for (name, expected) in [
        ("vkGetDeviceQueue", 1),
        ("vkGetDeviceQueue2", 1),
        ("vkAllocateCommandBuffers", 2),
        ("vkBeginCommandBuffer", 2),
        ("vkEndCommandBuffer", 2),
        ("vkQueueSubmit", 2),
        ("vkQueueWaitIdle", 2),
    ] {
        for (phase, count) in [(15, expected), (16, 0)] {
            if events
                .iter()
                .filter(|v| v["event"] == name && v["phase"] == phase)
                .count()
                != count
            {
                return Err(
                    format!("command route evidence mismatch: {name}, phase {phase}").into(),
                );
            }
        }
    }
    let initialized: Vec<_> = events
        .iter()
        .filter(|v| v["event"] == "route_loader_data" && v["phase"] == 16)
        .collect();
    if initialized.len() != 4
        || initialized
            .iter()
            .any(|v| v["details"]["result"] != 0 || v["details"]["dispatch_matches"] != true)
    {
        return Err("missing successful Loader object initialization".into());
    }
    Ok(())
}

pub(super) fn report(session: &Path, application: Vec<Value>, sdk: Vec<Value>) -> Result<()> {
    write_json(
        &session.join("sdk-command-calls.json"),
        &json!({
            "application":application, "sdk":sdk, "queue_index":0,
            "owner":"diagnostic host", "sdk_owned_worker_tested":false, "fg_enabled":false
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence() -> Vec<Value> {
        let mut rows = Vec::new();
        for (name, n) in [
            ("vkGetDeviceQueue", 1),
            ("vkGetDeviceQueue2", 1),
            ("vkAllocateCommandBuffers", 2),
            ("vkBeginCommandBuffer", 2),
            ("vkEndCommandBuffer", 2),
            ("vkQueueSubmit", 2),
            ("vkQueueWaitIdle", 2),
        ] {
            for _ in 0..n {
                rows.push(json!({"event":name, "phase":15}));
            }
        }
        for _ in 0..4 {
            rows.push(json!({"event":"route_loader_data", "phase":16,
                "details":{"result":0,"dispatch_matches":true}}));
        }
        rows
    }

    #[test]
    fn rejects_uninitialized_objects_reentry_and_missing_control() {
        let rows = evidence();
        assert!(validate(&rows).is_ok());
        let mut broken = rows.clone();
        broken.last_mut().unwrap()["details"]["dispatch_matches"] = json!(false);
        assert!(validate(&broken).is_err());
        let mut broken = rows.clone();
        broken.last_mut().unwrap()["details"]["result"] = json!(-3);
        assert!(validate(&broken).is_err());
        let mut broken = rows.clone();
        broken.push(json!({"event":"vkQueueSubmit","phase":16}));
        assert!(validate(&broken).is_err());
        assert!(validate(&rows[1..]).is_err());
        assert!(validate(&rows[..rows.len() - 1]).is_err());
    }
}
