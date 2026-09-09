use eframe::egui::{
    self, Color32, Pos2, Rect, Sense, Stroke, TextureFilter, TextureHandle, TextureOptions,
    TextureWrapMode, Vec2,
};
mod editing;
mod workspace;
mod liquify;
#[cfg(test)]
mod workspace_tests;
mod options;
mod projects;
mod opening;
#[cfg(test)]
mod pencil_hold_tests;
mod quick_controls;
mod gallery;
mod paper_settings;
mod shortcuts_ui;
#[cfg(test)]
mod recent_files_tests;
use graphite_studio::shortcuts::Command;

use crate::{
    core::{
        contact::PencilTipContact,
        document::{CanvasSpec, Document},
        history::{EditTransaction, History},
        paper::{
            generate_builtin_albedo, load_custom_texture_source, CustomPaperTextureSource,
            PaperPreset, PaperTexturePreset,
        },
        pencil::{PencilTipState, ToolKind, ToolSettings},
        smoothing::LineSmoother,
        smudge::smudge_pressure_radius_scale,
        stroke::{StrokeEngine, StrokePoint},
        viewport::ViewportState,
    },
    export::{save_rendered_document, PsdBitDepth, SaveFormat, SaveOptions},
    input::{PointerFrame, PressureInputState},
    render::{DocumentRenderer, GpuDisplayPyramid, RasterRenderer},
    ui::{
        show_layer_panel, show_tool_panel, show_topbar, CanvasPreset, LayerPanelAction,
        PaperTextureChoice, ToolPanelAction, TopbarAction,
    },
};

const CPU_FALLBACK_TEXTURE_OPTIONS: TextureOptions = TextureOptions {
    // Used only if the WGPU display-pyramid path cannot be initialized. The primary preview path
    // owns its mip chain explicitly and does not depend on egui TextureOptions mipmap behavior.
    // Match the GPU sampler, including the lightweight Intel/OpenGL display path.
    magnification: TextureFilter::Linear,
    minification: TextureFilter::Linear,
    wrap_mode: TextureWrapMode::ClampToEdge,
    mipmap_mode: None,
};

struct StrokeSession {
    settings: ToolSettings,
    selection: crate::core::selection::Selection,
    points: Vec<StrokePoint>,
    initial_tip: PencilTipState,
    initial_engine: StrokeEngine,
    /// Most recent stabilized control point (raw when smoothing is off).
    last_raw: StrokePoint,
    smoother: LineSmoother,
    /// Last point already committed to the smoothed quadratic path.
    curve_cursor: StrokePoint,
    transaction: EditTransaction,
    device_pressure_used: bool,
    device_orientation_used: bool,
}

struct DrawingState {
    document: Document,
    history: History,
    viewport: ViewportState,
    stroke_engine: StrokeEngine,
    settings: ToolSettings,
    tip_state: PencilTipState,
    canvas_preset: CanvasPreset,
    paper_texture_choice: PaperTextureChoice,
    custom_paper_texture: Option<CustomPaperTextureSource>,
    new_document_dpi: f32,
    psd_bit_depth: PsdBitDepth,
    fit_requested: bool,
    status: String,
}

struct DrawingTab {
    project_path: Option<std::path::PathBuf>,
    modified: bool,
    id: u64,
    title: String,
    stored: Option<DrawingState>,
}

pub struct GraphiteApp {
    workspace_canvas: Option<Rect>,
    liquify: liquify::LiquifyUi,
    workspace_view_size: Vec2,
    fullscreen: bool,
    fullscreen_applied: bool,
    fullscreen_size: Option<Vec2>,
    rotate_view: bool,
    view_rotation_drag: bool,
    mouse_rotation: crate::core::viewport::RotationGesture,
    touch_rotation: Option<crate::core::viewport::RotationGesture>,
    new_landscape: bool,
    acceleration: crate::performance::AccelerationMode,
    startup_acceleration: crate::performance::AccelerationMode,
    renderer_label: String,
    last_texture_update: Option<std::time::Instant>,
    last_statistics_update: Option<std::time::Instant>,
    preference_status: String,
    recent_files: graphite_studio::recent_files::RecentFiles,
    preferences_path: Option<std::path::PathBuf>,
    editing: editing::EditingState,
    quick_controls: quick_controls::QuickControls,
    shortcuts: shortcuts_ui::ShortcutEditor,
    brush_library: Vec<std::sync::Arc<crate::core::brush::BrushTip>>,
    gallery: gallery::GalleryUi,
    paper_settings: paper_settings::PaperSettingsUi,
    opening: Option<opening::OpeningJob>,
    open_error: Option<String>,
    prepared_preview: Option<(u64, egui::ColorImage)>,
    save_as_open: bool,
    save_as_format: projects::SaveAsFormat,
    shape_drag: Option<(StrokePoint, StrokePoint)>,
    #[cfg(windows)]
    wintab: crate::wintab::WintabBridge,
    tabs: Vec<DrawingTab>,
    active_tab: usize,
    next_tab_id: u64,
    close_tab_request: Option<usize>,
    material_panel_visible: bool,
    material_panel_width: f32,
    #[cfg(windows)]
    window_recovery: crate::window_recovery::WindowRecovery,
    #[cfg(windows)]
    dialog_parent: Option<std::sync::Arc<winit::window::Window>>,
    document: Document,
    renderer: RasterRenderer,
    wgpu_render_state: Option<egui_wgpu::RenderState>,
    display_pyramid: Option<GpuDisplayPyramid>,
    fallback_texture: Option<TextureHandle>,
    viewport: ViewportState,
    stroke_engine: StrokeEngine,
    stroke_session: Option<StrokeSession>,
    pressure_input: PressureInputState,
    history: History,
    settings: ToolSettings,
    tip_state: PencilTipState,
    canvas_preset: CanvasPreset,
    paper_texture_choice: PaperTextureChoice,
    custom_paper_texture: Option<CustomPaperTextureSource>,
    new_document_dpi: f32,
    psd_bit_depth: PsdBitDepth,
    layers_panel_visible: bool,
    layers_panel_width: f32,
    fit_requested: bool,
    status: String,
    sidebar_statistics: (u64, f32, f32),
}

impl GraphiteApp {
    /// The native owner determines modal stacking and the current monitor.
    /// Keep the actual window alive, rather than caching a screen or raw HWND.
    fn file_dialog(&self) -> rfd::FileDialog {
        let dialog = rfd::FileDialog::new();
        #[cfg(windows)]
        if let Some(parent) = &self.dialog_parent {
            return dialog.set_parent(parent.as_ref());
        }
        dialog // Headless tests and non-Windows builds have no Windows owner.
    }

    fn prepare_vector_base(&mut self) {
        let active = self.document.active_layer_index();
        if self.document.layers[active].vectors.strokes.is_empty()
            && self.document.layers[active].vectors.base.is_none()
        {
            self.document.layers[active].vectors = std::sync::Arc::new(
                crate::core::vector::VectorLayer::raster_base(&self.document),
            );
        }
    }

    fn import_brushes(&mut self) {
        self.finish_stroke();
        self.apply_transform();
        let Some(path) = self.file_dialog()
            .add_filter("Photoshop sampled brushes", &["abr"])
            .pick_file()
        else {
            return;
        };
        let load = || -> Result<_, String> {
            let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
            if size > 128 * 1024 * 1024 {
                return Err("ABR file exceeds 128 MB".into());
            }
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            crate::core::brush::import_abr(
                &bytes,
                path.file_stem().and_then(|s| s.to_str()).unwrap_or("Brush"),
            )
        };
        match load() {
            Ok(tips) => {
                if let Err(error) = self.gallery_import(&tips, path.file_stem().and_then(|s|s.to_str()).unwrap_or("Imported")) {
                    self.status = format!("Pencil texture import: {error}");
                    self.gallery.message = self.status.clone();
                    self.open_pencil_gallery();
                    return;
                }
                self.status = format!(
                    "Imported {} pencil textures. These use Graphite's pressure, tilt and paper response.",
                    tips.len()
                );
                self.brush_library = tips;
            }
            Err(error) => {
                self.status = format!("Pencil texture import: {error}");
                self.gallery.message = self.status.clone();
                self.open_pencil_gallery();
            }
        }
    }

    fn handle_shape_input(&mut self, ui: &egui::Ui, canvas: Rect, input: PointerFrame) {
        if input.wants_pan()
            || ui.input(|i| i.key_pressed(egui::Key::Escape))
            || !ui.input(|i| i.focused)
        {
            self.shape_drag = None;
            return;
        }
        if let Some(pos) = input.position {
            let p = self.viewport.screen_to_document(canvas, pos);
            let point = StrokePoint {
                x: p.x,
                y: p.y,
                pressure: input.pressure,
                tilt_deg: input.tilt_deg.unwrap_or(self.settings.tilt_deg),
                azimuth_deg: input
                    .azimuth_deg
                    .map(|a| a - self.viewport.rotation.to_degrees())
                    .unwrap_or(self.settings.azimuth_deg),
                rotation_deg: input.rotation_deg,
            };
            if input.primary_pressed && ui.clip_rect().contains(pos) {
                self.shape_drag = Some((point, point));
            } else if let Some((start, end)) = &mut self.shape_drag {
                if input.primary_down || input.primary_released {
                    // Pen-down can report zero force before contact settles. Use the
                    // strongest contact during placement, never pen-up's zero force.
                    if !input.primary_released {
                        start.pressure = start.pressure.max(point.pressure);
                    }
                    *end = point;
                }
            }
        }
        if input.primary_released {
            if let Some((mut start, end)) = self.shape_drag.take() {
                if start.pressure <= 1.0e-4 {
                    start.pressure = self.settings.mouse_pressure;
                }
                let mut points = self
                    .settings
                    .shape
                    .points_with_snap(Vec2::new(start.x, start.y), Vec2::new(end.x, end.y), self.settings.snap_shape(ui.input(|i| i.modifiers.shift)));
                let mut settings = self.settings.clone();
                settings.tool = ToolKind::Pencil;
                // Scale the current physical pencil footprint to the requested outline width.
                // Keep grade, pressure, pigment, grain and tip profile identical to the pencil.
                let contact = crate::core::contact::PencilTipContact::from_state_with_sharpness(
                    self.tip_state
                        .effective_core_diameter_px(self.document.spec.dpi)
                        * settings.pressure_width_scale(start.pressure),
                    start.pressure,
                    start.tilt_deg,
                    start.azimuth_deg,
                    settings.grade.formulation().core_hardness,
                    settings.tip_sharpness,
                );
                settings.pencil_geometry_scale =
                    settings.shape_width_px / (2. * contact.cross_radius_px);
                let extent=(contact.rear_extension_px+contact.cross_radius_px+contact.point_radius_px)
                    *settings.pencil_geometry_scale+4.;
                let mut bounds=Rect::NOTHING;
                for p in &points {bounds.extend_with(p.to_pos2());}
                let shift=self.grow_workspace(bounds.expand(extent));
                for p in &mut points {*p+=shift;}
                self.prepare_vector_base();
                let record = crate::core::vector::VectorStroke {
                    shape: None,
                    label: self.settings.shape.label().into(),
                    settings: settings.clone(),
                    tip: self.tip_state.clone(),
                    engine: self.stroke_engine.clone(),
                    points: points
                        .iter()
                        .map(|p| StrokePoint {
                            x: p.x,
                            y: p.y,
                            ..start
                        })
                        .collect(),
                    polyline: true,
                    selection: self.document.selection.clone(),
                };
                let mut tx = EditTransaction::default();
                record.apply(&mut self.document, &mut tx);
                if !tx.is_empty() {
                    let active = self.document.active_layer_index();
                    std::sync::Arc::make_mut(&mut self.document.layers[active].vectors)
                        .strokes
                        .push(std::sync::Arc::new(record));
                    self.history.push(tx, &self.document);
                    self.tabs[self.active_tab].modified = true;
                    // The preview disappears this frame; upload committed material
                    // on the next frame even when the pointer stops moving.
                    ui.ctx().request_repaint();
                    self.status = format!(
                        "{} · {}",
                        self.settings.shape.label(),
                        self.document.active_layer_name()
                    );
                }
            }
        }
        if self.shape_drag.is_some() {
            ui.ctx().request_repaint();
        }
    }

    /// The active physical surface owns several full-resolution f32 channels before renderer and
    /// history overhead. Inactive drawing layers are sparse-tiled, but the base document still
    /// needs a guard against accidental very-high-DPI allocations.

    pub fn new(
        cc: &eframe::CreationContext<'_>,
        mode: crate::performance::AccelerationMode,
    ) -> Self {
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        cc.egui_ctx.set_zoom_factor(1.0);
        let mut app = Self::with_render_state(cc.wgpu_render_state.clone());
        app.preferences_path=crate::performance::Preferences::path();
        if let Some(preferences)=app.preferences_path.as_deref().and_then(crate::performance::Preferences::load_from) {
            app.recent_files=preferences.recent_files;
        }
        #[cfg(windows)]
        { app.dialog_parent = cc.winit_window().cloned(); }
        app.acceleration = mode;
        app.startup_acceleration = mode;
        app.gallery.load();
        app.restore_pencil_preferences();
        if let Some(state) = &cc.wgpu_render_state {
            let info = state.adapter.get_info();
            app.renderer_label = format!("{} · {:?}", info.name, info.backend);
            if info.device_type!=egui_wgpu::wgpu::DeviceType::Cpu {
                if let Err(error)=crate::core::stroke_gpu::install(state.device.clone(),state.queue.clone()){
                    app.status=format!("GPU stroke compute unavailable; CPU drawing active. {error}");
                }
            }
        } else if let Some(gl) = &cc.gl {
            use eframe::glow::HasContext;
            // The creation context owns a current GL context on this thread.
            app.renderer_label = unsafe {
                format!(
                    "{} · OpenGL {}",
                    gl.get_parameter_string(eframe::glow::RENDERER),
                    gl.get_parameter_string(eframe::glow::VERSION)
                )
            };
        }
        app.apply_display_style(&cc.egui_ctx);
        app
    }

