use super::*;
use graphite_studio::import::{OpenProgress, OpenedDocument};
use std::{
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};

pub(super) struct OpeningJob {
    path: PathBuf,
    progress: Arc<Mutex<OpenProgress>>,
    result: mpsc::Receiver<Result<PreparedDocument, String>>,
}

struct PreparedDocument {
    opened: OpenedDocument,
    renderer: RasterRenderer,
    preview: egui::ColorImage,
    coverage: f32,
    disturbance: f32,
}

impl OpeningJob {
    fn start(path: PathBuf) -> Result<Self, String> {
        Self::spawn(path, |path, progress| {
            let opened = graphite_studio::import::open_with_progress(path, progress)?;
            progress(OpenProgress {
                fraction: 0.88,
                stage: "Preparing preview…",
            });
            let mut renderer = RasterRenderer::default();
            let preview = renderer.render_full(&opened.project.document);
            progress(OpenProgress {
                fraction: 0.97,
                stage: "Finishing drawing…",
            });
            let coverage = opened.project.document.graphite_coverage();
            let disturbance = opened.project.document.mean_paper_disturbance();
            Ok(PreparedDocument {
                opened,
                renderer,
                preview,
                coverage,
                disturbance,
            })
        })
    }

    fn spawn(
        path: PathBuf,
        load: impl FnOnce(&Path, &mut dyn FnMut(OpenProgress)) -> Result<PreparedDocument, String>
            + Send
            + 'static,
    ) -> Result<Self, String> {
        let progress = Arc::new(Mutex::new(OpenProgress {
            fraction: 0.,
            stage: "Starting…",
        }));
        let worker_progress = progress.clone();
        let worker_path = path.clone();
        let (sender, result) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("graphite-file-open".into())
            .spawn(move || {
                // The worker owns all imported state. No app/document state is shared or
                // modified until the UI receives a complete, successfully prepared result.
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    load(&worker_path, &mut |value| {
                        *worker_progress.lock().unwrap_or_else(|e| e.into_inner()) = value;
                    })
                }))
                .unwrap_or_else(|_| {
                    Err("The file could not be processed. Your open drawings are unchanged.".into())
                });
                let _ = sender.send(result);
            })
            .map_err(|e| format!("Could not start file opening: {e}"))?;
        Ok(Self {
            path,
            progress,
            result,
        })
    }
}

impl GraphiteApp {
    pub(super) fn open_file_with_feedback(&mut self, path: &Path) {
        if self.opening.is_some() {
            return;
        }
        self.finish_stroke();
        self.quick_controls.close_size();
        self.open_error = None;
        match OpeningJob::start(path.to_owned()) {
            Ok(job) => {
                self.status = format!(
                    "Opening {}…",
                    path.file_name().unwrap_or_default().to_string_lossy()
                );
                self.opening = Some(job);
            }
            Err(error) => self.report_open_error(error),
        }
    }

    fn report_open_error(&mut self, error: String) {
        self.status = format!("File not opened: {error}");
        self.open_error = Some(self.status.clone());
    }

