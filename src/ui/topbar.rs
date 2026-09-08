use super::action_icons::{labeled_button, ActionIcon};
use eframe::egui;

use crate::core::paper::PaperTexturePreset;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasPreset {
    A5,
    A4,
    Letter,
    Custom,
}

impl CanvasPreset {
    pub fn label(self) -> &'static str {
        match self {
            Self::A5 => "A5",
            Self::A4 => "A4",
            Self::Letter => "Letter",
            Self::Custom => "Custom…",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaperTextureChoice {
    White,
    Recycled,
    Ivory,
    Custom,
}

impl PaperTextureChoice {
    pub fn from_builtin(preset: PaperTexturePreset) -> Self {
        match preset {
            PaperTexturePreset::White => Self::White,
            PaperTexturePreset::Recycled => Self::Recycled,
            PaperTexturePreset::Ivory => Self::Ivory,
        }
    }

    pub fn to_builtin(self) -> Option<PaperTexturePreset> {
        match self {
            Self::White => Some(PaperTexturePreset::White),
            Self::Recycled => Some(PaperTexturePreset::Recycled),
            Self::Ivory => Some(PaperTexturePreset::Ivory),
            Self::Custom => None,
        }
    }

    pub fn label(self, custom_name: Option<&str>) -> String {
        match self {
            Self::White => "White".to_owned(),
            Self::Recycled => "Recycled".to_owned(),
            Self::Ivory => "Ivory".to_owned(),
            Self::Custom => custom_name
                .map(|name| format!("Custom: {name}"))
                .unwrap_or_else(|| "Custom (load...)".to_owned()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TopbarAction {
    None,
    PaperSettings,
    PencilGallery,
    NewCanvas { preset: CanvasPreset, dpi: f32 },
    Undo,
    Redo,
    Fit,
    Fullscreen,
    OpenProject,
    SaveProject,
    SaveProjectAs,
    ApplyPaperTexture,
    LoadCustomPaperTexture,
    ToggleLayersPanel,
    Acceleration(crate::performance::AccelerationMode),
    TabletBackend(bool),
    CanvasOrientation(bool),
}

pub fn show_topbar(
    ui: &mut egui::Ui,
    selected_preset: &mut CanvasPreset,
    paper_dpi: &mut f32,
    selected_paper_texture: &mut PaperTextureChoice,
    custom_texture_name: Option<&str>,
    layers_panel_visible: bool,
    can_undo: bool,
    can_redo: bool,
    acceleration: crate::performance::AccelerationMode,
    startup_acceleration: crate::performance::AccelerationMode,
    renderer_label: &str,
    use_wintab: bool,
    current_landscape: bool,
) -> TopbarAction {
    let mut action = TopbarAction::None;
    let _ = (
        selected_preset,
        paper_dpi,
        selected_paper_texture,
        custom_texture_name,
        current_landscape,
    );

    ui.horizontal_wrapped(|ui| {
        ui.menu_button("File", |ui| {
            for (label, command) in [
                ("Open project…    Ctrl+O", TopbarAction::OpenProject),
                ("Save project    Ctrl+S", TopbarAction::SaveProject),
                (
                    "Save project as…    Ctrl+Shift+S",
                    TopbarAction::SaveProjectAs,
                ),
            ] {
                if ui.button(label).clicked() {
                    action = command;
                    ui.close();
                }
            }
        });
        ui.menu_button("Options", |ui| {
            ui.set_max_width(320.);
            #[cfg(windows)]
            {
                ui.strong("Input backend");
                for (label, wintab) in [("Windows Ink", false), ("Wintab", true)] {
                    if ui.selectable_label(use_wintab == wintab, label).clicked() {
                        action = TopbarAction::TabletBackend(wintab);
                    }
                }
                ui.small("Switches immediately. Windows Ink for EasyCanvas; Wintab for compatible Wacom / XP-Pen drivers.");
                ui.separator();
            }
            #[cfg(not(windows))]
            let _ = use_wintab;
            ui.strong("Acceleration mode");
            for mode in crate::performance::AccelerationMode::ALL {
                if ui
                    .selectable_label(acceleration == mode, mode.label())
                    .on_hover_text(mode.description())
                    .clicked()
                {
                    action = TopbarAction::Acceleration(mode);
                }
            }
            ui.separator();
            ui.label(acceleration.description());
            ui.small("Drawing resolution, pen input and export quality stay unchanged.");
            ui.separator();
            ui.small(format!("Running: {renderer_label}"));
            if acceleration != startup_acceleration {
                ui.label(
                    "Save your drawings and reopen the app to finish switching renderer / GPU.",
                );
            }
        });
        if labeled_button(ui,ActionIcon::PaperSettings,"Paper settings…").clicked() { action = TopbarAction::PaperSettings; }
        for (landscape, label, icon) in [(false,"Portrait",super::action_icons::ActionIcon::Portrait),
            (true,"Landscape",super::action_icons::ActionIcon::Landscape)] {
            if super::action_icons::button_sized(ui,icon,label,current_landscape==landscape,egui::Vec2::splat(44.)).clicked() {
                action=TopbarAction::CanvasOrientation(landscape);
            }
        }
        if labeled_button(ui,ActionIcon::PencilGallery,"Pencil gallery…").clicked() { action = TopbarAction::PencilGallery; }
        ui.separator();
        for (enabled,icon,label,command) in [
            (can_undo,ActionIcon::Undo,"Undo (Ctrl+Z)",TopbarAction::Undo),
            (can_redo,ActionIcon::Redo,"Redo (Ctrl+Y)",TopbarAction::Redo),
        ] {
            if ui.add_enabled_ui(enabled,|ui|super::action_icons::button_sized(ui,icon,label,false,egui::Vec2::splat(44.))).inner.clicked() {
                action=command;
            }
        }
        if labeled_button(ui,ActionIcon::Fit,"Fit to screen").on_hover_text("Center the whole drawing in the available canvas area. This changes only the view.").clicked() {
            action = TopbarAction::Fit;
        }
        if labeled_button(ui,ActionIcon::Fullscreen,"Fullscreen").on_hover_text("Show the whole drawing without panels. Esc exits fullscreen.").clicked() {
            action = TopbarAction::Fullscreen;
        }
        if super::action_icons::button(
            ui,
            super::action_icons::ActionIcon::Layers,
            "Show / hide Layers",
            layers_panel_visible,
        )
        .clicked()
        {
            action = TopbarAction::ToggleLayersPanel;
        }

        ui.separator();
    });

    action
}
