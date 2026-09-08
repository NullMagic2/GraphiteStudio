//! Batched compute strokes. CPU material is committed only after a successful
//! readback; unsupported devices and failed dispatches leave it untouched.
use super::{
    brush::BrushTip,
    contact::pencil_effective_pressure,
    document::{DirtyRect, Document},
    history::EditTransaction,
    material::PixelSurfaceState,
    pencil::{PencilTipState, ToolKind, ToolSettings},
    stroke::StrokePoint,
};
use egui_wgpu::wgpu::{self, util::DeviceExt};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

enum ContactBounds<'a> {
    Native(super::contact::PreparedTipContact),
    Imported(super::brush::ImportedContact<'a>),
    Circle(f32),
}
impl ContactBounds<'_> {
    fn covers(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> bool {
        match self {
            Self::Native(t) => t.may_cover_box(x0, y0, x1, y1),
            Self::Imported(t) => t.may_cover_box(x0, y0, x1, y1),
            Self::Circle(r) => {
                let x = 0f32.clamp(x0, x1);
                let y = 0f32.clamp(y0, y1);
                x * x + y * y <= r * r
            }
        }
    }
}

pub struct GpuStroke {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    mask: Option<(Arc<BrushTip>, wgpu::Buffer)>,
    scratch: Option<Scratch>,
}
struct Scratch {
    capacity: usize,
    states: wgpu::Buffer,
    coordinates: wgpu::Buffer,
    dabs: wgpu::Buffer,
    profile: wgpu::Buffer,
    wear: wgpu::Buffer,
    metadata: wgpu::Buffer,
    readback: wgpu::Buffer,
}
impl Scratch {
    fn new(device: &wgpu::Device, n: usize) -> Self {
        let capacity = n.div_ceil(4096) * 4096;
        let buffer = |label, size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let storage = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;
        Self {
            capacity,
            states: buffer(
                "Stroke material scratch",
                capacity as u64 * 64,
                storage | wgpu::BufferUsages::COPY_SRC,
            ),
            coordinates: buffer("Stroke coordinate scratch", capacity as u64 * 4, storage),
            dabs: buffer("Stroke dab scratch", 64 * 64 * 4, storage),
            profile: buffer("Stroke profile scratch", 64 * 64 * 4, storage),
            wear: buffer(
                "Stroke wear scratch",
                64 * 64 * 4,
                storage | wgpu::BufferUsages::COPY_SRC,
            ),
            metadata: buffer(
                "Stroke metadata scratch",
                32,
                wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            ),
            readback: buffer(
                "Stroke readback scratch",
                capacity as u64 * 64 + 64 * 64 * 4,
                wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            ),
        }
    }
}
static GPU: OnceLock<Mutex<Option<GpuStroke>>> = OnceLock::new();
static ENABLED: AtomicBool = AtomicBool::new(true);
static COMPLETED: AtomicU64 = AtomicU64::new(0);
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}
pub fn completed_batches() -> u64 {
    COMPLETED.load(Ordering::Relaxed)
}
pub fn install(device: wgpu::Device, queue: wgpu::Queue) -> Result<(), String> {
    let engine = GpuStroke::new(device, queue)?;
    *GPU.get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|e| e.to_string())? = Some(engine);
    Ok(())
}
pub fn available() -> bool {
    ENABLED.load(Ordering::Relaxed)
        && GPU
            .get()
            .is_some_and(|g| g.try_lock().is_ok_and(|g| g.is_some()))
}
pub fn disable() {
    if let Some(g) = GPU.get() {
        if let Ok(mut g) = g.lock() {
            *g = None;
        }
    }
}
pub fn eligible(settings: &ToolSettings) -> bool {
    matches!(
        settings.tool,
        ToolKind::Pencil | ToolKind::Eraser | ToolKind::Tissue
    )
}
pub(crate) fn should_batch(doc: &Document, s: &ToolSettings, tip: &PencilTipState) -> bool {
    let large = match s.tool {
        ToolKind::Pencil => {
            tip.effective_core_diameter_px(doc.spec.dpi) * s.pencil_geometry_scale >= 40.
        }
        ToolKind::Eraser => {
            s.eraser_strength > 0.
                && s.eraser_strength < 1.
                && s.eraser_diameter_mm * doc.spec.dpi / 25.4 >= 80.
        }
        _ => false,
    };
    large && available()
}
pub(crate) type InputDab = (StrokePoint, f32, Option<(f32, f32)>);
pub(crate) fn try_apply(
    doc: &mut Document,
    settings: &ToolSettings,
    tip: &mut PencilTipState,
    dabs: &[InputDab],
    tx: &mut EditTransaction,
) -> Option<Option<DirtyRect>> {
    if !eligible(settings) {
        return None;
    }
    let mut guard = GPU.get()?.try_lock().ok()?;
    let engine = guard.as_mut()?;
    match engine.apply(doc, settings, tip, dabs, tx, false) {
        Ok(result) => result,
        Err(_) => {
            *guard = None;
            None
        }
    }
}
impl GpuStroke {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Result<Self, String> {
        let limits = device.limits();
        if limits.max_compute_invocations_per_workgroup < 64
            || limits.max_storage_buffers_per_shader_stage < 6
        {
            return Err("Device does not support stroke compute.".into());
        }
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Graphite material compute"),
            source: wgpu::ShaderSource::Wgsl(include_str!("stroke_compute.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Graphite strokes"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        // Allocate and initialize a typical broad-tip working set at startup,
        // rather than making the first pen contact pay this cost.
        let scratch = Scratch::new(&device, 131_072);
        let mut warm = device.create_command_encoder(&Default::default());
        warm.clear_buffer(&scratch.states, 0, None);
        warm.clear_buffer(&scratch.readback, 0, None);
        queue.submit([warm.finish()]);
        if let Some(error) = pollster::block_on(scope.pop()) {
            return Err(error.to_string());
        }
        Ok(Self {
            device,
            queue,
            pipeline,
            mask: None,
            scratch: Some(scratch),
        })
    }
    pub fn apply(
        &mut self,
        doc: &mut Document,
        s: &ToolSettings,
        tip: &mut PencilTipState,
        inputs: &[InputDab],
        tx: &mut EditTransaction,
        force: bool,
    ) -> Result<Option<Option<DirtyRect>>, String> {
        let clock = std::time::Instant::now();
        if !eligible(s) || inputs.is_empty() || inputs.len() > 64 {
            return Ok(None);
        }
        // A single tissue sample and a complete lift cost less on the CPU than a
        // round trip through GPU memory. Keep these measured fast paths local.
        if !force
            && (s.tool == ToolKind::Tissue
                || (s.tool == ToolKind::Eraser && s.eraser_strength >= 1.))
        {
            return Ok(None);
        }
        let mut dabs = Vec::<[f32; 64]>::new();
        let mut footprints = Vec::new();
        let mut bounds: Option<DirtyRect> = None;
        let f = s.grade.formulation();
        let dpi = doc.spec.dpi;
        for &(p, travel, motion) in inputs {
            let pressure = p.pressure.clamp(0., 1.);
            if pressure <= 1e-4 || travel <= 1e-8 {
                continue;
            }
            if s.tool == ToolKind::Eraser && s.eraser_strength <= 0. {
                continue;
            }
            if s.tool == ToolKind::Tissue && (s.tissue_load <= 0. || s.flow <= 0.) {
                continue;
            }
            let load = pencil_effective_pressure(pressure);
            let tilt = (p.tilt_deg.clamp(0., 84.) / 84.).powf(1.20);
            let rotation = p
                .rotation_deg
                .filter(|a| a.is_finite())
                .map(|a| a.rem_euclid(360.))
                .unwrap_or(tip.profile.orientation_deg);
            let angle = p.azimuth_deg + rotation;
            let (sin, cos) = angle.to_radians().sin_cos();
            let (rs, rc) = (-rotation.to_radians()).sin_cos();
            let mut a = [0.; 64];
            a[0] = p.x;
            a[1] = p.y;
            a[4] = sin;
            a[5] = cos;
            a[6] = rs;
            a[7] = rc;
            a[8] = super::contact::pencil_pressure_force_drive(pressure);
            a[9] = 0.77
                - load
                    * (0.66 + 0.08 * doc.paper.compliance() - 0.04 * f.core_hardness.clamp(0., 1.));
            a[10] = 0.70 + 0.05 * doc.paper.compliance();
            a[11] = 0.16 + 0.18 * load;
            a[12] = tilt;
            a[13] = f.wear_coefficient * s.flow.clamp(0.05, 2.);
            a[14] = 0.72 + 0.28 * load.powf(0.72);
            a[15] = 1. + 0.36 * tilt * f.lubricity;
            a[16] = travel * 25.4 / dpi;
            a[17] = 0.055 + 0.34 * pressure.powf(1.55);
            let direction = motion
                .map(|(x, y)| y.atan2(x))
                .unwrap_or(p.azimuth_deg.rem_euclid(360.).to_radians())
                * 2.;
            a[18] = direction.cos();
            a[19] = direction.sin();
            a[20] = f.wear_coefficient;
            a[21] = f.graphite_fraction;
            a[22] = f.clay_fraction;
            a[23] = f.wax_fraction;
            a[24] = f.core_hardness;
            a[25] = f.lubricity;
            a[26] = doc.paper.abrasion_resistance();
            a[27] = doc.paper.capture_bias();
            for c in 0..3 {
                a[28 + c] = s.pencil_color_rgb[c] as f32 / 255.;
            }
            a[31] = s.particle_variation;
            a[32] = travel * (120. / dpi.max(1.)).powi(3);
            a[33] = pressure;
            a[34] = f.point_retention.clamp(0., 1.);
            a[35] = tip.profile.peak_height();
            let contact_bounds;
            let reach = match s.tool {
                ToolKind::Eraser => {
                    a[36] = 1.;
                    a[37] = s.eraser_strength.clamp(0., 1.);
                    a[38] = if s.eraser_kind == super::pencil::EraserKind::Kneaded {
                        1.
                    } else {
                        0.
                    };
                    a[39] = (s.eraser_diameter_mm * dpi / 25.4).max(0.7)
                        * 0.5
                        * s.eraser_kind.pressure_scale(pressure);
                    contact_bounds = ContactBounds::Circle(a[39] + 0.5);
                    a[39] + 2.
                }
                ToolKind::Tissue => {
                    a[36] = 2.;
                    let size = s.tissue_size_px.clamp(0.1, 8192.) * (0.65 + 0.35 * pressure.sqrt());
                    a[40] = pressure * s.flow * (s.tissue_load * 0.20) * travel / size.max(1.);
                    a[41] = 4. / (size * size);
                    contact_bounds = ContactBounds::Circle(size * 0.5 + 0.5);
                    size * 0.5 + 2.
                }
                _ if s.pencil_tip_shape && s.pencil_texture.is_some() => {
                    let texture = s.pencil_texture.as_ref().unwrap();
                    let diameter = (tip.effective_core_diameter_px(dpi)
                        * s.pencil_geometry_scale
                        * s.pressure_width_scale(pressure))
                    .max(0.8);
                    let size = diameter
                        * (0.35 + 0.65 * load)
                        * (1.1 - 0.25 * s.tip_sharpness.clamp(0., 1.));
                    let longest = texture.width.max(texture.height) as f32;
                    a[2] = (size * texture.width as f32 / longest * 0.5).max(0.25);
                    a[3] = (size * texture.height as f32 / longest
                        * 0.5
                        * (1. + 0.8 * (p.tilt_deg.clamp(0., 84.) / 84.).powi(2)))
                    .max(0.25);
                    contact_bounds = ContactBounds::Imported(super::brush::ImportedContact::new(
                        texture,
                        diameter,
                        pressure,
                        p.tilt_deg,
                        angle,
                        s.tip_sharpness,
                    ));
                    a[2].hypot(a[3]) + 3.
                }
                _ => {
                    a[36] = 3.;
                    let diameter = (tip.effective_core_diameter_px(dpi)
                        * s.pencil_geometry_scale
                        * s.pressure_width_scale(pressure))
                    .max(0.8);
                    let geometry = super::contact::PencilTipContact::from_state_with_sharpness(
                        diameter,
                        pressure,
                        p.tilt_deg,
                        p.azimuth_deg,
                        f.core_hardness,
                        s.tip_sharpness,
                    );
                    let (gs, gc) = geometry.angle_rad.sin_cos();
                    a[4] = gs;
                    a[5] = gc;
                    a[42] = geometry.point_radius_px;
                    a[43] = geometry.cross_radius_px;
                    a[44] = geometry.point_axial_radius_px;
                    a[45] = -geometry.point_axial_radius_px - geometry.rear_extension_px;
                    a[46] = 0.5 * (a[44] + a[45]);
                    a[47] = (0.5 * (a[44] - a[45])).max(geometry.point_axial_radius_px);
                    let power = 2. + 0.25 * geometry.effective_pressure;
                    a[48] = power + (5. - power) * geometry.side_fraction;
                    a[49] = power + (2.25 - power) * geometry.side_fraction;
                    a[50] = geometry.side_fraction;
                    a[51] = geometry.grain_radius_px.max(0.35);
                    a[52] = 0.018 + 0.145 * load + 0.060 * geometry.side_fraction;
                    a[53] = geometry.force_density_scale;
                    a[54] = p
                        .rotation_deg
                        .filter(|v| v.is_finite())
                        .unwrap_or(tip.profile.orientation_deg)
                        .rem_euclid(360.)
                        .to_radians();
                    a[57] = (0.10 * dpi / 25.4) * (1.15 - 0.35 * pressure);
                    a[55] = (0.045 * dpi / 25.4).max(0.65);
                    a[56] = if s.pencil_texture.is_some() { 1. } else { 0. };
                    contact_bounds =
                        ContactBounds::Native(geometry.prepare(&tip.profile, pressure));
                    geometry.reach_px().ceil() + 2.
                }
            };
            let rect = DirtyRect::new(
                (p.x - reach).floor().max(0.) as usize,
                (p.y - reach).floor().max(0.) as usize,
                (p.x + reach + 1.)
                    .ceil()
                    .clamp(0., doc.spec.width_px as f32) as usize,
                (p.y + reach + 1.)
                    .ceil()
                    .clamp(0., doc.spec.height_px as f32) as usize,
            );
            if rect.min_x >= rect.max_x || rect.min_y >= rect.max_y {
                continue;
            }
            if a[36] == 3. {
                a[58] = (p.x.floor() - reach).max(0.);
                a[59] = (p.y.floor() - reach).max(0.);
                a[60] = (p.x.ceil() + reach + 1.).min(doc.spec.width_px as f32);
                a[61] = (p.y.ceil() + reach + 1.).min(doc.spec.height_px as f32);
            } else {
                a[58] = rect.min_x as f32;
                a[59] = rect.min_y as f32;
                a[60] = rect.max_x as f32;
                a[61] = rect.max_y as f32;
            }
            bounds = Some(bounds.map_or(rect, |b| b.union(rect)));
            dabs.push(a);
            footprints.push((p.x, p.y, contact_bounds));
        }
        let Some(rect) = bounds else {
            return Ok(Some(None));
        };
        // Build a conservative tile union. Only these pixels travel across the
        // CPU/GPU boundary. The same contact bounds are used by the CPU engine;
        // no spacing, material resolution or texture samples are reduced.
        const TILE: usize = 16;
        let cols = rect.width().div_ceil(TILE);
        let rows = rect.height().div_ceil(TILE);
        let mut tiles = vec![false; cols * rows];
        for ((cx, cy, bounds), a) in footprints.iter().zip(&dabs) {
            for ty in 0..rows {
                let y0 = rect.min_y + ty * TILE;
                let y1 = (y0 + TILE).min(rect.max_y) - 1;
                if y1 as f32 + 0.5 < a[59] || y0 as f32 + 0.5 >= a[61] {
                    continue;
                }
                for tx in 0..cols {
                    let t = ty * cols + tx;
                    if tiles[t] {
                        continue;
                    }
                    let x0 = rect.min_x + tx * TILE;
                    let x1 = (x0 + TILE).min(rect.max_x) - 1;
                    if x1 as f32 + 0.5 < a[58] || x0 as f32 + 0.5 >= a[60] {
                        continue;
                    }
                    tiles[t] = bounds.covers(
                        x0 as f32 + 0.5 - cx,
                        y0 as f32 + 0.5 - cy,
                        x1 as f32 + 0.5 - cx,
                        y1 as f32 + 0.5 - cy,
                    );
                }
            }
        }
        let mut indices = Vec::new();
        for y in rect.min_y..rect.max_y {
            let ty = (y - rect.min_y) / TILE;
            for tx in 0..cols {
                if !tiles[ty * cols + tx] {
                    continue;
                }
                let left = rect.min_x + tx * TILE;
                for x in left..(left + TILE).min(rect.max_x) {
                    if doc.selection.allows(doc.index(x, y)) {
                        indices.push(doc.index(x, y) as u32);
                    }
                }
            }
        }
        let n = indices.len();
        if n == 0 {
            return Ok(Some(None));
        }
        // A 200 px side facet can cover most of an A4 sheet at 120 dpi.
        // Bound scratch storage to 96 MB per material buffer, also respecting
        // the actual device limit, instead of rejecting those large contacts.
        // Contact culling makes upright tips much cheaper to transfer. The old
        // rectangle threshold accidentally routed these expensive solvers to CPU.
        let minimum_work = match s.tool {
            ToolKind::Pencil if s.pencil_texture.is_none() => 8_000,
            ToolKind::Pencil => 16_000,
            _ => 40_000,
        };
        if n > 1_500_000
            || (n.div_ceil(4096) * 4096) as u64 * 64
                > self.device.limits().max_storage_buffer_binding_size as u64
            || tip.profile.height.len() > 64 * 64
            || (!force && n * dabs.len() < minimum_work)
        {
            return Ok(None);
        }
        let mut pixels = Vec::with_capacity(n);
        let prepared = clock.elapsed();
        for &offset in &indices {
            let i = offset as usize;
            let p = doc.surface.pixel(i);
            pixels.push([
                p.current_height,
                p.abrasion,
                p.graphite_mass,
                p.clay_mass,
                p.wax_mass,
                p.loose_mass,
                p.compacted_mass,
                p.orientation_x,
                p.orientation_y,
                p.color_r_mass,
                p.color_g_mass,
                p.color_b_mass,
                doc.surface.fiber[i],
                doc.surface.contact_support[i] as f32 / 255.,
                (doc.color_grain[i] as u32 + 256 * doc.surface.edge_grain[i] as u32) as f32,
                1., // Packed coordinates have already passed selection clipping.
            ]);
        }
        let validation = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let packed = clock.elapsed();
        let memory = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let internal = self.device.push_error_scope(wgpu::ErrorFilter::Internal);
        let texture = s
            .pencil_texture
            .as_ref()
            .filter(|_| s.tool == ToolKind::Pencil)
            .cloned()
            .unwrap_or_else(|| {
                static WHITE: OnceLock<Arc<BrushTip>> = OnceLock::new();
                WHITE
                    .get_or_init(|| {
                        Arc::new(BrushTip {
                            name: String::new(),
                            width: 1,
                            height: 1,
                            mask: vec![255],
                        })
                    })
                    .clone()
            });
        if self
            .mask
            .as_ref()
            .is_none_or(|(t, _)| !Arc::ptr_eq(t, &texture))
        {
            let values: Vec<u32> = texture.mask.iter().map(|&v| v as u32).collect();
            if values.len() as u64 * 4 > self.device.limits().max_storage_buffer_binding_size as u64
            {
                return Ok(None);
            }
            let buffer = self.buffer(
                "Pencil mask",
                bytemuck::cast_slice(&values),
                wgpu::BufferUsages::STORAGE,
            );
            self.mask = Some((texture.clone(), buffer));
        }
        if self.scratch.as_ref().is_none_or(|b| b.capacity < n) {
            self.scratch = Some(Scratch::new(&self.device, n));
        }
        let scratch = self.scratch.as_ref().unwrap();
        let states = &scratch.states;
        let dab_buffer = &scratch.dabs;
        let profile = &scratch.profile;
        let wear = &scratch.wear;
        self.queue
            .write_buffer(states, 0, bytemuck::cast_slice(&pixels));
        self.queue
            .write_buffer(dab_buffer, 0, bytemuck::cast_slice(&dabs));
        self.queue
            .write_buffer(profile, 0, bytemuck::cast_slice(&tip.profile.height));
        self.queue.write_buffer(
            wear,
            0,
            bytemuck::cast_slice(&vec![0u32; tip.profile.height.len()]),
        );
        let meta = [
            0,
            0,
            doc.spec.width_px as u32,
            n as u32,
            dabs.len() as u32,
            tip.profile.resolution as u32,
            texture.width as u32,
            texture.height as u32,
        ];
        let meta_buffer = &scratch.metadata;
        let index_buffer = &scratch.coordinates;
        self.queue
            .write_buffer(meta_buffer, 0, bytemuck::cast_slice(&meta));
        self.queue
            .write_buffer(index_buffer, 0, bytemuck::cast_slice(&indices));
        let buffers = [
            &states,
            &dab_buffer,
            &self.mask.as_ref().unwrap().1,
            &profile,
            &wear,
            &meta_buffer,
            &index_buffer,
        ];
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Stroke compute"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &buffers
                .iter()
                .enumerate()
                .map(|(i, b)| wgpu::BindGroupEntry {
                    binding: i as u32,
                    resource: b.as_entire_binding(),
                })
                .collect::<Vec<_>>(),
        });
        let material_bytes = n as u64 * 64;
        let wear_bytes = tip.profile.height.len() as u64 * 4;
        let readback = &scratch.readback;
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Deposit stroke"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups((n as u32).div_ceil(64), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&states, 0, &readback, 0, material_bytes);
        encoder.copy_buffer_to_buffer(&wear, 0, &readback, material_bytes, wear_bytes);
        let submission = self.queue.submit([encoder.finish()]);
        let submitted = clock.elapsed();
        let (send, recv) = std::sync::mpsc::sync_channel(1);
        readback
            .slice(..material_bytes + wear_bytes)
            .map_async(wgpu::MapMode::Read, move |r| {
                let _ = send.send(r);
            });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(std::time::Duration::from_secs(2)),
            })
            .map_err(|e| e.to_string())?;
        recv.recv_timeout(std::time::Duration::from_secs(2))
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        for scope in [internal, memory, validation] {
            if let Some(error) = pollster::block_on(scope.pop()) {
                return Err(error.to_string());
            }
        }
        let bytes = readback
            .slice(..material_bytes + wear_bytes)
            .get_mapped_range();
        let waited = clock.elapsed();
        let output: &[f32] = bytemuck::cast_slice(&bytes);
        if output.iter().any(|v| !v.is_finite()) {
            return Err("Non-finite GPU material.".into());
        }
        let mut changed: Option<DirtyRect> = None;
        for (i, values) in output[..n * 16].chunks_exact(16).enumerate() {
            if values[..12] == pixels[i][..12] {
                continue;
            }
            let index = indices[i] as usize;
            let x = index % doc.spec.width_px;
            let y = index / doc.spec.width_px;
            tx.remember(index, doc);
            doc.surface.set_pixel(
                index,
                PixelSurfaceState {
                    current_height: values[0],
                    abrasion: values[1],
                    graphite_mass: values[2],
                    clay_mass: values[3],
                    wax_mass: values[4],
                    loose_mass: values[5],
                    compacted_mass: values[6],
                    orientation_x: values[7],
                    orientation_y: values[8],
                    color_r_mass: values[9],
                    color_g_mass: values[10],
                    color_b_mass: values[11],
                },
            );
            let pixel = DirtyRect::new(x, y, x + 1, y + 1);
            changed = Some(changed.map_or(pixel, |r| r.union(pixel)));
        }
        for (v, &delta) in tip.profile.pending_wear.iter_mut().zip(&output[n * 16..]) {
            *v += delta;
        }
        drop(bytes);
        readback.unmap();
        COMPLETED.fetch_add(1, Ordering::Relaxed);
        static PROFILE: OnceLock<bool> = OnceLock::new();
        if *PROFILE.get_or_init(|| std::env::var_os("GRAPHITE_GPU_PROFILE").is_some()) {
            eprintln!("GPU phase {n} pixels, {} samples: prepare {:.2}, pack {:.2}, upload {:.2}, wait {:.2}, commit {:.2} ms",dabs.len(),prepared.as_secs_f64()*1000.,(packed-prepared).as_secs_f64()*1000.,(submitted-packed).as_secs_f64()*1000.,(waited-submitted).as_secs_f64()*1000.,(clock.elapsed()-waited).as_secs_f64()*1000.);
        }
        Ok(Some(changed))
    }
    fn buffer(&self, label: &str, bytes: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
        self.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytes,
                usage,
            })
    }
}