    pub(super) fn poll_opening(&mut self) {
        let Some(job) = &self.opening else {
            return;
        };
        let result = match job.result.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("The file-opening worker stopped unexpectedly.".into())
            }
        };
        let path = self.opening.take().unwrap().path;
        match result {
            Ok(prepared) => match self.install_opened_document(&path, prepared.opened) {
                Ok(()) => {
                    self.renderer = prepared.renderer;
                    self.prepared_preview = Some((self.document.revision(), prepared.preview));
                    self.sidebar_statistics = (
                        self.document.revision(),
                        prepared.coverage,
                        prepared.disturbance,
                    );
                    self.last_statistics_update = Some(std::time::Instant::now());
                }
                Err(error) => self.report_open_error(error),
            },
            Err(error) => self.report_open_error(error),
        }
    }

    pub(super) fn show_opening(&mut self, ctx: &egui::Context) {
        if let Some(job) = &self.opening {
            let progress = *job.progress.lock().unwrap_or_else(|e| e.into_inner());
            egui::Modal::new(egui::Id::new("opening_file")).show(ctx, |ui| {
                ui.set_width(360.);
                ui.heading("Opening image…");
                ui.add_space(8.);
                ui.label(job.path.file_name().unwrap_or_default().to_string_lossy())
                    .on_hover_text(graphite_studio::recent_files::display_path(&job.path));
                ui.add_space(10.);
                ui.add(
                    egui::ProgressBar::new(progress.fraction)
                        .animate(true)
                        .desired_width(360.),
                )
                .on_hover_text("Progress through file loading, layer preparation, and preview.");
                ui.label(progress.stage);
            });
            // Never wait or join on the UI thread, including when minimized.
            ctx.request_repaint_after(Duration::from_millis(33));
        }
        if let Some(error) = self.open_error.clone() {
            let response = egui::Modal::new(egui::Id::new("open_file_error")).show(ctx, |ui| {
                ui.set_width(400.);
                ui.heading("Could not open file");
                ui.label(error);
                let button = ui.button("OK");
                #[cfg(test)]
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("open_error_ok"), button.rect));
                button.clicked()
            });
            if response.inner || response.should_close() {
                self.open_error = None;
            }
        }
    }

    #[cfg(test)]
    pub(super) fn wait_for_opening(&mut self) {
        let deadline = std::time::Instant::now() + Duration::from_secs(120);
        while self.opening.is_some() {
            assert!(
                std::time::Instant::now() < deadline,
                "background open timed out"
            );
            self.poll_opening();
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_open_preserves_layers_and_recovers_after_errors() {
        let mut app = GraphiteApp::with_render_state(None);
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/open");
        let history = app.recent_files.clone();
        let tabs = app.tabs.len();
        app.open_file_with_feedback(&fixtures.join("missing.png"));
        app.wait_for_opening();
        assert!(app.open_error.is_some());
        assert_eq!(app.tabs.len(), tabs);
        assert_eq!(app.recent_files, history);
        for name in [
            "rgba.PNG",
            "color.jpeg",
            "color.jpg",
            "color.bmp",
            "group-mask.psd",
            "shape-layers.psd",
        ] {
            let expected = graphite_studio::import::open(&fixtures.join(name)).unwrap();
            app.open_file_with_feedback(&fixtures.join(name));
            app.wait_for_opening();
            assert!(app.open_error.is_none(), "{:?}", app.open_error);
            assert_eq!(
                app.document.layers.len(),
                expected.project.document.layers.len()
            );
            for (a, b) in app
                .document
                .layers
                .iter()
                .zip(&expected.project.document.layers)
            {
                assert_eq!(a.name, b.name);
                assert_eq!(a.vectors.strokes.len(), b.vectors.strokes.len());
            }
            assert_eq!(
                app.prepared_preview.as_ref().unwrap().1,
                RasterRenderer::default().render_full(&expected.project.document)
            );
            assert!(app.recent_files.files()[0].ends_with(name));
        }
        let native =
            std::env::temp_dir().join(format!("graphite-background-native-{}", std::process::id()));
        std::fs::create_dir_all(&native).unwrap();
        for name in ["drawing.graphite", "drawing.psd"] {
            let path = native.join(name);
            app.save_project_path(&path).unwrap();
            let expected = app.renderer.render_full(&app.document);
            app.open_file_with_feedback(&path);
            app.wait_for_opening();
            assert!(app.open_error.is_none(), "{:?}", app.open_error);
            assert_eq!(
                app.tabs[app.active_tab].project_path.as_deref(),
                Some(path.as_path())
            );
            assert_eq!(app.prepared_preview.as_ref().unwrap().1, expected);
            std::fs::remove_file(path).unwrap();
        }
        std::fs::remove_dir(native).unwrap();
    }

    #[test]
    #[ignore = "Set GRAPHITE_TEST_PSD and GRAPHITE_TEST_RGB for local artwork verification"]
    fn full_size_background_open_matches_independent_pixels() {
        let path = PathBuf::from(std::env::var_os("GRAPHITE_TEST_PSD").expect("input PSD"));
        let reference =
            std::fs::read(std::env::var_os("GRAPHITE_TEST_RGB").expect("RGB reference")).unwrap();
        let mut app = GraphiteApp::with_render_state(None);
        let start = std::time::Instant::now();
        app.open_file_with_feedback(&path);
        assert!(app.opening.is_some());
        app.wait_for_opening();
        assert!(app.open_error.is_none(), "{:?}", app.open_error);
        let preview = &app.prepared_preview.as_ref().unwrap().1;
        assert_eq!(preview.size, [4032, 3024]);
        assert_eq!(preview.pixels.len() * 3, reference.len());
        for (i, (pixel, rgb)) in preview
            .pixels
            .iter()
            .zip(reference.chunks_exact(3))
            .enumerate()
        {
            assert_eq!(&pixel.to_array()[..3], rgb, "pixel {i}");
        }
        eprintln!(
            "Background open and preview: {:?}; {} pixels verified.",
            start.elapsed(),
            preview.pixels.len()
        );
    }

    #[test]
    #[ignore = "Set GRAPHITE_LAYER_PSD and GRAPHITE_LAYER_REFERENCE for local multilayer verification"]
    fn large_layered_background_open_matches_every_layer() {
        let path = PathBuf::from(std::env::var_os("GRAPHITE_LAYER_PSD").expect("input PSD"));
        let reference = PathBuf::from(
            std::env::var_os("GRAPHITE_LAYER_REFERENCE").expect("reference directory"),
        );
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(reference.join("manifest.json")).unwrap())
                .unwrap();
        let layers = manifest["layers"].as_array().unwrap();
        let mut app = GraphiteApp::with_render_state(None);
        let start = std::time::Instant::now();
        app.open_file_with_feedback(&path);
        app.wait_for_opening();
        assert!(app.open_error.is_none(), "{:?}", app.open_error);
        assert_eq!(app.document.layers.len(), layers.len());
        let mut total = 0;
        for (i, expected) in layers.iter().enumerate() {
            let layer = &app.document.layers[i];
            assert_eq!(layer.name, expected["name"].as_str().unwrap());
            assert_eq!(layer.visible, expected["visible"].as_bool().unwrap());
            assert_eq!(layer.opacity as u64, expected["opacity"].as_u64().unwrap());
            assert_eq!(
                layer.blend_mode,
                match expected["blend"].as_str().unwrap() {
                    "norm" => crate::core::document::BlendMode::Normal,
                    "mul " => crate::core::document::BlendMode::Multiply,
                    other => panic!("reference blend {other}"),
                }
            );
            let rgba = std::fs::read(reference.join(format!("{i}.rgba"))).unwrap();
            assert_eq!(rgba.len() / 4, app.document.spec.pixel_count());
            for (index, expected) in rgba.chunks_exact(4).enumerate() {
                let actual =
                    graphite_studio::render::layer_pixel_rgba(&app.document, Some(i), index)
                        .map(|v| (v.clamp(0., 1.) * 255.).round() as u8);
                assert_eq!(actual[3], expected[3], "layer {i}, alpha {index}");
                if expected[3] != 0 {
                    assert_eq!(actual, expected, "layer {i}, pixel {index}");
                }
            }
            total += rgba.len() / 4;
            eprintln!(
                "Verified layer {i}: {} ({} pixels)",
                layer.name,
                rgba.len() / 4
            );
        }
        assert!(total > 32_000_000);
        assert!(app.prepared_preview.is_some());
        eprintln!(
            "Verified {total} layer pixels through background opening in {:?}.",
            start.elapsed()
        );
    }

    #[test]
    fn progress_ui_keeps_painting_while_worker_is_blocked_and_handles_panic() {
        let mut app = GraphiteApp::with_render_state(None);
        app.replace_document(CanvasSpec::from_physical("Existing", 10., 10., 120.));
        let original = app.document.surface.graphite_mass.clone();
        let (release, gate) = mpsc::channel();
        let (started, ready) = mpsc::channel();
        let ui_thread = std::thread::current().id();
        app.opening = Some(
            OpeningJob::spawn("example.psd".into(), move |_, progress| {
                assert_ne!(ui_thread, std::thread::current().id());
                progress(OpenProgress {
                    fraction: 0.6,
                    stage: "Preparing layers…",
                });
                started.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(15)).unwrap();
                panic!("simulated decoder failure");
            })
            .unwrap(),
        );
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
        let ctx = egui::Context::default();
        let mut preview = crate::ui::test_render::Preview::default();
        let mut output = None;
        for _ in 0..20 {
            let frame = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440., 920.))),
                    ..Default::default()
                },
                |ui| app.interface(ui),
            );
            preview.update(&frame);
            output = Some(frame);
        }
        assert!(app.opening.is_some());
        // A second request cannot replace or queue over the in-flight document.
        app.open_file_with_feedback(Path::new("another.png"));
        assert_eq!(app.opening.as_ref().unwrap().path, Path::new("example.psd"));
        let output = output.unwrap();
        assert!(output.shapes.iter().any(
            |s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.job.text == "Preparing layers…")
        ));
        preview.save(&ctx, &output, "../../ui-v2410-opening.png", [1440, 920]);
        for pressed in [true, false] {
            let _ = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440., 920.))),
                    events: vec![
                        egui::Event::PointerMoved(Pos2::new(700., 300.)),
                        egui::Event::PointerButton {
                            pos: Pos2::new(700., 300.),
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ui| app.interface(ui),
            );
        }
        assert_eq!(app.document.surface.graphite_mass, original);
        release.send(()).unwrap();
        app.wait_for_opening();
        assert!(app
            .open_error
            .as_ref()
            .unwrap()
            .contains("could not be processed"));
        assert!(app.recent_files.files().is_empty());
    }
}