    fn with_render_state(render_state: Option<egui_wgpu::RenderState>) -> Self {
        let spec = CanvasSpec::a4(120.0);
        let paper_texture_choice = PaperTextureChoice::White;
        let paper_albedo = generate_builtin_albedo(
            spec.width_px,
            spec.height_px,
            spec.dpi,
            PaperTexturePreset::White,
        );
        let document = Document::new(spec, PaperPreset::DrawingMedium, "White", paper_albedo);
        let settings = ToolSettings { pressure_width: 0.6, tip_sharpness: 0.65, ..Default::default() };
        let tip_state = PencilTipState::fresh_for_formulation(
            settings.pencil_core_diameter_mm,
            settings.grade.formulation(),
        );
        Self {
            acceleration: Default::default(),
            startup_acceleration: Default::default(),
            renderer_label: "egui display".into(),
            last_texture_update: None,
            last_statistics_update: None,
            preference_status: String::new(),
            recent_files: Default::default(),
            preferences_path: None,
            editing: Default::default(),
            quick_controls: Default::default(),
            shortcuts: Default::default(),
            brush_library: Vec::new(),
            gallery: Default::default(),
            paper_settings: Default::default(),
            opening: None,
            open_error: None,
            prepared_preview: None,
            save_as_open: false,
            save_as_format: Default::default(),
            shape_drag: None,
            #[cfg(windows)]
            wintab: Default::default(),
            tabs: vec![DrawingTab {
                project_path: None,
                modified: true,
                id: 1,
                title: "Drawing 1".into(),
                stored: None,
            }],
            active_tab: 0,
            next_tab_id: 2,
            close_tab_request: None,
            material_panel_visible: true,
            workspace_canvas: None,
            liquify: Default::default(),
            workspace_view_size: Vec2::new(1000.,800.),
            fullscreen: false,
            fullscreen_applied: false,
            fullscreen_size: None,
            rotate_view: false,
            view_rotation_drag: false,
            mouse_rotation: Default::default(),
            touch_rotation: None,
            new_landscape: false,
            material_panel_width: 282.0,
            #[cfg(windows)]
            window_recovery: Default::default(),
            #[cfg(windows)]
            dialog_parent: None,
            document,
            renderer: RasterRenderer::default(),
            wgpu_render_state: render_state,
            display_pyramid: None,
            fallback_texture: None,
            viewport: ViewportState::default(),
            stroke_engine: StrokeEngine::default(),
            stroke_session: None,
            pressure_input: PressureInputState::default(),
            history: History::default(),
            settings,
            tip_state,
            canvas_preset: CanvasPreset::A4,
            paper_texture_choice,
            custom_paper_texture: None,
            new_document_dpi: 120.0,
            psd_bit_depth: PsdBitDepth::Sixteen,
            layers_panel_visible: true,
            layers_panel_width: 250.,
            fit_requested: true,
            sidebar_statistics: (0, 0.0, 0.0),
            status: "Ready. Graphite Studio v0.24.16".to_owned(),
        }
    }

    fn canvas_spec_for(preset: CanvasPreset, dpi: f32, landscape: bool) -> CanvasSpec {
        let dpi = dpi.clamp(36.0, 600.0);
        let mut spec = match preset {
            CanvasPreset::A5 => CanvasSpec::a5(dpi),
            CanvasPreset::A4 | CanvasPreset::Custom => CanvasSpec::a4(dpi),
            CanvasPreset::Letter => CanvasSpec::letter(dpi),
        };
        if landscape {
            std::mem::swap(&mut spec.width_px, &mut spec.height_px);
            std::mem::swap(&mut spec.width_mm, &mut spec.height_mm);
        }
        spec
    }

    fn resolve_paper_texture(&self, spec: &CanvasSpec) -> (String, Vec<[f32; 3]>) {
        match self.paper_texture_choice {
            PaperTextureChoice::White => (
                PaperTexturePreset::White.label().to_owned(),
                generate_builtin_albedo(
                    spec.width_px,
                    spec.height_px,
                    spec.dpi,
                    PaperTexturePreset::White,
                ),
            ),
            PaperTextureChoice::Recycled => (
                PaperTexturePreset::Recycled.label().to_owned(),
                generate_builtin_albedo(
                    spec.width_px,
                    spec.height_px,
                    spec.dpi,
                    PaperTexturePreset::Recycled,
                ),
            ),
            PaperTextureChoice::Ivory => (
                PaperTexturePreset::Ivory.label().to_owned(),
                generate_builtin_albedo(
                    spec.width_px,
                    spec.height_px,
                    spec.dpi,
                    PaperTexturePreset::Ivory,
                ),
            ),
            PaperTextureChoice::Custom => {
                if let Some(custom) = &self.custom_paper_texture {
                    (
                        custom.label(),
                        custom.render_albedo(spec.width_px, spec.height_px),
                    )
                } else {
                    (
                        PaperTexturePreset::White.label().to_owned(),
                        generate_builtin_albedo(
                            spec.width_px,
                            spec.height_px,
                            spec.dpi,
                            PaperTexturePreset::White,
                        ),
                    )
                }
            }
        }
    }

    fn apply_selected_paper_texture(&mut self) {
        let (label, albedo) = self.resolve_paper_texture(&self.document.spec);
        self.document.set_paper_texture(label.clone(), albedo);
        self.tabs[self.active_tab].modified = true;
        self.status = format!("Applied paper texture: {label}.");
    }

    fn load_custom_paper_texture(&mut self) {
        let fallback_choice = match self.document.paper_texture_label.as_str() {
            "Recycled" => PaperTextureChoice::Recycled,
            "Ivory" => PaperTextureChoice::Ivory,
            _ => PaperTextureChoice::White,
        };
        let Some(path) = self.file_dialog()
            .set_title("Load custom paper texture")
            .add_filter("Image", &["png", "jpg", "jpeg", "bmp"])
            .pick_file()
        else {
            if self.custom_paper_texture.is_none()
                && self.paper_texture_choice == PaperTextureChoice::Custom
            {
                self.paper_texture_choice = fallback_choice;
            }
            return;
        };

        match load_custom_texture_source(&path) {
            Ok(custom) => {
                let label = custom.label();
                self.custom_paper_texture = Some(custom);
                self.paper_texture_choice = PaperTextureChoice::Custom;
                self.apply_selected_paper_texture();
                self.status = format!("Loaded custom paper texture: {label}.");
            }
            Err(error) => {
                if self.custom_paper_texture.is_none()
                    && self.paper_texture_choice == PaperTextureChoice::Custom
                {
                    self.paper_texture_choice = PaperTextureChoice::White;
                }
                self.status = format!("Could not load custom paper texture: {error}");
            }
        }
    }

    fn replace_document(&mut self, spec: CanvasSpec) {
        if let Err(error) = graphite_studio::limits::canvas_pixels(spec.width_px, spec.height_px) {
            self.status = format!("Document not created: {error}");
            return;
        }
        let (paper_texture_label, paper_albedo) = self.resolve_paper_texture(&spec);
        let state = DrawingState {
            document: Document::new(
                spec,
                PaperPreset::DrawingMedium,
                paper_texture_label,
                paper_albedo,
            ),
            history: History::default(),
            viewport: ViewportState::default(),
            stroke_engine: StrokeEngine::default(),
            settings: self.settings.clone(),
            tip_state: PencilTipState::fresh_for_formulation(
                self.settings.pencil_core_diameter_mm,
                self.settings.grade.formulation(),
            ),
            canvas_preset: self.canvas_preset,
            paper_texture_choice: self.paper_texture_choice,
            custom_paper_texture: self.custom_paper_texture.clone(),
            new_document_dpi: self.new_document_dpi,
            psd_bit_depth: self.psd_bit_depth,
            fit_requested: true,
            status: "New drawing.".into(),
        };
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        self.tabs.push(DrawingTab {
            project_path: None,
            modified: true,
            id,
            title: format!("Drawing {id}"),
            stored: Some(state),
        });
        self.activate_tab(self.tabs.len() - 1);
    }

    fn activate_tab(&mut self, index: usize) {
        if index == self.active_tab || index >= self.tabs.len() {
            return;
        }
        self.finish_liquify(true);
        self.finish_stroke();
        self.cancel_transform();
        self.editing.lasso = None;
        self.editing.selected_path = None;
        self.shape_drag = None;
        let Some(mut state) = self.tabs[index].stored.take() else {
            return;
        };
        std::mem::swap(&mut self.document, &mut state.document);
        std::mem::swap(&mut self.history, &mut state.history);
        std::mem::swap(&mut self.viewport, &mut state.viewport);
        std::mem::swap(&mut self.stroke_engine, &mut state.stroke_engine);
        std::mem::swap(&mut self.settings, &mut state.settings);
        std::mem::swap(&mut self.tip_state, &mut state.tip_state);
        std::mem::swap(&mut self.canvas_preset, &mut state.canvas_preset);
        std::mem::swap(
            &mut self.paper_texture_choice,
            &mut state.paper_texture_choice,
        );
        std::mem::swap(
            &mut self.custom_paper_texture,
            &mut state.custom_paper_texture,
        );
        std::mem::swap(&mut self.new_document_dpi, &mut state.new_document_dpi);
        std::mem::swap(&mut self.psd_bit_depth, &mut state.psd_bit_depth);
        std::mem::swap(&mut self.fit_requested, &mut state.fit_requested);
        std::mem::swap(&mut self.status, &mut state.status);

        self.tabs[self.active_tab].stored = Some(state);
        self.active_tab = index;
        self.pressure_input = PressureInputState::default();
        self.workspace_canvas=None;
        self.renderer.reset();
        self.prepared_preview = None;
        self.display_pyramid = None;
        self.fallback_texture = None;
        self.sidebar_statistics = (0, 0.0, 0.0);
        self.document.mark_all_dirty();
    }

    fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        if self.tabs.len() == 1 {
            // Keep a usable workspace after the final drawing is closed.
            self.replace_document(self.document.spec.clone());
            if self.tabs.len() == 1 {
                return;
            }
        }
        if index == self.active_tab {
            self.activate_tab(if index == 0 { 1 } else { index - 1 });
        }
        self.tabs.remove(index);
        if self.active_tab > index {
            self.active_tab -= 1;
        }
    }

    fn reorder_tab(&mut self, id: u64, target: u64, before: bool) {
        if id == target {
            return;
        }
        let active = self.tabs[self.active_tab].id;
        let Some(from) = self.tabs.iter().position(|t| t.id == id) else {
            return;
        };
        if !self.tabs.iter().any(|t| t.id == target) {
            return;
        }
        let tab = self.tabs.remove(from);
        let at = self.tabs.iter().position(|t| t.id == target).unwrap() + usize::from(!before);
        self.tabs.insert(at, tab);
        self.active_tab = self.tabs.iter().position(|t| t.id == active).unwrap();
    }

    fn show_drawing_tabs(&mut self, ui: &mut egui::Ui) {
        use crate::ui::document_tabs::{self, TabAction, TabView};
        let tabs: Vec<_> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let (doc, view) = if i == self.active_tab {
                    (&self.document, &self.viewport)
                } else {
                    let state = t.stored.as_ref().unwrap();
                    (&state.document, &state.viewport)
                };
                TabView {
                    id: t.id,
                    title: t.title.clone(),
                    zoom: view.zoom,
                    layer: doc.active_layer_name().to_owned(),
                    modified: t.modified,
                }
            })
            .collect();
        let action = document_tabs::show(ui, &tabs, self.tabs[self.active_tab].id);
        match action {
            TabAction::None => {}
            TabAction::Activate(id) => {
                if let Some(i) = self.tabs.iter().position(|t| t.id == id) {
                    self.activate_tab(i);
                }
            }
            TabAction::Close(id) => {
                self.close_tab_request = self.tabs.iter().position(|t| t.id == id);
            }
            TabAction::New => self.replace_document(self.configured_paper_spec(self.canvas_preset, self.new_document_dpi)),
            TabAction::Reorder(id, target, before) => self.reorder_tab(id, target, before),
        }
        if let Some(index) = self.close_tab_request {
            let mut close = false;
            let mut cancel = false;
            egui::Modal::new(egui::Id::new("close_drawing")).show(ui.ctx(),|ui| {
                ui.heading(format!("Close {}?",self.tabs[index].title));
                ui.label("Save an editable PSD or .graphite project to keep paths, layers and tools. PNG / JPEG / BMP retain pixels only. Closing removes the current undo history.");
                ui.horizontal(|ui| {
                    cancel=ui.button("Keep drawing").clicked();
                    close=ui.button("Close drawing").clicked();
                });
            });
            if close {
                self.close_tab(index);
            }
            if close || cancel {
                self.close_tab_request = None;
            }
        }
    }

    fn ensure_texture(&mut self, ctx: &egui::Context) {
        if self.display_pyramid.is_none() && self.fallback_texture.is_none() {
            let image = self.prepared_preview.take()
                .filter(|(revision, _)| *revision == self.document.revision())
                .map(|(_, image)| image)
                .unwrap_or_else(|| self.renderer.render_full(&self.document));

            let mut gpu_error = None;
            if let Some(render_state) = self
                .wgpu_render_state
                .clone()
                .filter(|_| self.acceleration != crate::performance::AccelerationMode::IntelHd)
            {
                match GpuDisplayPyramid::new(render_state, &image) {
                    Ok(pyramid) => {
                        let levels = pyramid.mip_count();
                        self.display_pyramid = Some(pyramid);
                        self.status = format!(
                            "GPU linear-light display reconstruction active ({levels} pyramid levels)."
                        );
                    }
                    Err(error) => gpu_error = Some(error),
                }
            }

            if self.display_pyramid.is_none() {
                self.fallback_texture = Some(ctx.load_texture(
                    "graphite_document_cpu_fallback",
                    image,
                    CPU_FALLBACK_TEXTURE_OPTIONS,
                ));
                if let Some(error) = gpu_error {
                    self.status = format!(
                        "GPU display pyramid unavailable ({error}); using CPU/egui fallback filtering."
                    );
                } else {
                    self.status = format!(
                        "{} · direct canvas display active.",
                        self.acceleration.label()
                    );
                }
            }

            let _ = self.document.take_dirty();
            self.last_texture_update = Some(std::time::Instant::now());
            return;
        }

        // Dirty rectangles stay queued while shading/uploads are limited. Every input event
        // still updates physical material. Pen-up bypasses the delay and flushes the final mark.
        if self.stroke_session.is_some() {
            if let Some(last) = self.last_texture_update {
                let remaining = self
                    .acceleration
                    .update_interval()
                    .saturating_sub(last.elapsed());
                if !remaining.is_zero() {
                    ctx.request_repaint_after(remaining);
                    return;
                }
            }
        }
        let Some(dirty) = self.document.take_dirty() else {
            return;
        };
        if dirty.is_empty() {
            return;
        }

        let full_size = [self.document.spec.width_px, self.document.spec.height_px];
        let gpu_size_mismatch = self
            .display_pyramid
            .as_ref()
            .is_some_and(|pyramid| pyramid.size() != full_size);
        let fallback_size_mismatch = self
            .fallback_texture
            .as_ref()
            .is_some_and(|texture| texture.size() != full_size);

        if gpu_size_mismatch || fallback_size_mismatch {
            self.display_pyramid = None;
            self.fallback_texture = None;
            self.sidebar_statistics.0 = 0;
            self.renderer.reset();
            self.document.mark_all_dirty();
            self.ensure_texture(ctx);
            return;
        }

        let patch = self.renderer.render_region(&self.document, dirty);
        self.last_texture_update = Some(std::time::Instant::now());
        let gpu_update = self
            .display_pyramid
            .as_mut()
            .map(|pyramid| pyramid.upload_patch([dirty.min_x, dirty.min_y], &patch));
        if let Some(result) = gpu_update {
            if let Err(error) = result {
                // A GPU update failure should not destroy the user's session. Fall back to a full
                // egui texture and keep the underlying simulation/document untouched.
                let image = self.renderer.render_full(&self.document);
                self.display_pyramid = None;
                self.fallback_texture = Some(ctx.load_texture(
                    "graphite_document_cpu_fallback",
                    image,
                    CPU_FALLBACK_TEXTURE_OPTIONS,
                ));
                self.status = format!(
                    "GPU display-pyramid update failed ({error}); switched to CPU/egui fallback."
                );
            }
            return;
        }

        if let Some(texture) = &mut self.fallback_texture {
            texture.set_partial(
                [dirty.min_x, dirty.min_y],
                patch,
                CPU_FALLBACK_TEXTURE_OPTIONS,
            );
        }
    }

    fn document_texture_id(&self) -> Option<egui::TextureId> {
        self.display_pyramid
            .as_ref()
            .map(GpuDisplayPyramid::texture_id)
            .or_else(|| self.fallback_texture.as_ref().map(TextureHandle::id))
    }

    fn handle_topbar_action(&mut self, action: TopbarAction) {
        if !matches!(action,TopbarAction::None|TopbarAction::Fit|TopbarAction::Fullscreen){self.finish_liquify(true);}
        if let TopbarAction::RecentFilesMaximum(maximum)=action {self.set_recent_files_maximum(maximum);return;}
        if action == TopbarAction::Shortcuts {self.open_shortcuts();return;}
        if action == TopbarAction::PencilGallery {self.open_pencil_gallery();return;}
        if action == TopbarAction::PaperSettings {
            self.finish_stroke();self.apply_transform();self.gallery.open=false;self.paper_settings.open=true;return;
        }
        if let TopbarAction::CanvasOrientation(landscape) = action {
            self.new_landscape = landscape;
            self.finish_stroke();self.apply_transform();
            self.viewport.rotation = 0.;self.view_rotation_drag=false;self.touch_rotation=None;self.rotate_view=false;self.fit_requested=false;
            if (self.document.spec.width_px > self.document.spec.height_px) != landscape
                && self.document.spec.width_px != self.document.spec.height_px
            {
                self.finish_stroke();
                self.apply_transform();
                self.shape_drag = None;
                self.editing.lasso = None;
                self.history.rotate_document(&mut self.document, landscape);
                self.viewport.rotation = 0.;
                self.view_rotation_drag = false;
                self.touch_rotation = None;
                self.stroke_engine.clear_smudger();
                self.tabs[self.active_tab].modified = true;
                self.status = format!("{} drawing. Paper, artwork, layers and paths rotated together; Ctrl+Z undoes it.", if landscape { "Landscape" } else { "Portrait" });
            }
            return;
        }
        if let TopbarAction::TabletBackend(use_wintab) = action {
            if self.settings.use_wintab != use_wintab {
                self.finish_stroke();
                self.shape_drag = None;
                self.editing.lasso = None;
                self.pressure_input = PressureInputState::default();
                self.settings.use_wintab = use_wintab;
                self.tabs[self.active_tab].modified = true;
                self.status = format!(
                    "Input backend: {}. Applies immediately.",
                    if use_wintab { "Wintab" } else { "Windows Ink" }
                );
            }
            return;
        }
        if action == TopbarAction::Fullscreen {
            self.finish_stroke();
            self.shape_drag = None;
            self.editing.guide = false;
            self.fullscreen = !self.fullscreen;
            self.fullscreen_size = None;
            self.fit_requested = true;
            return;
        }
        if action == TopbarAction::Fit {
            self.fit_requested = true;
            return;
        }
        if let TopbarAction::Acceleration(mode) = action {
            self.set_acceleration(mode);
            return;
        }
        if action != TopbarAction::None && self.editing.transform.is_some() {
            if matches!(
                action,
                TopbarAction::SaveProject | TopbarAction::SaveProjectAs
            ) {
                self.apply_transform();
            } else {
                self.cancel_transform();
            }
            if matches!(action, TopbarAction::Undo | TopbarAction::Redo) {
                return;
            }
        }
        match action {
            TopbarAction::None => {}
            TopbarAction::Shortcuts => unreachable!(),
            TopbarAction::PaperSettings => unreachable!(),
            TopbarAction::PencilGallery => unreachable!(),
            TopbarAction::Fullscreen => unreachable!(),
            TopbarAction::Acceleration(_) => unreachable!(),
            TopbarAction::TabletBackend(_) => unreachable!(),
            TopbarAction::CanvasOrientation(_) => unreachable!(),
            TopbarAction::NewCanvas { preset, dpi } => {
                self.replace_document(self.configured_paper_spec(preset, dpi));
            }
            TopbarAction::Undo => {
                let size = (self.document.spec.width_px, self.document.spec.height_px);
                if self.history.undo(&mut self.document) {
                    self.fit_requested |=
                        size != (self.document.spec.width_px, self.document.spec.height_px);
                    self.tabs[self.active_tab].modified = true;
                    self.stroke_engine.clear_smudger();
                    self.status = "Undid last edit; smudger reservoir reset.".to_owned();
                }
            }
            TopbarAction::Redo => {
                let size = (self.document.spec.width_px, self.document.spec.height_px);
                if self.history.redo(&mut self.document) {
                    self.fit_requested |=
                        size != (self.document.spec.width_px, self.document.spec.height_px);
                    self.tabs[self.active_tab].modified = true;
                    self.stroke_engine.clear_smudger();
                    self.status = "Redid edit; smudger reservoir reset.".to_owned();
                }
            }
            TopbarAction::Fit => self.fit_requested = true,

            TopbarAction::OpenProject => self.open_project(),
            TopbarAction::OpenRecent(index) => self.open_recent_project(index),
            TopbarAction::RecentFilesMaximum(_) => unreachable!(),
            TopbarAction::SaveProject => self.save_project(false),
            TopbarAction::SaveProjectAs => self.save_project(true),
            TopbarAction::ApplyPaperTexture => self.apply_selected_paper_texture(),
            TopbarAction::LoadCustomPaperTexture => self.load_custom_paper_texture(),
            TopbarAction::ToggleLayersPanel => {
                self.layers_panel_visible = !self.layers_panel_visible;
            }
        }
    }

    fn handle_layer_panel_action(&mut self, action: LayerPanelAction) {
        if let LayerPanelAction::SetMultiple(multiple)=action { self.document.layer_selection.multiple=multiple; return; }
        if action == LayerPanelAction::None {
            return;
        }
        self.finish_liquify(true);
        self.cancel_transform();
        self.shape_drag = None;
        self.editing.lasso = None;
        self.editing.selected_path = None;
        if self.stroke_session.is_some() {
            self.finish_stroke();
        }

        if !matches!(action, LayerPanelAction::Activate(_) | LayerPanelAction::Select {..}) {
            self.tabs[self.active_tab].modified = true;
        }
        match action {
            LayerPanelAction::None => {}
            LayerPanelAction::Activate(index) => {
                self.document.select_layer(index,false,false);
                self.stroke_engine.clear_smudger();
                self.status = format!("Active layer: {}.", self.document.active_layer_name());
            }
            LayerPanelAction::Select {index,toggle,range} => {
                self.document.select_layer(index,toggle,range);
                self.stroke_engine.clear_smudger();
                self.status=format!("{} layers selected. Active layer: {}.",self.document.selected_layer_indices().len(),self.document.active_layer_name());
            }
            LayerPanelAction::SetMultiple(_) => {}
            LayerPanelAction::Add => {
                self.document.add_layer();
                self.document.select_layer(self.document.active_layer_index(),false,false);
                self.stroke_engine.clear_smudger();
                self.status = format!("Added {}.", self.document.active_layer_name());
            }
            LayerPanelAction::RemoveActive => {
                let name = self.document.active_layer_name().to_owned();
                if self.document.remove_active_layer() {
                    self.document.select_layer(self.document.active_layer_index(),false,false);
                    // Stroke history entries can target the deleted layer, so clear history rather
                    // than leaving hidden invalid references in the undo stack.
                    self.history.clear();
                    self.stroke_engine.clear_smudger();
                    self.status = format!("Deleted {name}. Stroke undo history was reset after the structural layer change.");
                }
            }
            LayerPanelAction::MergeDown => {
                if self.history.merge_down(&mut self.document) {
                    self.stroke_engine.clear_smudger();
                    self.status = "Merged down into erasable graphite. Ctrl+Z restores both layers and editable paths.".into();
                }
            }
            LayerPanelAction::CopyActive => {
                if self.history.copy_layer(&mut self.document) {
                    self.stroke_engine.clear_smudger();
                    self.status=format!("Created {} above the original. Ctrl+Z undoes the copy.",self.document.active_layer_name());
                }
            }
            LayerPanelAction::MergeSelected => {
                let count=self.document.selected_layer_indices().len();
                if self.history.merge_selected(&mut self.document) {
                    self.stroke_engine.clear_smudger();
                    self.status=format!("Merged {count} layers. Ctrl+Z restores all original layers and editable paths.");
                }
            }
            LayerPanelAction::MoveActiveUp => {
                if self.document.move_active_layer_up() {
                    self.status = format!("Moved {} up.", self.document.active_layer_name());
                }
            }
            LayerPanelAction::MoveActiveDown => {
                if self.document.move_active_layer_down() {
                    self.status = format!("Moved {} down.", self.document.active_layer_name());
                }
            }
            LayerPanelAction::SetVisible { index, visible } => {
                if self.document.set_layer_visible(index, visible) {
                    let state = if visible { "Shown" } else { "Hidden" };
                    self.status = format!("{state} {}.", self.document.layers[index].name.as_str());
                }
            }
            LayerPanelAction::SetBlendMode { id, mode } => {
                if self.document.set_layer_blend_mode(id, mode) {
                    self.status = format!(
                        "{} blend mode: {}.",
                        self.document.active_layer_name(),
                        mode.label()
                    );
                }
            }
            LayerPanelAction::SetOpacity { id, opacity } => {
                if self.document.set_layer_opacity(id, opacity) {
                    self.status = format!(
                        "{} opacity: {:.0}%.",
                        self.document.active_layer_name(),
                        opacity as f32 * 100. / 255.
                    );
                }
            }
            LayerPanelAction::Reorder {
                dragged_id,
                target_id,
                above,
            } => {
                if self
                    .document
                    .move_layer_relative(dragged_id, target_id, above)
                {
                    self.status = "Layer order updated.".to_owned();
                }
            }
        }
    }

    fn handle_keyboard_shortcuts(&mut self, ui: &mut egui::Ui) {
        if self.shortcuts.open {return;}
        if self.liquify.session.is_some() && !ui.ctx().text_edit_focused() {
            if ui.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Escape)){self.liquify_request_finish(false);return;}
            if ui.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Enter)){self.liquify_request_finish(true);return;}
        }
        // Esc must always leave fullscreen, even if a field or transform had focus.
        if self.fullscreen
            && ui.input_mut(|i| self.shortcuts.bindings.consume(i,Command::Cancel))
        {
            self.cancel_transform_and_deselect();
            self.fullscreen = false;
            self.fullscreen_size = None;
            self.rotate_view = false;
            self.view_rotation_drag = false;
            self.fit_requested = true;
            return;
        }
        // Do not steal Ctrl+Z/Ctrl+Y from an actively edited numeric/text field.
        if self.shortcuts.open || self.gallery.open || self.paper_settings.open || self.save_as_open || ui.ctx().text_edit_focused() {
            return;
        }
        
        {
            let tool = ui.input_mut(|i| {
                if self.shortcuts.bindings.consume(i,Command::Pencil)
                    || self.shortcuts.bindings.consume(i,Command::PencilAlternate) {
                    Some(ToolKind::Pencil)
                } else if self.shortcuts.bindings.consume(i,Command::Eraser) {
                    Some(ToolKind::Eraser)
                } else if self.shortcuts.bindings.consume(i,Command::VectorSelect) {
                    Some(ToolKind::VectorSelect)
                } else {
                    None
                }
            });
            if let Some(tool) = tool {
                self.select_tool(tool);
                return;
            }
            if ui.input_mut(|i| self.shortcuts.bindings.consume(i,Command::RotateView)) {
                self.finish_stroke();
                self.cancel_transform_for_tool_change();
                self.shape_drag = None;
                self.editing.lasso = None;
                self.rotate_view = !self.rotate_view;
                self.view_rotation_drag = false;
                self.status = "Rotate paper: drag around its center. Shift snaps to 0°, 90°, 180° and 270°. R or Esc returns; P selects Pencil.".into();
                return;
            }
            if self.rotate_view
                && ui.input_mut(|i| self.shortcuts.bindings.consume(i,Command::Cancel))
            {
                self.rotate_view = false;
                self.view_rotation_drag = false;
                return;
            }
        }
        if self.editing_shortcuts(ui) {
            return;
        }
        for (command,action) in [(Command::Fit,TopbarAction::Fit),(Command::Fullscreen,TopbarAction::Fullscreen)] {
            if ui.input_mut(|i|self.shortcuts.bindings.consume(i,command)) {self.handle_topbar_action(action);return;}
        }

        let action = ui.input_mut(|input| {
            
            if self.shortcuts.bindings.consume(input,Command::SaveAs) {
                TopbarAction::SaveProjectAs
            } else if self.shortcuts.bindings.consume(input,Command::Save) {
                TopbarAction::SaveProject
            } else if self.shortcuts.bindings.consume(input,Command::Open) {
                TopbarAction::OpenProject
            } else if self.shortcuts.bindings.consume(input,Command::Undo) {
                TopbarAction::Undo
            } else if self.shortcuts.bindings.consume(input,Command::Redo) {
                TopbarAction::Redo
            } else {
                TopbarAction::None
            }
        });

        if action != TopbarAction::None {
            // If a shortcut is pressed while a stroke is still active, commit that stroke first so
            // Ctrl+Z always removes exactly what the user can currently see.
            if self.stroke_session.is_some() {
                self.finish_stroke();
            }
            self.handle_topbar_action(action);
            ui.ctx().request_repaint();
        }
    }

    fn save_image_path(&mut self, path: &std::path::Path) {
        if SaveFormat::from_path(path).is_none() {
            self.status = "Save failed: use .psd, .graphite, .png, .bmp or .jpg/.jpeg.".into();
            return;
        }
        let options = SaveOptions {
            psd_bit_depth: self.psd_bit_depth,
        };
        let saved = save_rendered_document(&mut self.renderer, &self.document, &path, options);
        if saved.is_ok() {self.remember_recent_file(path);}
        if saved.is_ok() && self.tabs[self.active_tab].project_path.is_none() {
            if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
                self.tabs[self.active_tab].title = name.to_owned();
            }
        }
        self.status = match saved {
            Ok(SaveFormat::Psd) => format!(
                "Saved PSD ({} bits/channel) to {}",
                self.psd_bit_depth.bits(),
                path.display()
            ),
            Ok(format) => format!("Saved {} to {}", format.label(), path.display()),
            Err(error) => format!("Save failed: {error}"),
        };
    }

    fn draw_canvas(&mut self, ui: &mut egui::Ui) {
        let raw_available = ui.available_size();
        self.workspace_view_size=ui.clip_rect().size();
        let available = Vec2::new(raw_available.x.max(120.0), raw_available.y.max(120.0));
        if self.fullscreen && self.fullscreen_size != Some(available) {
            self.fullscreen_size = Some(available);
            self.fit_requested = true;
        }
        let doc_size = Vec2::new(
            self.document.spec.width_px as f32,
            self.document.spec.height_px as f32,
        );

        let reset_scroll = self.fit_requested;
        if self.fit_requested {
            self.viewport.fit(available, doc_size);
            self.fit_requested = false;
        }

        let mut area = egui::ScrollArea::both()
            .animated(false)
            .id_salt(("paper_workspace_scroll", self.tabs[self.active_tab].id))
            .auto_shrink([false, false])
            .scroll_bar_visibility(
                egui::containers::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
            )
            // The wheel remains dedicated to cursor-centered zoom. The bars themselves are native
            // egui scrollbars; mouse/touch content drag is handled explicitly below so left drag
            // stays reserved for drawing.
            .scroll_source(egui::containers::scroll_area::ScrollSource::SCROLL_BAR);
        if reset_scroll {
            area = area.scroll_offset(Vec2::ZERO);
        }

        area.show_viewport(ui, |ui, scroll_viewport| {
            let view_rect = ui.clip_rect();
            let view_size = scroll_viewport.size();
            self.workspace_view_size = view_size;
            if self.opening.is_none() && self.open_error.is_none() && !self.shortcuts.open && !self.gallery.open && !self.paper_settings.open && !self.save_as_open
                && !ui.ctx().text_edit_focused() {
                let steps=ui.input_mut(|input| {
                    let mut steps=0i32;
                    input.events.retain(|event| {
                        if let egui::Event::Key {key,pressed:true,modifiers,..}=event {
                            if self.shortcuts.bindings.matches(Command::ZoomIn,*key,*modifiers) || self.shortcuts.bindings.matches(Command::ZoomInAlternate,*key,*modifiers) {steps+=1;return false;}
                            if self.shortcuts.bindings.matches(Command::ZoomOut,*key,*modifiers) {steps-=1;return false;}
                        }
                        true
                    });
                    steps
                });
                if steps!=0 {
                    self.finish_stroke();
                    let motion=self.viewport.zoom_at(view_size*0.5,scroll_viewport.min.to_vec2(),view_size,doc_size,1.2f32.powi(steps.clamp(-20,20)));
                    ui.scroll_with_delta_animation(motion,egui::style::ScrollAnimation::none());
                    ui.ctx().request_repaint();
                }
            }
            let mouse_pressure = self.settings.mouse_pressure;
            let mut input = PointerFrame::capture(
                ui,
                view_rect,
                &mut self.pressure_input,
                mouse_pressure,
                self.settings.pen_pressure_gamma,
            );
            // Consume native packets, but keep gallery/settings gestures off the drawing.
            input.space_down=!ui.ctx().text_edit_focused() && ui.input(|i|self.shortcuts.bindings.down(i,Command::Pan));
            let input = if self.opening.is_some() || self.open_error.is_some() || self.shortcuts.open || self.gallery.open || self.paper_settings.open || self.save_as_open { PointerFrame::default() } else { input };

            if !input.touch_navigation {
                self.touch_rotation = None;
            }
            if let Some((center, factor, translation)) = input.zoom_gesture {
                if center.is_finite()
                    && translation.is_finite()
                    && factor.is_finite()
                    && factor > 0.0
                    && input.twist_radians.is_finite()
                {
                    let rotation = if input.touch_navigation {
                        self.touch_rotation
                            .get_or_insert(crate::core::viewport::RotationGesture(
                                self.viewport.rotation,
                            ))
                            .advance(input.twist_radians, ui.input(|i| i.modifiers.shift))
                            - self.viewport.rotation
                    } else {
                        0.
                    };
                    let scroll_motion = self.viewport.gesture_rotate_at(
                        center - view_rect.min,
                        translation,
                        scroll_viewport.min.to_vec2(),
                        view_size,
                        doc_size,
                        factor,
                        rotation,
                    );
                    if scroll_motion.length_sq() > 1.0e-6 {
                        ui.scroll_with_delta_animation(
                            scroll_motion,
                            egui::style::ScrollAnimation::none(),
                        );
                    }
                }
            }

            if !input.wants_pan() && input.zoom_gesture.is_none() && !input.touch_navigation {
                if let Some(cursor) = input.position {
                    if input.scroll_y.abs() > 0.01 {
                        let factor = (-input.scroll_y * 0.0023).exp();
                        let cursor_in_view = cursor - view_rect.min;
                        let scroll_motion = self.viewport.zoom_at(
                            cursor_in_view,
                            scroll_viewport.min.to_vec2(),
                            view_size,
                            doc_size,
                            factor,
                        );
                        if scroll_motion.length_sq() > 1.0e-6 {
                            ui.scroll_with_delta(scroll_motion);
                        }
                    }
                }
            }

            let mut layout = self.viewport.layout(view_size, doc_size);
            if self.rotate_view && !input.touch_navigation && !input.wants_pan() {
                if input.primary_pressed && input.over_canvas {
                    self.view_rotation_drag = true;
                    self.mouse_rotation =
                        crate::core::viewport::RotationGesture(self.viewport.rotation);
                }
                if self.view_rotation_drag && input.primary_down {
                    if let Some(pos) = input.position {
                        let center = view_rect.min + layout.sheet_min + layout.sheet_size * 0.5
                            - scroll_viewport.min.to_vec2();
                        let old = pos - input.delta - center;
                        let new = pos - center;
                        if old.length() > 12. && new.length() > 12. {
                            let delta = (new.angle() - old.angle() + std::f32::consts::PI)
                                .rem_euclid(std::f32::consts::TAU)
                                - std::f32::consts::PI;
                            let motion = self.viewport.gesture_rotate_at(
                                center - view_rect.min,
                                Vec2::ZERO,
                                scroll_viewport.min.to_vec2(),
                                view_size,
                                doc_size,
                                1.,
                                self.mouse_rotation
                                    .advance(delta, ui.input(|i| i.modifiers.shift))
                                    - self.viewport.rotation,
                            );
                            ui.scroll_with_delta_animation(
                                motion,
                                egui::style::ScrollAnimation::none(),
                            );
                            layout = self.viewport.layout(view_size, doc_size);
                        }
                    }
                }
                if input.primary_released || !ui.input(|i| i.focused) {
                    self.view_rotation_drag = false;
                }
            }
            if input.wants_pan() {
                // Right drag, middle drag and Space+left drag all move the sheet. Native scrolling
                // owns overflowing axes; free_pan lets a fitted/small sheet still be repositioned.
                let scroll_motion = Vec2::new(
                    if layout.overflow[0] && !self.viewport.unbounded {
                        input.delta.x
                    } else {
                        0.0
                    },
                    if layout.overflow[1] && !self.viewport.unbounded {
                        input.delta.y
                    } else {
                        0.0
                    },
                );
                if scroll_motion.length_sq() > 1.0e-6 {
                    ui.scroll_with_delta(scroll_motion);
                }
                self.viewport.pan_fitting_axes(input.delta, layout);
                layout = self.viewport.layout(view_size, doc_size);
            }

            let (content_rect, _) = ui.allocate_exact_size(layout.content_size, Sense::hover());
            let painter = ui.painter_at(content_rect);
            painter.rect_filled(content_rect, 0.0, Color32::from_gray(43));

            let mut canvas_rect = Rect::from_center_size(
                content_rect.min + layout.sheet_min + layout.sheet_size * 0.5,
                doc_size * self.viewport.zoom,
            );
            self.workspace_canvas=Some(canvas_rect);
            #[cfg(test)]
            ui.data_mut(|d| d.insert_temp(egui::Id::new("test_paper_rect"), canvas_rect));
            let paper_corners = [
                Vec2::ZERO,
                Vec2::new(doc_size.x, 0.),
                doc_size,
                Vec2::new(0., doc_size.y),
            ]
            .map(|p| self.viewport.document_to_screen(canvas_rect, p));
            let native_owned = self.pressure_input.native_owns_pointer();
            let pen_frames = self.pressure_input.take_pen_frames();
            let pen_had_frames=!pen_frames.is_empty();
            self.show_quick_controls(ui, Rect::from_points(&paper_corners), view_rect);
            let action_rect = self.show_transform_actions(ui, canvas_rect, view_rect);
            let actions_block_mouse = if !native_owned { self.transform_actions_block_input(action_rect, input) } else { false };
            let quick_block_mouse = if !native_owned {self.quick_controls.block_input(input)} else {false};
            if input.touch_navigation && !native_owned {
                self.pause_liquify();
                self.shape_drag = None;
                self.editing.lasso = None;
                // A first finger can arrive a frame before the second. Restore its provisional
                // stroke, tip wear and smudger load without altering the user's undo history.
                if let Some(session) = self.stroke_session.take() {
                    session.transaction.rollback(&mut self.document);
                    self.tip_state = session.initial_tip;
                    self.stroke_engine = session.initial_engine;
                }
            } else if self.rotate_view {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            } else if native_owned {
                for frame in pen_frames {
                    let quick_block=self.quick_controls.block_input(frame);
                    if !self.transform_actions_block_input(action_rect, frame) && !quick_block {
                        self.handle_drawing_input(ui, self.workspace_canvas.unwrap_or(canvas_rect), frame);
                    }else{self.pause_liquify();}
                }
            } else if !actions_block_mouse && !quick_block_mouse {
                self.handle_drawing_input(ui, canvas_rect, input);
            }else{self.pause_liquify();}
            if native_owned&&!pen_had_frames&&self.liquify.session.is_some(){self.liquify_input(ui,canvas_rect,PointerFrame::default());}
            canvas_rect=self.workspace_canvas.unwrap_or(canvas_rect);
            let size=Vec2::new(self.document.spec.width_px as f32,self.document.spec.height_px as f32);
            let artwork_corners=[Vec2::ZERO,Vec2::new(size.x,0.),size,Vec2::new(0.,size.y)]
                .map(|p|self.viewport.document_to_screen(canvas_rect,p));
            let [x,y,w,h]=self.document.page_bounds();
            let paper_corners=[Vec2::new(x as f32,y as f32),Vec2::new((x+w) as f32,y as f32),
                Vec2::new((x+w) as f32,(y+h) as f32),Vec2::new(x as f32,(y+h) as f32)]
                .map(|p|self.viewport.document_to_screen(canvas_rect,p));
            // Shade once, after all queued pen packets, before this frame samples the canvas.
            self.ensure_texture(ui.ctx());
            if let Some(texture_id) = self.document_texture_id() {
                let mut mesh = egui::Mesh::with_texture(texture_id);
                for (pos, uv) in artwork_corners.into_iter().zip([
                    Pos2::ZERO,
                    Pos2::new(1., 0.),
                    Pos2::new(1., 1.),
                    Pos2::new(0., 1.),
                ]) {
                    mesh.vertices.push(egui::epaint::Vertex {
                        pos,
                        uv,
                        color: Color32::WHITE,
                    });
                }
                mesh.indices.extend([0, 1, 2, 0, 2, 3]);
                painter.add(egui::Shape::mesh(mesh));
            }
            painter.add(egui::Shape::closed_line(
                paper_corners.to_vec(),
                Stroke::new(1., Color32::from_gray(105)),
            ));


            self.paint_editing(&painter, canvas_rect);
            if let Some((start, end)) = self.shape_drag {
                let points = self
                    .settings
                    .shape
                    .points_with_snap(Vec2::new(start.x, start.y), Vec2::new(end.x, end.y), self.settings.snap_shape(ui.input(|i| i.modifiers.shift)));
                let points = points
                    .into_iter()
                    .map(|p| self.viewport.document_to_screen(canvas_rect, p))
                    .collect();
                painter.add(egui::Shape::line(
                    points,
                    Stroke::new(
                        (self.settings.shape_width_px * self.viewport.zoom).clamp(1., 16.),
                        Color32::from_rgba_unmultiplied(
                            self.settings.pencil_color_rgb[0],
                            self.settings.pencil_color_rgb[1],
                            self.settings.pencil_color_rgb[2],
                            (self.settings.shape_opacity * 255.).round() as u8,
                        ),
                    ),
                ));
            }
            if !self.rotate_view && !self.quick_controls.covers(input.position) && !input.position.is_some_and(|p| action_rect.is_some_and(|r| r.contains(p))) {
                self.paint_cursor(
                    painter,
                    view_rect,
                    canvas_rect,
                    input.position,
                    input.pressure,
                );
            }
        });
    }

    fn handle_drawing_input(&mut self, ui: &mut egui::Ui, canvas_rect: Rect, input: PointerFrame) {
        if self.opening.is_some() || self.open_error.is_some() || self.shortcuts.open || self.gallery.open || self.paper_settings.open || self.save_as_open { return; }
        if self.liquify.session.is_some(){self.liquify_input(ui,canvas_rect,input);return;}
        let mut canvas_rect=canvas_rect;
        self.workspace_canvas=Some(canvas_rect);
        if self.editing.transform.is_none() && !matches!(self.settings.tool,ToolKind::Lasso|ToolKind::VectorSelect)
            && !input.wants_pan() && !input.touch_navigation && (input.primary_pressed || ((self.stroke_session.is_some() || self.shape_drag.is_some()) && (input.primary_down || input.primary_released))) {
            if let Some(pos)=input.position.filter(|p|ui.clip_rect().contains(*p)) {
                let p=self.viewport.screen_to_document(canvas_rect,pos);
                self.grow_workspace(Rect::from_center_size(p.to_pos2(),Vec2::ZERO).expand(self.brush_extent(input)));
                canvas_rect=self.workspace_canvas.unwrap_or(canvas_rect);
            }
        }
        if self.editing.transform.is_some()
            || matches!(self.settings.tool, ToolKind::Lasso | ToolKind::VectorSelect)
        {
            self.edit_input(ui, canvas_rect, input);
            return;
        }
        if self.settings.tool == ToolKind::Shapes {
            self.handle_shape_input(ui, canvas_rect, input);
            return;
        }
        if input.wants_pan() {
            if self.stroke_session.is_some() {
                self.finish_stroke();
            }
            return;
        }

        let pointer_inside_workspace = input
            .position
            .is_some_and(|p| ui.clip_rect().contains(p));

        if input.primary_pressed && pointer_inside_workspace {
            if self.stroke_session.is_some() {
                self.finish_stroke();
            }
            self.prepare_vector_base();
            let initial_tip = self.tip_state.clone();
            let initial_engine = self.stroke_engine.clone();
            let point = self.make_stroke_point(
                input.position.unwrap(),
                canvas_rect,
                input.pressure,
                input.pressure_from_device,
                None,
                input.tilt_deg,
                input.azimuth_deg,
                input.rotation_deg,
            );
            if self.settings.tool == ToolKind::Pencil {
                self.stroke_engine.begin_pencil_stroke(StrokePoint { x:point.x-self.document.page_origin().x,y:point.y-self.document.page_origin().y,..point });
            }
            let mut tx = EditTransaction::default();
            // Preserve measured contact force, including deliberate firm taps.
            let start_point = point;
            if let Some(dirty) = self.stroke_engine.apply_dab(
                &mut self.document,
                &self.settings,
                &mut self.tip_state,
                start_point,
                &mut tx,
            ) {
                self.document.mark_dirty(dirty);
            }
            self.stroke_session = Some(StrokeSession {
                settings: self.settings.clone(),
                selection: self.document.selection.clone(),
                points: vec![start_point],
                initial_tip,
                initial_engine,
                last_raw: start_point,
                smoother: LineSmoother::new(
                    start_point,
                    if self.settings.tool == ToolKind::Pencil {
                        self.settings.line_smoothing
                    } else {
                        0.
                    },
                    self.viewport.zoom,
                ),
                curve_cursor: start_point,
                transaction: tx,
                device_pressure_used: input.pressure_from_device,
                device_orientation_used: input.orientation_from_device,
            });
            ui.ctx().request_repaint();
        } else if input.primary_down && pointer_inside_workspace {
            if let Some(screen_pos) = input.position {
                let previous = self
                    .stroke_session
                    .as_ref()
                    .map(|session| session.smoother.raw());
                let point = self.make_stroke_point(
                    screen_pos,
                    canvas_rect,
                    input.pressure,
                    input.pressure_from_device,
                    previous,
                    input.tilt_deg,
                    input.azimuth_deg,
                    input.rotation_deg,
                );
                if let Some(session) = &mut self.stroke_session {
                    let point = session.smoother.push(point);
                    // Midpoint quadratic interpolation turns the raw pointer polyline into a C1
                    // continuous path. Each raw sample is the control point between the midpoint
                    // before it and the midpoint after it, removing the square/polygonal corners
                    // that were visible in circles without adding a heavy stabilizer.
                    let next_midpoint = session.last_raw.lerp(point, 0.5);
                    self.stroke_engine.apply_quadratic_segment(
                        &mut self.document,
                        &self.settings,
                        &mut self.tip_state,
                        session.curve_cursor,
                        session.last_raw,
                        next_midpoint,
                        &mut session.transaction,
                    );
                    session.curve_cursor = next_midpoint;
                    session.last_raw = point;
                    session.points.push(point);
                    session.device_pressure_used |= input.pressure_from_device;
                    session.device_orientation_used |= input.orientation_from_device;
                    ui.ctx().request_repaint();
                }
            }
        }

        if input.primary_released {
            self.finish_stroke();
        }
    }

    fn finish_stroke(&mut self) {
        let Some(mut session) = self.stroke_session.take() else {
            return;
        };

        // Flush only to the measured endpoint. Coalesced Windows Ink samples already
        // contain the pressure ramp; invented extensions cause hooks and overshoots.
        self.stroke_engine.apply_quadratic_segment(
            &mut self.document,
            &self.settings,
            &mut self.tip_state,
            session.curve_cursor,
            session.last_raw,
            session.last_raw,
            &mut session.transaction,
        );

        // Pencil wear is intentionally deferred until stroke end. The previous implementation
        // changed the contact footprint continuously during a long mouse drag, so a nominally
        // constant-pressure line slowly became wider and softer while it was being drawn.
        if self.settings.tool == ToolKind::Pencil {
            self.tip_state.commit_pending_wear();
        } else {
            self.tip_state.clear_pending_wear();
        }

        if !session.transaction.is_empty() {
            let record = crate::core::vector::VectorStroke {
                shape: None,
                label: session.settings.tool.label().into(),
                settings: session.settings,
                tip: session.initial_tip,
                engine: session.initial_engine,
                points: session.points,
                polyline: false,
                selection: session.selection,
            };
            let active = self.document.active_layer_index();
            std::sync::Arc::make_mut(&mut self.document.layers[active].vectors)
                .strokes
                .push(std::sync::Arc::new(record));
            self.tabs[self.active_tab].modified = true;
            self.history.push(session.transaction, &self.document);
            self.status = match self.settings.tool {
                ToolKind::Pencil => format!(
                    "Pencil stroke · {} · {} · pressure {:.0}% ({}) · orientation {} · tip wear {:.1}% · rotation {:.0}°",
                    self.document.active_layer_name(),
                    self.settings.grade.label(),
                    session.last_raw.pressure * 100.0,
                    if session.device_pressure_used { "device" } else { "mouse fallback" },
                    if session.device_orientation_used { "device orientation" } else if self.settings.auto_azimuth { "manual tilt + auto azimuth" } else { "manual tilt/azimuth" },
                    self.tip_state.wear_fraction() * 100.0,
                    self.tip_state.profile.orientation_deg
                ),
                ToolKind::Smudge => format!(
                    "Smudge stroke · {} · pressure {:.0}% ({}) · stump load {:.0}%",
                    self.document.active_layer_name(),
                    session.last_raw.pressure * 100.0,
                    if session.device_pressure_used { "device" } else { "mouse fallback" },
                    self.stroke_engine.smudger_load_fraction(self.settings.smudge_size_px) * 100.0
                ),
                ToolKind::Eraser => format!(
                    "Eraser stroke · {} · pressure {:.0}% ({})",
                    self.document.active_layer_name(),
                    session.last_raw.pressure * 100.0,
                    if session.device_pressure_used { "device" } else { "mouse fallback" }
                ),
                ToolKind::Brush | ToolKind::Tissue | ToolKind::Shapes | ToolKind::Lasso | ToolKind::VectorSelect => format!("{} stroke · {}",self.settings.tool.label(),self.document.active_layer_name()),
            };
        }
    }

    fn make_stroke_point(
        &self,
        screen: Pos2,
        canvas_rect: Rect,
        pressure: f32,
        pressure_from_device: bool,
        previous: Option<StrokePoint>,
        device_tilt_deg: Option<f32>,
        device_azimuth_deg: Option<f32>,
        device_rotation_deg: Option<f32>,
    ) -> StrokePoint {
        let p = self.viewport.screen_to_document(canvas_rect, screen);
        let tilt_deg = device_tilt_deg.unwrap_or(self.settings.tilt_deg);
        let resolved_pressure = if pressure_from_device || self.settings.tool != ToolKind::Pencil {
            pressure
        } else if let Some(prev) = previous {
            let dx = p.x - prev.x;
            let dy = p.y - prev.y;
            let frame_motion = (dx * dx + dy * dy).sqrt();
            // Mouse/EasyCanvas fallback only: a modest dwell/velocity response prevents a totally
            // mechanical constant-width line when the platform supplies no pressure packets. This
            // is intentionally much weaker than genuine stylus pressure.
            let speed = (frame_motion / 10.0).clamp(0.0, 1.0);
            (pressure * (1.06 - 0.20 * speed)).clamp(0.01, 1.0)
        } else {
            pressure
        };
        let azimuth_deg = if let Some(device_azimuth_deg) = device_azimuth_deg {
            (device_azimuth_deg - self.viewport.rotation.to_degrees()).rem_euclid(360.0)
        } else if self.settings.tool == ToolKind::Pencil && self.settings.auto_azimuth {
            previous
                .and_then(|prev| {
                    let dx = p.x - prev.x;
                    let dy = p.y - prev.y;
                    let len2 = dx * dx + dy * dy;
                    if len2 > 1.0e-6 {
                        Some(dy.atan2(dx).to_degrees().rem_euclid(360.0))
                    } else {
                        None
                    }
                })
                .unwrap_or(self.settings.azimuth_deg)
        } else {
            self.settings.azimuth_deg
        };
        StrokePoint {
            x: p.x,
            y: p.y,
            pressure: resolved_pressure,
            tilt_deg,
            azimuth_deg,
            rotation_deg: device_rotation_deg,
        }
    }

    fn paint_cursor(
        &self,
        painter: egui::Painter,
        view_rect: Rect,
        _canvas_rect: Rect,
        pointer: Option<Pos2>,
        pressure: f32,
    ) {
        if self.editing.transform.is_some()
            || matches!(self.settings.tool, ToolKind::Lasso | ToolKind::VectorSelect)
        {
            return;
        }
        let Some(pointer) = pointer else {
            return;
        };
        if !view_rect.contains(pointer) {
            return;
        }
        if self.liquify_controls_cover(pointer){return;}
        if self.liquify.session.is_some() {
            painter.circle_stroke(pointer,self.liquify.settings.size*self.viewport.zoom*0.5,Stroke::new(1.5,Color32::from_rgb(41,162,183)));
            return;
        }

        let cursor_color = if self.settings.tool == ToolKind::Pencil {
            Color32::from_rgb(
                self.settings.pencil_color_rgb[0],
                self.settings.pencil_color_rgb[1],
                self.settings.pencil_color_rgb[2],
            )
        } else {
            Color32::from_gray(80)
        };

        if self.settings.tool == ToolKind::Pencil {
            let core_diameter_px = self
                .tip_state
                .effective_core_diameter_px(self.document.spec.dpi);
            let geometry = PencilTipContact::from_state_with_sharpness(
                core_diameter_px * self.settings.pencil_geometry_scale
                    * self.settings.pressure_width_scale(pressure),
                pressure,
                self.pressure_input
                    .active_tilt_deg()
                    .unwrap_or(self.settings.tilt_deg),
                self.pressure_input
                    .active_azimuth_deg()
                    .unwrap_or(self.settings.azimuth_deg + self.viewport.rotation.to_degrees()),
                self.settings.grade.formulation().core_hardness,
                self.settings.tip_sharpness,
            );
            let radius_screen = (geometry.cross_radius_px * self.viewport.zoom).max(2.0);
            painter.circle_stroke(pointer, radius_screen, Stroke::new(1.0, cursor_color));
            painter.circle_stroke(
                pointer,
                radius_screen + 1.0,
                Stroke::new(1.0, Color32::from_gray(230)),
            );

            if geometry.rear_extension_px > geometry.point_radius_px * 0.5 {
                let a = geometry.angle_rad;
                let direction = Vec2::new(a.cos(), a.sin());
                let rear = pointer - direction * geometry.rear_extension_px * self.viewport.zoom;
                painter.line_segment([rear, pointer], Stroke::new(1.0, cursor_color));
            }
            return;
        }

        let diameter_px = match self.settings.tool {
            ToolKind::Smudge => self.settings.smudge_size_px,
            ToolKind::Eraser => self.settings.eraser_diameter_mm * self.document.spec.dpi / 25.4,
            ToolKind::Pencil => unreachable!(),
            ToolKind::Brush => self.settings.brush_size_px,
            ToolKind::Tissue => self.settings.tissue_size_px,
            ToolKind::Shapes => self.settings.shape_width_px,
            ToolKind::Lasso | ToolKind::VectorSelect => 1.,
        };
        let pressure_scale = match self.settings.tool {
            ToolKind::Smudge => smudge_pressure_radius_scale(pressure),
            ToolKind::Eraser => self.settings.eraser_kind.pressure_scale(pressure),
            ToolKind::Pencil => unreachable!(),
            _ => 0.65 + 0.35 * pressure.clamp(0., 1.).sqrt(),
        };
        let radius_screen = (diameter_px * pressure_scale * 0.5 * self.viewport.zoom).max(2.0);
        painter.circle_stroke(pointer, radius_screen, Stroke::new(1.0, cursor_color));
        painter.circle_stroke(
            pointer,
            radius_screen + 1.0,
            Stroke::new(1.0, Color32::from_gray(230)),
        );
    }
}

