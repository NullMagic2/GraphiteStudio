//! Test the production egui OpenGL painter without a visible window or desktop input.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use eframe::{egui, egui_glow, glow};
    use glow::HasContext;
    use std::{ffi::CString, sync::Arc};
    use windows_sys::Win32::{
        Graphics::{Gdi::*, OpenGL::*},
        System::LibraryLoader::*,
        UI::WindowsAndMessaging::*,
    };
    unsafe {
        let class: Vec<u16> = "GraphiteHiddenGLProbe\0".encode_utf16().collect();
        let instance = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSW {
            style: CS_OWNDC,
            lpfnWndProc: Some(DefWindowProcW),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        assert!(RegisterClassW(&wc) != 0);
        // Deliberately no WS_VISIBLE, ShowWindow, activation or event injection.
        let window = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            WS_POPUP,
            0,
            0,
            512,
            512,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        assert!(!window.is_null());
        let dc = GetDC(window);
        let format = PIXELFORMATDESCRIPTOR {
            nSize: std::mem::size_of::<PIXELFORMATDESCRIPTOR>() as u16,
            nVersion: 1,
            dwFlags: PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL,
            iPixelType: PFD_TYPE_RGBA,
            cColorBits: 32,
            cAlphaBits: 8,
            ..std::mem::zeroed()
        };
        assert!(SetPixelFormat(dc, ChoosePixelFormat(dc, &format), &format) != 0);
        let context = wglCreateContext(dc);
        assert!(!context.is_null() && wglMakeCurrent(dc, context) != 0);
        let library = LoadLibraryA(c"opengl32.dll".as_ptr().cast());
        let gl = Arc::new(glow::Context::from_loader_function(|name| {
            let name = CString::new(name).unwrap();
            let address = wglGetProcAddress(name.as_ptr().cast())
                .map(|f| f as *const () as usize)
                .unwrap_or(0);
            if address > 3 && address != usize::MAX {
                address as *const _
            } else {
                GetProcAddress(library, name.as_ptr().cast())
                    .map(|f| f as *const () as *const _)
                    .unwrap_or(std::ptr::null())
            }
        }));
        let mut reports = Vec::new();
        for shader in [
            egui_glow::ShaderVersion::Gl120,
            egui_glow::ShaderVersion::Gl140,
        ] {
            let mut painter = egui_glow::Painter::new(gl.clone(), "", Some(shader), false)?;
            let ctx = egui::Context::default();
            let mut texture = ctx.load_texture(
                "probe",
                egui::ColorImage::filled([16, 16], egui::Color32::from_rgb(150, 70, 30)),
                egui::TextureOptions::LINEAR,
            );
            let mut elapsed = Vec::new();
            for pass in 0..32 {
                if pass == 1 {
                    texture.set_partial(
                        [4, 4],
                        egui::ColorImage::filled([8, 8], egui::Color32::from_rgb(20, 150, 60)),
                        egui::TextureOptions::LINEAR,
                    );
                }
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::Vec2::splat(128.),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        ui.painter().image(
                            texture.id(),
                            egui::Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(128., 128.)),
                            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                            egui::Color32::WHITE,
                        );
                    },
                );
                let start = std::time::Instant::now();
                painter.clear([128, 128], [1.; 4]);
                painter.paint_and_update_textures(
                    [128, 128],
                    output.pixels_per_point,
                    &ctx.tessellate(output.shapes, output.pixels_per_point),
                    &output.textures_delta,
                );
                gl.finish();
                if pass >= 2 {
                    elapsed.push(start.elapsed().as_secs_f64() * 1000.);
                }
                let mut pixel = [0u8; 4];
                gl.read_pixels(
                    64,
                    64,
                    1,
                    1,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelPackData::Slice(Some(&mut pixel)),
                );
                assert_eq!(gl.get_error(), glow::NO_ERROR);
                let expected = if pass == 0 {
                    [150u8, 70, 30]
                } else {
                    [20u8, 150, 60]
                };
                assert!(
                    pixel[..3]
                        .iter()
                        .zip(expected)
                        .all(|(&a, b)| a.abs_diff(b) <= 2),
                    "{shader:?}: {pixel:?}"
                );
                if pass == 1 {
                    // x=32 lies between original texels 3 and 4 when enlarged 8x.
                    // Nearest sampling would return one endpoint, rather than this blend.
                    gl.read_pixels(
                        32,
                        64,
                        1,
                        1,
                        glow::RGBA,
                        glow::UNSIGNED_BYTE,
                        glow::PixelPackData::Slice(Some(&mut pixel)),
                    );
                    assert!(
                        (40..130).contains(&pixel[0]) && (85..140).contains(&pixel[1]),
                        "edge must interpolate: {pixel:?}"
                    );
                }
            }
            reports.push(format!("{shader:?}: full/partial upload, interpolated magnified edge and pixel readback passed; mean tiny-probe frame {:.3} ms", elapsed.iter().sum::<f64>() / elapsed.len() as f64));
            painter.destroy();
        }
        if let Some(path) = std::env::args().nth(2) {
            use graphite_studio::{
                core::{
                    document::{CanvasSpec, Document},
                    history::EditTransaction,
                    paper::PaperPreset,
                    pencil::{PencilTipState, ToolSettings},
                    stroke::{StrokeEngine, StrokePoint},
                },
                render::{DocumentRenderer, RasterRenderer},
            };
            let spec = CanvasSpec {
                width_px: 96,
                height_px: 96,
                width_mm: 20.32,
                height_mm: 20.32,
                dpi: 120.,
                name: "Anti-aliasing study".into(),
            };
            let mut doc = Document::new(
                spec,
                PaperPreset::DrawingMedium,
                "White",
                vec![[1.; 3]; 96 * 96],
            );
            let settings = ToolSettings::default();
            let mut tip = PencilTipState::fresh_for_formulation(
                settings.pencil_core_diameter_mm,
                settings.grade.formulation(),
            );
            let mut engine = StrokeEngine::default();
            let mut tx = EditTransaction::default();
            let point = |t: f32| StrokePoint {
                x: 10. + t * 75.,
                y: 48. + 28. * (t * 6.2).sin(),
                pressure: 0.65,
                tilt_deg: 8.,
                azimuth_deg: 20.,
                rotation_deg: None,
            };
            engine.begin_pencil_stroke(point(0.));
            for i in 1..=180 {
                engine.apply_segment(
                    &mut doc,
                    &settings,
                    &mut tip,
                    point((i - 1) as f32 / 180.),
                    point(i as f32 / 180.),
                    &mut tx,
                );
            }
            let image = RasterRenderer::default().render_full(&doc);
            let mut comparison = image::RgbaImage::new(768, 384);
            let mut painter = egui_glow::Painter::new(
                gl.clone(),
                "",
                Some(egui_glow::ShaderVersion::Gl140),
                false,
            )?;
            for (column, options) in [egui::TextureOptions::NEAREST, egui::TextureOptions::LINEAR]
                .into_iter()
                .enumerate()
            {
                let ctx = egui::Context::default();
                let texture = ctx.load_texture("graphite study", image.clone(), options);
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::Vec2::splat(384.),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        ui.painter().image(
                            texture.id(),
                            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(384.)),
                            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                            egui::Color32::WHITE,
                        );
                    },
                );
                painter.clear([384, 384], [1.; 4]);
                painter.paint_and_update_textures(
                    [384, 384],
                    output.pixels_per_point,
                    &ctx.tessellate(output.shapes, output.pixels_per_point),
                    &output.textures_delta,
                );
                let mut pixels = vec![0u8; 384 * 384 * 4];
                gl.read_pixels(
                    0,
                    0,
                    384,
                    384,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelPackData::Slice(Some(&mut pixels)),
                );
                assert_eq!(gl.get_error(), glow::NO_ERROR);
                for y in 0..384usize {
                    for x in 0..384usize {
                        let index = ((383 - y) * 384 + x) * 4;
                        comparison.put_pixel(
                            (column * 384 + x) as u32,
                            y as u32,
                            image::Rgba(pixels[index..index + 4].try_into().unwrap()),
                        );
                    }
                }
            }
            comparison.save(path)?;
            painter.destroy();
        }
        let report = format!("OpenGL compatibility renderer validation\nAdapter: {}\nDriver OpenGL: {}\n{}\nThis tests the installed AMD driver, not physical Intel HD 2000 performance.\nNo visible window or desktop input used.\n", gl.get_parameter_string(glow::RENDERER), gl.get_parameter_string(glow::VERSION), reports.join("\n"));
        if let Some(path) = std::env::args().nth(1) {
            std::fs::write(path, &report)?;
        }
        println!("{report}");
        drop(gl);
        wglMakeCurrent(std::ptr::null_mut(), std::ptr::null_mut());
        wglDeleteContext(context);
        ReleaseDC(window, dc);
        DestroyWindow(window);
        UnregisterClassW(class.as_ptr(), instance);
    }
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("This compatibility probe requires Windows.");
}
