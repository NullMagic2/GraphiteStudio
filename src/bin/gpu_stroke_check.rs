//! Real adapter test: material, pixels and undo, including transfer costs.
use egui_wgpu::wgpu;
use graphite_studio::{
    core::{
        brush::BrushTip,
        document::{CanvasSpec, Document},
        history::{EditTransaction, History},
        paper::PaperPreset,
        pencil::{PencilTipState, ToolKind, ToolSettings},
        stroke::{StrokeEngine, StrokePoint},
        stroke_gpu::GpuStroke,
    },
    render::{DocumentRenderer, RasterRenderer},
};
use std::{sync::Arc, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN | wgpu::Backends::DX12,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));
    let texture = Arc::new(BrushTip {
        name: "test rectangular tip with hole".into(),
        width: 256,
        height: 128,
        mask: (0..32768)
            .map(|i| {
                if (90..166).contains(&(i % 256)) && (40..88).contains(&(i / 256)) {
                    0
                } else {
                    140 + (i * 73 % 116) as u8
                }
            })
            .collect(),
    });
    let args: Vec<_> = std::env::args().collect();
    let texture = if let Some(index) = args.iter().position(|s| s == "--abr") {
        graphite_studio::core::brush::import_abr(&std::fs::read(&args[index + 1])?, "Chromagraph")?
            .remove(0)
    } else {
        texture
    };
    let mut checked = 0;
    for adapter in adapters {
        let info = adapter.get_info();
        if args.iter().any(|s| s == "--dx12") && info.backend != wgpu::Backend::Dx12 {
            continue;
        }
        if info.device_type == wgpu::DeviceType::Cpu {
            continue;
        }
        println!(
            "{} · {:?} · {:?}",
            info.name, info.backend, info.device_type
        );
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("Graphite compute check"),
                ..Default::default()
            }))?;
        let integrated = std::env::args().any(|a| a == "--integrated");
        if integrated {
            graphite_studio::core::stroke_gpu::install(device.clone(), queue.clone())?;
        }
        let display_state = args
            .iter()
            .any(|a| a == "--display")
            .then(|| egui_wgpu::RenderState {
                adapter: adapter.clone(),
                available_adapters: vec![adapter.clone()],
                device: device.clone(),
                queue: queue.clone(),
                target_format: wgpu::TextureFormat::Bgra8Unorm,
                renderer: Arc::new(eframe::egui::mutex::RwLock::new(egui_wgpu::Renderer::new(
                    &device,
                    wgpu::TextureFormat::Bgra8Unorm,
                    Default::default(),
                ))),
                surface_config: egui_wgpu::SurfaceConfig::LOW_LATENCY,
            });
        let mut gpu = GpuStroke::new(device, queue)?;
        checked += 1;
        for (tool, size, tilt, strength) in [
            (ToolKind::Pencil, 200., 65., 0.6),
            (ToolKind::Pencil, 100., 8., 0.6),
            (ToolKind::Eraser, 200., 8., 0.6),
            (ToolKind::Eraser, 200., 8., 1.),
            (ToolKind::Tissue, 400., 8., 0.6),
            (ToolKind::Pencil, 100., 65., 0.7),
            (ToolKind::Pencil, 100., 8., 0.7),
            (ToolKind::Pencil, 200., 8., 0.7),
            (ToolKind::Pencil, 200., 65., 0.7),
            (ToolKind::Eraser, 200., 8., 0.75),
        ] {
            if args.iter().any(|a| a == "--large-paper")
                && !(tool == ToolKind::Pencil && size == 200. && strength == 0.7 && tilt == 65.)
            {
                continue;
            }
            let settings = ToolSettings {
                tool,
                pencil_texture: if strength == 0.7 {
                    None
                } else {
                    Some(texture.clone())
                },
                pencil_color_rgb: [170, 45, 95],
                eraser_diameter_mm: size * 25.4 / 120.,
                eraser_strength: strength,
                eraser_kind: if strength == 0.75 {
                    graphite_studio::core::pencil::EraserKind::Kneaded
                } else {
                    graphite_studio::core::pencil::EraserKind::Vinyl
                },
                tissue_size_px: size,
                ..Default::default()
            };
            let spec = if args.iter().any(|a| a == "--large-paper") {
                CanvasSpec::a4(120.)
            } else {
                CanvasSpec::from_physical("GPU check", 125., 125., 120.)
            };
            let n = spec.pixel_count();
            let mut cpu =
                Document::new(spec, PaperPreset::DrawingMedium, "White", vec![[1.; 3]; n]);
            if tool == ToolKind::Eraser {
                for i in 0..n {
                    cpu.surface.graphite_mass[i] = 0.3;
                    cpu.surface.clay_mass[i] = 0.12;
                    cpu.surface.wax_mass[i] = 0.03;
                    cpu.surface.loose_mass[i] = 0.3;
                    cpu.surface.compacted_mass[i] = 0.15;
                    cpu.surface.color_r_mass[i] = 0.2;
                    cpu.surface.color_g_mass[i] = 0.1;
                    cpu.surface.color_b_mass[i] = 0.15;
                }
            }
            if args.iter().any(|a| a == "--selection") {
                cpu.selection.set(
                    vec![
                        eframe::egui::vec2(220., 190.),
                        eframe::egui::vec2(380., 230.),
                        eframe::egui::vec2(260., 360.),
                    ],
                    cpu.spec.width_px,
                    cpu.spec.height_px,
                );
            }
            let mut accelerated = cpu.clone();
            let mut raster = RasterRenderer::default();
            let mut display = display_state.as_ref().map(|state| {
                let image = raster.render_full(&accelerated);
                let pyramid =
                    graphite_studio::render::GpuDisplayPyramid::new(state.clone(), &image).unwrap();
                state
                    .device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .unwrap();
                accelerated.take_dirty();
                pyramid
            });
            let mut visible_times = Vec::new();
            let before = RasterRenderer::default().rgba8(&cpu);
            let mut ct = PencilTipState::fresh(size * 25.4 / 120.);
            let mut gt = ct.clone();
            let mut engine = StrokeEngine::default();
            let mut gpu_engine = StrokeEngine::default();
            let mut c_tx = EditTransaction::default();
            let mut g_tx = EditTransaction::default();
            let point = |i: usize| StrokePoint {
                x: 200. + i as f32 * 8.,
                y: 240. + (i as f32 * 0.2).sin() * 15.,
                pressure: 0.25 + i as f32 * 0.025,
                tilt_deg: tilt,
                azimuth_deg: i as f32 * 7.,
                rotation_deg: Some(37.),
            };
            let (mut cm, mut gm) = (0., 0.);
            for i in 1..=20 {
                let from = point(i - 1);
                let to = point(i);
                let dx = to.x - from.x;
                let dy = to.y - from.y;
                let distance = (dx * dx + dy * dy).sqrt();
                let steps = (distance / StrokeEngine::dab_spacing_px(&cpu, &settings, &ct))
                    .ceil()
                    .max(1.) as usize;
                let inputs: Vec<_> = (1..=steps)
                    .map(|j| {
                        (
                            from.lerp(to, (j as f32 - 0.5) / steps as f32),
                            distance / steps as f32,
                            Some((dx / distance, dy / distance)),
                        )
                    })
                    .collect();
                graphite_studio::core::stroke_gpu::set_enabled(false);
                let start = Instant::now();
                engine.apply_segment(&mut cpu, &settings, &mut ct, from, to, &mut c_tx);
                cm += start.elapsed().as_secs_f64() * 1000.;
                graphite_studio::core::stroke_gpu::set_enabled(true);
                let start = Instant::now();
                if integrated {
                    gpu_engine.apply_segment(
                        &mut accelerated,
                        &settings,
                        &mut gt,
                        from,
                        to,
                        &mut g_tx,
                    );
                } else {
                    assert!(gpu
                        .apply(
                            &mut accelerated,
                            &settings,
                            &mut gt,
                            &inputs,
                            &mut g_tx,
                            true
                        )?
                        .is_some());
                }
                gm += start.elapsed().as_secs_f64() * 1000.;
                if let Some(display) = display.as_mut() {
                    if let Some(rect) = accelerated.take_dirty() {
                        let image = raster.render_region(&accelerated, rect);
                        display.upload_patch([rect.min_x, rect.min_y], &image)?;
                    }
                    display_state
                        .as_ref()
                        .unwrap()
                        .device
                        .poll(wgpu::PollType::wait_indefinitely())?;
                    visible_times.push(start.elapsed().as_secs_f64() * 1000.);
                }
            }
            ct.commit_pending_wear();
            gt.commit_pending_wear();
            let mut material = 0f32;
            let mut worst = 0;
            for i in 0..n {
                let a = cpu.surface.pixel(i);
                let b = accelerated.surface.pixel(i);
                for (x, y) in [
                    (a.current_height, b.current_height),
                    (a.abrasion, b.abrasion),
                    (a.graphite_mass, b.graphite_mass),
                    (a.clay_mass, b.clay_mass),
                    (a.wax_mass, b.wax_mass),
                    (a.loose_mass, b.loose_mass),
                    (a.compacted_mass, b.compacted_mass),
                    (a.orientation_x, b.orientation_x),
                    (a.orientation_y, b.orientation_y),
                    (a.color_r_mass, b.color_r_mass),
                    (a.color_g_mass, b.color_g_mass),
                    (a.color_b_mass, b.color_b_mass),
                ] {
                    if (x - y).abs() > material {
                        material = (x - y).abs();
                        worst = i;
                    }
                }
            }
            let wear = ct
                .profile
                .height
                .iter()
                .zip(&gt.profile.height)
                .map(|(a, b)| (a - b).abs())
                .fold(0f32, f32::max);
            let a = RasterRenderer::default().rgba8(&cpu);
            let b = RasterRenderer::default().rgba8(&accelerated);
            let error = a.iter().zip(&b).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
            println!("{tool:?} {size}px tilt {tilt} strength {strength}: CPU {:.3} ms; GPU {:.3} ms/segment; material {material:.8}, wear {wear:.8}, pixels {error}/255",cm/20.,gm/20.);
            if !visible_times.is_empty() {
                let first = visible_times[0];
                visible_times.sort_by(f64::total_cmp);
                println!("Stroke + shading + upload + mip completion: median {:.3} ms, p95 {:.3} ms, first {:.3} ms (excludes window presentation/input)", visible_times[10], visible_times[18], first);
            }
            image::save_buffer(
                format!("../../gpu-{tool:?}-{size}-{strength}-cpu.png"),
                &a,
                cpu.spec.width_px as u32,
                cpu.spec.height_px as u32,
                image::ColorType::Rgba8,
            )?;
            image::save_buffer(
                format!("../../gpu-{tool:?}-{size}-{strength}-gpu.png"),
                &b,
                cpu.spec.width_px as u32,
                cpu.spec.height_px as u32,
                image::ColorType::Rgba8,
            )?;
            if material >= 0.0002 {
                println!(
                    "worst {},{} CPU {:?} GPU {:?}",
                    worst % cpu.spec.width_px,
                    worst / cpu.spec.width_px,
                    cpu.surface.pixel(worst),
                    accelerated.surface.pixel(worst)
                );
            }
            assert!(material < 0.002, "Material mismatch");
            assert!(wear < 0.0001, "Wear mismatch");
            assert!(error <= 1, "Visible mismatch");
            let mut history = History::default();
            history.push(g_tx, &accelerated);
            assert!(history.undo(&mut accelerated));
            assert_eq!(RasterRenderer::default().rgba8(&accelerated), before);
            assert!(history.redo(&mut accelerated));
            assert_eq!(RasterRenderer::default().rgba8(&accelerated), b);
        }
        if std::env::args().any(|a| a == "--one") {
            break;
        }
    }
    println!(
        "{} successful compute batches",
        graphite_studio::core::stroke_gpu::completed_batches()
    );
    assert!(graphite_studio::core::stroke_gpu::completed_batches() > 0);
    assert!(checked > 0, "No hardware compute adapter tested");
    Ok(())
}
