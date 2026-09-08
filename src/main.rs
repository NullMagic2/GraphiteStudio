#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod input;
#[cfg(windows)]
mod native_pen;
mod ui;
#[cfg(windows)]
mod window_recovery;
#[cfg(windows)]
mod wintab;

use app::GraphiteApp;
use eframe::egui;
use graphite_studio::performance;
use graphite_studio::{core, export, render};

fn main() {
    if let Err(error) = run() {
        rfd::MessageDialog::new()
            .set_title("Graphite Studio could not start")
            .set_description(error.to_string())
            .set_level(rfd::MessageLevel::Error)
            .show();
    }
}

fn run() -> eframe::Result {
    #[cfg(windows)]
    {
        // Version-independent identity keeps taskbar grouping consistent across updates.
        #[link(name = "shell32")]
        extern "system" {
            fn SetCurrentProcessExplicitAppUserModelID(id: *const u16) -> i32;
        }
        let id: Vec<u16> = "GraphiteStudio.Desktop"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        unsafe {
            SetCurrentProcessExplicitAppUserModelID(id.as_ptr());
        }
    }
    let acceleration = graphite_studio::performance::Preferences::startup();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_icon(egui::IconData {
                rgba: include_bytes!(concat!(env!("OUT_DIR"), "/graphite-icon.rgba")).to_vec(),
                width: 256,
                height: 256,
            })
            .with_inner_size([1440.0, 920.0])
            .with_min_inner_size([900.0, 600.0]),
        renderer: acceleration.renderer(),
        multisampling: 0,
        dithering: acceleration != graphite_studio::performance::AccelerationMode::IntelHd,
        depth_buffer: 0,
        stencil_buffer: 0,
        wgpu_options: render::gpu::configuration_for(
            acceleration,
            std::env::args().any(|arg| arg == "--dx12"),
        ),
        #[cfg(windows)]
        event_loop_builder: Some(Box::new(|builder| {
            use winit::platform::windows::EventLoopBuilderExtWindows;
            builder.with_msg_hook(native_pen::message_hook);
        })),
        ..Default::default()
    };

    eframe::run_native(
        "Graphite Studio v0.23.9",
        options,
        Box::new(move |cc| Ok(Box::new(GraphiteApp::new(cc, acceleration)))),
    )
}