impl eframe::App for GraphiteApp {
    fn on_exit(&mut self,_gl:Option<&eframe::glow::Context>) {
        self.gallery.flush_preferences();
    }
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // eframe runs logic for hidden/minimized windows too when a repaint is requested.
        #[cfg(windows)]
        {
            if !self.shortcuts.open && !ctx.text_edit_focused() && ctx.input_mut(|input| {
                self.shortcuts.bindings.consume(input,Command::Recover)
            }) {
                self.window_recovery.request_recovery();
            }
            if let Some(window) = frame.winit_window() {
                if self.dialog_parent.as_ref().is_none_or(|parent| !std::sync::Arc::ptr_eq(parent, window)) {
                    self.dialog_parent = Some(window.clone());
                }
                let polling =
                    self.wintab
                        .update(window, self.settings.use_wintab, ctx.input(|i| i.focused));
                if polling {
                    // Keep tablet packet polling responsive even when canvas uploads are paced.
                    ctx.request_repaint_after(std::time::Duration::from_millis(8));
                }
                if self.window_recovery.tick(window) {
                    self.status = "Moved Graphite Studio back onto an available screen.".into();
                }
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(750));
        }
        #[cfg(not(windows))]
        let _ = (ctx, frame);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.interface(ui);
    }
}

impl GraphiteApp {
    fn canvas_contact_active(&self) -> bool {
        self.stroke_session.is_some() || self.shape_drag.is_some()
            || self.editing.lasso.is_some() || self.pressure_input.native_owns_pointer()
            || self.view_rotation_drag
    }

