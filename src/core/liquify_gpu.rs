//! Reusable GPU deformation buffers, dispatched by Liquify's background worker.
use egui_wgpu::wgpu;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex, OnceLock,
};
static GPU: OnceLock<Mutex<Option<Compute>>> = OnceLock::new();
static ENABLED: AtomicBool = AtomicBool::new(true);
static BATCHES: AtomicU64 = AtomicU64::new(0);
pub fn set_enabled(value: bool) {
    ENABLED.store(value, Ordering::Relaxed);
}
pub fn completed_batches() -> u64 {
    BATCHES.load(Ordering::Relaxed)
}
pub fn available() -> bool {
    ENABLED.load(Ordering::Relaxed)
        && GPU
            .get()
            .is_some_and(|g| g.try_lock().is_ok_and(|g| g.is_some()))
}
pub fn install(device: wgpu::Device, queue: wgpu::Queue) -> Result<(), String> {
    let c = Compute::new(device, queue)?;
    *GPU.get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|e| e.to_string())? = Some(c);
    Ok(())
}
pub fn transform(points: &[[f32; 4]], params: [f32; 16]) -> Option<Vec<[f32; 2]>> {
    if !ENABLED.load(Ordering::Relaxed) || points.len() < 256 {
        return None;
    }
    let mut guard = GPU.get()?.try_lock().ok()?;
    let result = guard.as_mut()?.run(points, params);
    match result {
        Ok(v) => {
            BATCHES.fetch_add(1, Ordering::Relaxed);
            Some(v)
        }
        Err(_) => {
            *guard = None;
            None
        }
    }
}
struct Buffers {
    capacity: usize,
    input: wgpu::Buffer,
    output: wgpu::Buffer,
    params: wgpu::Buffer,
    readback: wgpu::Buffer,
}
pub struct Compute {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    buffers: Option<Buffers>,
}
impl Compute {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Result<Self, String> {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Liquify deformation"),
            source: wgpu::ShaderSource::Wgsl(include_str!("liquify.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Liquify modes"),
            layout: None,
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        if let Some(e) = pollster::block_on(scope.pop()) {
            return Err(e.to_string());
        }
        Ok(Self {
            device,
            queue,
            pipeline,
            buffers: None,
        })
    }
    pub fn run(&mut self, points: &[[f32; 4]], params: [f32; 16]) -> Result<Vec<[f32; 2]>, String> {
        let n = points.len();
        if n == 0 {
            return Ok(Vec::new());
        }
        let capacity = n.next_power_of_two();
        let limits = self.device.limits();
        if capacity as u64 * 16 > limits.max_storage_buffer_binding_size as u64
            || n.div_ceil(64) > limits.max_compute_workgroups_per_dimension as usize
        {
            return Err("Use CPU for this working set".into());
        }
        let validation = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let memory = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        if self.buffers.as_ref().is_none_or(|b| b.capacity < n) {
            let buffer = |label, size, usage| {
                self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size,
                    usage,
                    mapped_at_creation: false,
                })
            };
            self.buffers = Some(Buffers {
                capacity,
                input: buffer(
                    "Liquify points",
                    capacity as u64 * 16,
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                ),
                output: buffer(
                    "Liquify source coordinates",
                    capacity as u64 * 8,
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                ),
                params: buffer(
                    "Liquify settings",
                    64,
                    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                ),
                readback: buffer(
                    "Liquify result",
                    capacity as u64 * 8,
                    wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                ),
            });
        }
        let b = self.buffers.as_ref().unwrap();
        self.queue
            .write_buffer(&b.input, 0, bytemuck::cast_slice(points));
        self.queue
            .write_buffer(&b.params, 0, bytemuck::cast_slice(&params));
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Liquify"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[&b.input, &b.output, &b.params]
                .into_iter()
                .enumerate()
                .map(|(i, b)| wgpu::BindGroupEntry {
                    binding: i as u32,
                    resource: b.as_entire_binding(),
                })
                .collect::<Vec<_>>(),
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Liquify"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(n.div_ceil(64) as u32, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&b.output, 0, &b.readback, 0, n as u64 * 8);
        let submission = self.queue.submit([encoder.finish()]);
        let (send, recv) = std::sync::mpsc::sync_channel(1);
        b.readback
            .slice(..n as u64 * 8)
            .map_async(wgpu::MapMode::Read, move |r| {
                let _ = send.send(r);
            });
        let poll = self.device.poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(std::time::Duration::from_secs(2)),
        });
        let mapped = recv.recv_timeout(std::time::Duration::from_secs(2));
        for scope in [memory, validation] {
            if let Some(e) = pollster::block_on(scope.pop()) {
                return Err(e.to_string());
            }
        }
        poll.map_err(|e| e.to_string())?;
        mapped
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let data = b.readback.slice(..n as u64 * 8).get_mapped_range();
        let result = bytemuck::cast_slice::<u8, [f32; 2]>(&data).to_vec();
        drop(data);
        b.readback.unmap();
        if result.iter().flatten().any(|v| !v.is_finite()) {
            return Err("Invalid deformation result".into());
        }
        Ok(result)
    }
}