    fn interface(&mut self, ui: &mut egui::Ui) {
        self.poll_liquify(ui.ctx());
        ui.data_mut(|d|d.insert_temp(egui::Id::new("active_shortcuts"),self.shortcuts.bindings.clone()));
        ui.ctx().options_mut(|o| o.zoom_with_keyboard = false);
        self.tick_pencil_preferences(ui.ctx());
        self.apply_display_style(ui.ctx());
        self.poll_opening();
        self.show_opening(ui.ctx());
        if self.opening.is_some() || self.open_error.is_some() {
            ui.disable();
        } else {
            self.prepare_quick_controls(ui);
            self.handle_keyboard_shortcuts(ui);
        }
        if self.fullscreen_applied != self.fullscreen {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
            self.fullscreen_applied = self.fullscreen;
            ui.ctx().request_repaint();
        }
        self.show_shortcuts(ui.ctx()); self.show_guide(ui.ctx());
        self.show_pencil_gallery(ui.ctx());
        self.show_paper_settings(ui.ctx());
        self.show_save_as(ui.ctx());
        self.show_liquify(ui.ctx());
        // Sidebar diagnostics scan the physical sheet. Compute once after an edit,
        // never on every mouse move or while a stroke is still being applied.
        let statistics_ready = self.acceleration != crate::performance::AccelerationMode::IntelHd
            || self
                .last_statistics_update
                .is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(1));
        if self.stroke_session.is_none() && self.liquify.session.is_none()
            && statistics_ready
            && self.sidebar_statistics.0 != self.document.revision()
        {
            self.sidebar_statistics = (
                self.document.revision(),
                self.document.graphite_coverage(),
                self.document.mean_paper_disturbance(),
            );
            self.last_statistics_update = Some(std::time::Instant::now());
        }

        egui::CentralPanel::default().show(ui, |ui| {
            if self.fullscreen {
                self.draw_canvas(ui);
                return;
            }
            let custom_texture_name = self
                .custom_paper_texture
                .as_ref()
                .map(|texture| texture.name.clone());
            let action = show_topbar(
                ui,
                &mut self.canvas_preset,
                &mut self.new_document_dpi,
                &mut self.paper_texture_choice,
                custom_texture_name.as_deref(),
                self.layers_panel_visible,
                self.history.can_undo(),
                self.history.can_redo(),
                self.acceleration,
                self.startup_acceleration,
                &self.renderer_label,
                self.settings.use_wintab,
                self.document.spec.width_px > self.document.spec.height_px,
                &self.recent_files,
            );
            self.handle_topbar_action(action);
            if self.opening.is_some() { ui.ctx().request_repaint(); }
            if self.fullscreen { ui.ctx().request_repaint(); }
            if self.rotate_view {
                ui.horizontal(|ui| {
                    ui.label(format!("Rotate paper · {:.0}° · drag around its center · R / Esc returns", self.viewport.rotation.to_degrees()));
                    if ui.button("Reset rotation").clicked() { self.viewport.rotation = 0.; self.fit_requested = true; }
                });
            }
            if !self.preference_status.is_empty() { ui.small(&self.preference_status); }
            self.show_drawing_tabs(ui);
            ui.separator();
            let active_device_pressure = self.pressure_input.active_pressure();
            let device_pressure_seen = self.pressure_input.device_pressure_seen();
            let active_device_tilt = self.pressure_input.active_tilt_deg();
            let device_orientation_seen = self.pressure_input.device_orientation_seen();
            ui.horizontal_top(|ui| {
                let row_height = ui.available_height().max(120.0);
                ui.allocate_ui_with_layout(Vec2::new(38.,row_height),egui::Layout::top_down(egui::Align::Center),|ui|{
                    ui.spacing_mut().item_spacing.y=3.;
                    for tool in ToolKind::PALETTE {
                        if crate::ui::tool_icons::tool_button(ui,tool,self.liquify.session.is_none()&&self.settings.tool==tool).clicked(){
                            self.select_tool(tool);
                        }
                        if tool==ToolKind::VectorSelect && crate::ui::tool_icons::liquify_button(ui,self.liquify.session.is_some(),32.).clicked(){self.begin_liquify();}
                    }
                    ui.separator();if ui.button("?").on_hover_text("Tool guide").clicked(){self.editing.guide=true;}
                });
                if self.material_panel_visible {
                    let material_width = self.material_panel_width.min(
                        (ui.available_width() - if self.layers_panel_visible { 470. } else { 220. })
                            .clamp(282., 520.));
                    ui.allocate_ui_with_layout(
                        Vec2::new(material_width, row_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("material_scroll")
                                .max_height(row_height)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    self.path_controls(ui);
                                    if self.editing.transform.is_some() || matches!(self.settings.tool, ToolKind::Lasso | ToolKind::VectorSelect) {
                                        self.mirror_controls(ui);
                                        let aspect = ui.checkbox(&mut self.settings.transform_keep_aspect, "Keep aspect ratio")
                                            .on_hover_text("Preserve the selection's proportions while resizing. Shift also constrains resizing.");
                                        #[cfg(test)]
                                        ui.data_mut(|d| d.insert_temp(egui::Id::new("test_aspect_option"), aspect.rect));
                                        let _ = aspect;
                                    }
                                    if self.editing.transform.is_some(){
                                        ui.strong("FREE TRANSFORM");
                                        if let Some(t)=&mut self.editing.transform{
                                            ui.checkbox(&mut t.rotate_mode,"Rotate · Ctrl+R");let mut degrees=t.angle.to_degrees();
                                            if ui.add(egui::DragValue::new(&mut degrees).speed(0.5).suffix("°")).changed() && degrees.is_finite(){t.angle=degrees.rem_euclid(360.).to_radians();}
                                        }
                                        ui.small("Drag inside to move; corners resize. Drag the round handle to rotate. Shift: keep proportions / snap 90°.");ui.separator();
                                    }else{ui.horizontal(|ui|{if ui.button(graphite_studio::shortcuts::hint(ui.ctx(),"Transform",Command::Transform)).clicked(){self.begin_transform(ui.ctx());}if ui.button(graphite_studio::shortcuts::hint(ui.ctx(),"Rotate",Command::RotateSelection)).clicked(){self.begin_rotation(ui.ctx());}});}
                                    ui.horizontal(|ui| {
                                        let icon=crate::ui::tool_icons::liquify_button(ui,self.liquify.session.is_some(),32.);
                                        if icon.clicked()||ui.button("Liquify").clicked(){self.begin_liquify();}
                                    });
                                    if self.settings.tool==ToolKind::Pencil {
                                        if ui.add_sized([240.,44.],egui::Button::new("Pencil gallery…")).clicked() {self.open_pencil_gallery();}
                                        ui.small(self.settings.pencil_texture.as_ref().map(|t|t.name.as_str()).unwrap_or("Standard graphite"));
                                    }
                                    let smudger_load = self
                                        .stroke_engine
                                        .smudger_load_fraction(self.settings.smudge_size_px);
                                    #[cfg(windows)]
                                    if self.settings.use_wintab {
                                        ui.small(&self.wintab.status);
                                    }
                                    if let Some(state)=&self.wgpu_render_state {
                                        let info=state.adapter.get_info();
                                        let compute=if crate::core::stroke_gpu::available(){"Large pencils and partial erasing use GPU compute; smaller strokes, tissue and smudge use the CPU."}else{"CPU stroke rendering active."};
                                        ui.small(format!("GPU · {:?}",info.backend)).on_hover_text(format!("{}\nAccelerated canvas display and zoom. {compute}",info.name));
                                    }
                                    let controls_before=graphite_studio::pencil_settings::PencilControls::capture(&self.settings,self.tip_state.profile.orientation_deg);
                                    let tool_action = show_tool_panel(
                                        ui,
                                        &mut self.settings,
                                        &mut self.tip_state,
                                        &self.document,
                                        &self.status,
                                        active_device_pressure,
                                        device_pressure_seen,
                                        active_device_tilt,
                                        self.pressure_input.active_rotation_deg(),
                                        self.pressure_input.device_rotation_seen(),
                                        device_orientation_seen,
                                        smudger_load,
                                        (self.sidebar_statistics.1, self.sidebar_statistics.2),
                                    );
                                    if controls_before!=graphite_studio::pencil_settings::PencilControls::capture(&self.settings,self.tip_state.profile.orientation_deg) {
                                        self.remember_pencil_controls();
                                    }
                                    self.tip_state
                                        .sync_core_diameter(self.settings.pencil_core_diameter_mm);
                                    match tool_action {
                                        ToolPanelAction::None => {}
                                        ToolPanelAction::ImportBrush => self.import_brushes(),
                                        ToolPanelAction::SharpenPencil => {
                                            self.tip_state.sharpen(
                                                self.settings.pencil_core_diameter_mm,
                                                self.settings.grade.formulation(),
                                            );
                                            self.status =
                                                "Sharpened pencil: tip wear reset.".to_owned();
                                        }
                                        ToolPanelAction::CleanSmudger => {
                                            self.stroke_engine.clear_smudger();
                                            self.status = "Cleaned smudger reservoir.".to_owned();
                                        }
                                    }
                                });
                        },
                    );
                }
                let (handle_rect, _) =
                    ui.allocate_exact_size(Vec2::new(14.0, row_height), Sense::hover());
                let material_edge_enabled = !self.canvas_contact_active();
                let handle = ui.interact(handle_rect, ui.make_persistent_id("material_edge_handle"),
                    if material_edge_enabled { Sense::click_and_drag() } else { Sense::hover() });
                #[cfg(test)]
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("test_material_handle"), handle_rect)
                });
                ui.painter()
                    .rect_filled(handle_rect, 3.0, ui.visuals().widgets.inactive.bg_fill);
                ui.painter().text(
                    handle_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    if self.material_panel_visible {
                        "‹"
                    } else {
                        "›"
                    },
                    egui::FontId::proportional(22.0),
                    ui.visuals().text_color(),
                );
                if material_edge_enabled && handle.clicked() {
                    self.material_panel_visible = !self.material_panel_visible;
                }
                if material_edge_enabled && handle.dragged() {
                    self.material_panel_visible = true;
                    let max_width = (ui.max_rect().width() - 400.0).clamp(282.0, 520.0);
                    self.material_panel_width = (self.material_panel_width
                        + ui.input(|i| i.pointer.delta().x))
                    .clamp(282.0, max_width);
                }
                let material_edge_hovered = material_edge_enabled && handle.hovered()
                    && ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| handle_rect.intersect(ui.clip_rect()).contains(p));
                let material_edge_dragging = material_edge_enabled && handle.dragged();
                handle.on_hover_text("Click to show or hide Material. Drag to resize.");
                let spacing = ui.spacing().item_spacing.x;
                let max_layer_width = (ui.available_width() - 14. - 2. * spacing - 180.).clamp(230., 520.);
                let layer_width = if self.layers_panel_visible {
                    self.layers_panel_width.clamp(230., max_layer_width)
                } else {
                    0.0
                };
                // Keep the edge available even while Layers is hidden.
                let separator_allowance = 14. + spacing + if self.layers_panel_visible { spacing } else { 0. };
                let canvas_width =
                    (ui.available_width() - layer_width - separator_allowance).max(120.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(canvas_width, row_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        self.draw_canvas(ui);
                    },
                );

                let (edge_rect, _) = ui.allocate_exact_size(Vec2::new(14., row_height), Sense::hover());
                let layer_edge_enabled = !self.canvas_contact_active();
                let edge = ui.interact(edge_rect, ui.make_persistent_id("layers_edge_handle"),
                    if layer_edge_enabled { Sense::click_and_drag() } else { Sense::hover() });
                #[cfg(test)]
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("test_layers_handle"), edge_rect));
                edge.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Show or hide Layers"));
                ui.painter().rect_filled(edge_rect, 3., ui.visuals().widgets.inactive.bg_fill);
                ui.painter().text(edge_rect.center(), egui::Align2::CENTER_CENTER,
                    if self.layers_panel_visible { "›" } else { "‹" },
                    egui::FontId::proportional(22.), ui.visuals().text_color());
                if layer_edge_enabled && edge.clicked() {
                    self.layers_panel_visible = !self.layers_panel_visible;
                    ui.ctx().request_repaint();
                }
                if layer_edge_enabled && edge.dragged() {
                    self.layers_panel_visible = true;
                    self.layers_panel_width = (layer_width.max(230.) - ui.input(|i| i.pointer.delta().x))
                        .clamp(230., max_layer_width);
                    ui.ctx().request_repaint();
                }
                let layer_edge_hovered = layer_edge_enabled && edge.hovered()
                    && ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| edge_rect.intersect(ui.clip_rect()).contains(p));
                let layer_edge_dragging = layer_edge_enabled && edge.dragged();
                edge.on_hover_text("Click to show or hide Layers. Drag left to widen; drag right to narrow.");
                if layer_width > 0. {
                    let layer_action = ui
                        .allocate_ui_with_layout(
                            Vec2::new(layer_width, row_height),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| show_layer_panel(ui, &self.document),
                        )
                        .inner;
                    self.handle_layer_panel_action(layer_action);
                }
                // A resize cursor belongs only to the clipped panel edge or its
                // active drag, never egui's expanded hover area around controls.
                // Native canvas contact also overrides stale mouse hover.
                if !self.canvas_contact_active() && (material_edge_hovered || layer_edge_hovered
                    || (ui.input(|i| i.pointer.primary_down()) && (material_edge_dragging || layer_edge_dragging))) {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                } else if (self.canvas_contact_active() && !self.rotate_view)
                    || ui.ctx().output(|o| o.cursor_icon == egui::CursorIcon::ResizeHorizontal) {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_pointer_marks_are_uploaded_in_the_same_frame() {
        let mut app = GraphiteApp::with_render_state(None);
        app.replace_document(CanvasSpec::from_physical("Latency", 50., 50., 120.));
        let ctx = egui::Context::default();
        let frame = |app: &mut GraphiteApp, events| ctx.run_ui(egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440., 920.))),
            events, ..Default::default()
        }, |ui| app.interface(ui));
        for _ in 0..3 { frame(&mut app, vec![]); }
        let start = ctx.data(|d| d.get_temp::<Rect>(egui::Id::new("test_paper_rect")).unwrap().center());
        frame(&mut app, vec![egui::Event::PointerMoved(start), egui::Event::PointerButton {
            pos: start, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default()
        }]);
        app.last_texture_update = None;
        let output = frame(&mut app, vec![egui::Event::PointerMoved(start + Vec2::new(35., 10.))]);
        assert!(app.document.surface.graphite_mass.iter().any(|&m| m > 0.));
        assert!(app.document.take_dirty().is_none(), "new material was left for the next frame");
        assert!(!output.textures_delta.set.is_empty(), "new mark was not uploaded");
        // Even a throttled display must publish the final tail immediately on lift.
        app.last_texture_update = Some(std::time::Instant::now());
        frame(&mut app, vec![egui::Event::PointerButton {
            pos: start + Vec2::new(50., 15.), button: egui::PointerButton::Primary,
            pressed: false, modifiers: Default::default()
        }]);
        assert!(app.stroke_session.is_none());
        assert!(app.document.take_dirty().is_none());
    }

    #[test]
    fn pencil_smoothing_reaches_the_stroke_path_and_remains_undoable() {
        let mut app = GraphiteApp::with_render_state(None);
        app.replace_document(CanvasSpec::from_physical("Smoothing", 40., 40., 120.));
        app.settings.line_smoothing = 0.8;
        app.viewport.zoom = 1.;
        let ctx = egui::Context::default();
        let canvas = Rect::from_min_size(Pos2::ZERO, Vec2::splat(188.));
        let draw = |app: &mut GraphiteApp, frame: PointerFrame| {
            let _ = ctx.run_ui(Default::default(), |ui| {
                app.handle_drawing_input(ui, canvas, frame)
            });
        };
        let base = PointerFrame {
            primary_down: true,
            pressure: 0.4,
            pressure_from_device: true,
            tilt_deg: Some(8.),
            azimuth_deg: Some(20.),
            rotation_deg: Some(40.),
            ..Default::default()
        };
        draw(
            &mut app,
            PointerFrame {
                position: Some(Pos2::new(30., 60.)),
                primary_pressed: true,
                ..base
            },
        );
        for i in 1..=20 {
            draw(
                &mut app,
                PointerFrame {
                    position: Some(Pos2::new(
                        30. + i as f32 * 3.,
                        60. + if i % 2 == 0 { 4. } else { -4. },
                    )),
                    ..base
                },
            );
        }
        let session = app.stroke_session.as_ref().unwrap();
        assert!((session.last_raw.y - 60.).abs() < 1.);
        assert_eq!(session.smoother.raw().y, 64.);
        assert_eq!(session.last_raw.pressure, 0.4);
        assert_eq!(session.last_raw.rotation_deg, Some(40.));
        draw(
            &mut app,
            PointerFrame {
                primary_released: true,
                ..Default::default()
            },
        );
        assert!(app.stroke_session.is_none());
        assert!(app.document.surface.graphite_mass.iter().any(|&m| m > 0.));
        assert!(app.history.undo(&mut app.document));
        assert!(app.document.surface.graphite_mass.iter().all(|&m| m == 0.));
    }

    #[test]
    fn keyboard_zoom_changes_only_the_drawing_and_preserves_text_input() {
        let mut app=GraphiteApp::with_render_state(None);
        app.replace_document(CanvasSpec::from_physical("Keyboard zoom",40.,40.,120.));
        let ctx=egui::Context::default();
        let frame=|app:&mut GraphiteApp,events:Vec<egui::Event>|ctx.run_ui(egui::RawInput {
            screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(1440.,920.))),events,..Default::default()
        },|ui|app.interface(ui));
        for _ in 0..3 {frame(&mut app,vec![]);}
        let zoom=app.viewport.zoom;let ppp=ctx.pixels_per_point();
        let control=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new("pencil_width_response")).unwrap());
        let key=|key,modifiers|egui::Event::Key {key,physical_key:None,pressed:true,repeat:false,modifiers};
        // These shortcuts also work while the pointer is over a panel.
        frame(&mut app,vec![egui::Event::PointerMoved(Pos2::new(30.,160.)),key(egui::Key::Plus,egui::Modifiers::CTRL|egui::Modifiers::SHIFT)]);
        frame(&mut app,vec![]);
        assert!((app.viewport.zoom/zoom-1.2).abs()<0.0001);
        assert_eq!(ctx.zoom_factor(),1.);assert_eq!(ctx.pixels_per_point(),ppp);
        assert_eq!(ctx.data(|d|d.get_temp::<Rect>(egui::Id::new("pencil_width_response")).unwrap()),control);
        frame(&mut app,vec![key(egui::Key::Minus,egui::Modifiers::CTRL)]);
        frame(&mut app,vec![]);
        assert!((app.viewport.zoom-zoom).abs()<0.0001);
        frame(&mut app,vec![key(egui::Key::Equals,egui::Modifiers::CTRL)]);
        assert!((app.viewport.zoom/zoom-1.2).abs()<0.0001);
        let zoom=app.viewport.zoom;
        app.gallery.open=true;
        for _ in 0..3 {frame(&mut app,vec![]);}
        frame(&mut app,vec![key(egui::Key::Plus,egui::Modifiers::CTRL)]);
        frame(&mut app,vec![]);
        assert_eq!(app.viewport.zoom,zoom);assert_eq!(ctx.zoom_factor(),1.);
        assert!(app.document.surface.graphite_mass.iter().all(|&m|m==0.));
    }

    #[test]
    fn two_finger_drag_moves_fitted_and_scrolled_paper_without_drawing() {
        fn frame(app: &mut GraphiteApp, ctx: &egui::Context, events: Vec<egui::Event>) {
            let _ = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440., 920.))),
                    events,
                    ..Default::default()
                },
                |ui| app.interface(ui),
            );
        }
        let touch = |id, phase, pos| egui::Event::Touch {
            device_id: egui::TouchDeviceId(99),
            id: egui::TouchId(id),
            phase,
            pos,
            force: None,
        };
        for zoom in [0.5, 3.] {
            let mut app = GraphiteApp::with_render_state(None);
            app.replace_document(CanvasSpec::from_physical("Gesture", 80., 80., 120.));
            app.fit_requested = false;
            app.viewport.zoom = zoom;
            let ctx = egui::Context::default();
            for _ in 0..3 {
                frame(&mut app, &ctx, vec![]);
            }
            let a = Pos2::new(600., 400.);
            let b = Pos2::new(700., 400.);
            frame(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(a),
                    touch(1, egui::TouchPhase::Start, a),
                    touch(2, egui::TouchPhase::Start, b),
                ],
            );
            frame(&mut app, &ctx, vec![]);
            let paper = || {
                ctx.data(|d| {
                    d.get_temp::<Rect>(egui::Id::new("test_paper_rect"))
                        .unwrap()
                })
            };
            let before = paper();
            let delta = Vec2::new(-20., -30.);
            frame(
                &mut app,
                &ctx,
                vec![
                    touch(1, egui::TouchPhase::Move, a + delta),
                    touch(2, egui::TouchPhase::Move, b + delta),
                ],
            );
            frame(&mut app, &ctx, vec![]);
            assert!(
                (paper().min - before.min - delta).length() < 0.1,
                "zoom {zoom}: {:?}",
                paper().min - before.min
            );
            assert_eq!(app.viewport.zoom, zoom);
            assert!(app.document.surface.graphite_mass.iter().all(|&m| m == 0.));
            assert!(!app.history.can_undo());
            assert!(app.stroke_session.is_none());
        }
    }

    #[test]
    fn drawing_tabs_preserve_layers_history_paper_and_view() {
        let mut app = GraphiteApp::with_render_state(None);
        app.document.add_layer();
        app.document.set_paper_color([220, 200, 180]);
        let mut tx = EditTransaction::default();
        tx.remember(0, &app.document);
        app.document.surface.graphite_mass[0] = 0.4;
        app.document.surface.loose_mass[0] = 0.4;
        app.history.push(tx, &app.document);
        app.viewport.zoom = 2.3;
        app.settings.pen_pressure_gamma = 0.8;
        app.replace_document(CanvasSpec::from_physical("Second", 20., 20., 120.));
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active_tab, 1);
        assert_eq!(app.document.surface.graphite_mass[0], 0.);
        assert_eq!(app.document.layer_count(), 1);
        assert!(!app.history.can_undo());
        app.document.set_paper_color([120, 140, 160]);
        app.viewport.zoom = 0.7;
        app.activate_tab(0);
        assert_eq!(app.document.layer_count(), 2);
        assert_eq!(app.document.paper_color_rgb, [220, 200, 180]);
        assert_eq!(app.viewport.zoom, 2.3);
        assert_eq!(app.settings.pen_pressure_gamma, 0.8);
        assert!(app.history.undo(&mut app.document));
        assert_eq!(app.document.surface.graphite_mass[0], 0.);
        app.activate_tab(1);
        assert_eq!(app.document.paper_color_rgb, [120, 140, 160]);
        assert_eq!(app.viewport.zoom, 0.7);
        assert!(!app.history.can_redo());
        let active_id = app.tabs[1].id;
        let first_id = app.tabs[0].id;
        app.reorder_tab(active_id, first_id, true);
        assert_eq!(app.active_tab, 0);
        assert_eq!(app.tabs[0].id, active_id);
        assert_eq!(app.document.paper_color_rgb, [120, 140, 160]);
        app.reorder_tab(active_id, first_id, false);
        assert_eq!(app.active_tab, 1);
        app.close_tab(1);
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.active_tab, 0);
        assert_eq!(app.document.layer_count(), 2);
        assert!(app.history.can_redo());
        app.close_tab(0);
        assert_eq!(app.tabs.len(), 1);
        assert_ne!(app.tabs[0].id, first_id);
        assert_eq!(app.document.layer_count(), 1);
        assert!(!app.history.can_undo());
        assert!(!app.history.can_redo());
    }

    #[test]
    fn interface_preview_at_normal_and_collapsed_panel_widths() {
        let mut app = GraphiteApp::with_render_state(None);
        app.replace_document(CanvasSpec::from_physical("Study", 80., 90., 120.));
        app.document.set_paper_color([245, 232, 208]);
        let ctx = egui::Context::default();
        ctx.set_visuals(egui::Visuals::light());
        let mut preview = crate::ui::test_render::Preview::default();
        for (width, visible, tool, file) in [
            (282., true, ToolKind::Pencil, "../../ui-v18.png"),
            (410., true, ToolKind::Pencil, "../../ui-v18-wide.png"),
            (410., false, ToolKind::Pencil, "../../ui-v18-collapsed.png"),
            (282., true, ToolKind::Eraser, "../../ui-v18-eraser.png"),
            (282., true, ToolKind::Brush, "../../ui-v18-brush.png"),
            (282., true, ToolKind::Tissue, "../../ui-v18-tissue.png"),
            (282., true, ToolKind::Shapes, "../../ui-v18-shapes.png"),
            (282., true, ToolKind::Lasso, "../../ui-v18-lasso.png"),
        ] {
            app.material_panel_width = width;
            app.material_panel_visible = visible;
            app.settings.tool = tool;
            app.settings.eraser_strength = 1.;
            for pass in 0..3 {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440., 920.))),
                        ..Default::default()
                    },
                    |ui| app.interface(ui),
                );
                preview.update(&output);
                if pass == 2 {
                    preview.save(&ctx, &output, file, [1440, 920]);
                }
            }
        }
    }

    #[test]
    fn resize_cursor_stays_on_panel_edges_and_off_tool_icons() {
        let mut app = GraphiteApp::with_render_state(None);
        app.replace_document(CanvasSpec::from_physical("Hover test", 40., 40., 120.));
        let ctx = egui::Context::default();
        let frame = |app: &mut GraphiteApp, events| ctx.run_ui(egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440., 920.))),
            events, ..Default::default()
        }, |ui| app.interface(ui));
        for _ in 0..3 { frame(&mut app, vec![]); }
        for edge in ["test_material_handle", "test_layers_handle"] {
            for tool in ToolKind::PALETTE {
                let edge_rect = ctx.data(|d| d.get_temp::<Rect>(egui::Id::new(edge)).unwrap());
                let output = frame(&mut app, vec![egui::Event::PointerMoved(edge_rect.center())]);
                assert_eq!(output.platform_output.cursor_icon, egui::CursorIcon::ResizeHorizontal);
                let icon = ctx.data(|d| d.get_temp::<Rect>(egui::Id::new(("test_tool_icon", tool.label()))).unwrap());
                for pos in [icon.center(), icon.left_center() - Vec2::new(1., 0.),
                    icon.right_center() + Vec2::new(1., 0.)] {
                    let output = frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
                    assert_ne!(output.platform_output.cursor_icon, egui::CursorIcon::ResizeHorizontal,
                        "resize cursor around {} at {pos:?}", tool.label());
                }
                // egui can consider nearby pixels hovered; the actual bounds win.
                let pos = edge_rect.center_top() - Vec2::new(0., 1.);
                let output = frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
                assert_ne!(output.platform_output.cursor_icon, egui::CursorIcon::ResizeHorizontal);
            }
        }
    }

    #[test]
    fn drawing_contact_overrides_stale_panel_hover_cursor() {
        for key in ["test_material_handle", "test_layers_handle"] {
            let mut app = GraphiteApp::with_render_state(None);
            app.replace_document(CanvasSpec::from_physical("Cursor test", 50., 50., 120.));
            let ctx = egui::Context::default();
            let frame = |app: &mut GraphiteApp, events| ctx.run_ui(egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440., 920.))),
                events, ..Default::default()
            }, |ui| app.interface(ui));
            for _ in 0..3 { frame(&mut app, vec![]); }
            let pos = ctx.data(|d| d.get_temp::<Rect>(egui::Id::new(key)).unwrap().center());
            let idle = frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
            assert_eq!(idle.platform_output.cursor_icon, egui::CursorIcon::ResizeHorizontal);
            // A pen starts drawing while the mouse hover remains parked over a panel edge.
            app.viewport.zoom = 1.;
            let _ = ctx.run_ui(Default::default(), |ui| app.handle_drawing_input(ui,
                Rect::from_min_size(Pos2::ZERO, Vec2::splat(236.)), PointerFrame {
                    position: Some(Pos2::new(60., 60.)), primary_pressed: true, primary_down: true,
                    pressure: 0.5, pressure_from_device: true, ..Default::default()
                }));
            assert!(app.stroke_session.is_some());
            let widths = (app.material_panel_width, app.layers_panel_width);
            let drawing = frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
            assert_eq!(drawing.platform_output.cursor_icon, egui::CursorIcon::Default);
            assert_eq!(widths, (app.material_panel_width, app.layers_panel_width));
            assert!(app.material_panel_visible && app.layers_panel_visible);
            app.finish_stroke();
            let idle = frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
            assert_eq!(idle.platform_output.cursor_icon, egui::CursorIcon::ResizeHorizontal);
        }
    }

    #[test]
    fn layers_edge_hides_restores_and_resizes_without_changing_artwork() {
        let mut app = GraphiteApp::with_render_state(None);
        app.replace_document(CanvasSpec::from_physical("Panel test", 40., 60., 120.));
        app.document.add_layer();
        let before = app.renderer.rgba8(&app.document);
        let ctx = egui::Context::default();
        fn frame(app: &mut GraphiteApp, ctx: &egui::Context, events: Vec<egui::Event>) {
            let _ = ctx.run_ui(egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440., 920.))),
                events, ..Default::default()
            }, |ui| app.interface(ui));
        }
        let handle = |ctx: &egui::Context| ctx.data(|d| d.get_temp::<Rect>(egui::Id::new("test_layers_handle")).unwrap().center());
        let button = |pos, pressed| egui::Event::PointerButton {
            pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default()
        };
        for _ in 0..3 { frame(&mut app, &ctx, vec![]); }
        let initial = app.layers_panel_width;
        let pos = handle(&ctx);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos), button(pos, true)]);
        let moved = pos - Vec2::new(100., 0.);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(moved)]);
        frame(&mut app, &ctx, vec![button(moved, false)]);
        assert!(app.layers_panel_visible && app.layers_panel_width > initial + 80.);
        let resized = app.layers_panel_width;
        for visible in [false, true] {
            frame(&mut app, &ctx, vec![]);
            let pos = handle(&ctx);
            assert!(pos.x < 1440. && pos.x > 0.);
            frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos), button(pos, true)]);
            frame(&mut app, &ctx, vec![button(pos, false)]);
            assert_eq!(app.layers_panel_visible, visible);
            assert_eq!(app.layers_panel_width, resized);
        }
        frame(&mut app, &ctx, vec![]);
        let pos = handle(&ctx);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos), button(pos, true)]);
        let moved = pos + Vec2::new(70., 0.);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(moved)]);
        frame(&mut app, &ctx, vec![button(moved, false)]);
        assert!(app.layers_panel_width < resized - 50.);
        // Shrinking a window after widening both panels must not strand Layers offscreen.
        app.material_panel_width = 520.;
        app.layers_panel_width = 520.;
        for _ in 0..3 {
            let _ = ctx.run_ui(egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 600.))),
                ..Default::default()
            }, |ui| app.interface(ui));
        }
        assert!(handle(&ctx).x < 900.);
        let row = ctx.data(|d| d.get_temp::<Rect>(egui::Id::new(("test_layer_rect", app.document.layers[0].id))).unwrap());
        assert!(row.right() <= 900., "Layers must fit the smaller window: {row:?}");
        assert_eq!(app.document.layer_count(), 2);
        assert_eq!(app.renderer.rgba8(&app.document), before);
        assert!(!app.history.can_undo());
    }

    #[test]
    fn material_edge_handle_toggles_and_resizes_with_real_pointer_events() {
        let mut app = GraphiteApp::with_render_state(None);
        let ctx = egui::Context::default();
        fn frame(app: &mut GraphiteApp, ctx: &egui::Context, events: Vec<egui::Event>) {
            let _ = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440., 920.))),
                    events,
                    ..Default::default()
                },
                |ui| app.interface(ui),
            );
        }
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        for _ in 0..3 {
            frame(&mut app, &ctx, vec![]);
        }
        let handle = |ctx: &egui::Context| {
            ctx.data(|d| {
                d.get_temp::<Rect>(egui::Id::new("test_material_handle"))
                    .unwrap()
                    .center()
            })
        };
        let pos = handle(&ctx);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(pos), button(pos, true)],
        );
        frame(&mut app, &ctx, vec![button(pos, false)]);
        assert!(!app.material_panel_visible);
        frame(&mut app, &ctx, vec![]);
        let pos = handle(&ctx);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(pos), button(pos, true)],
        );
        frame(&mut app, &ctx, vec![button(pos, false)]);
        assert!(app.material_panel_visible);
        frame(&mut app, &ctx, vec![]);
        let initial = app.material_panel_width;
        let pos = handle(&ctx);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(pos), button(pos, true)],
        );
        let moved = pos + Vec2::new(80., 0.);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(moved)]);
        frame(&mut app, &ctx, vec![button(moved, false)]);
        assert!(app.material_panel_visible && app.material_panel_width > initial + 50.);
    }
}
