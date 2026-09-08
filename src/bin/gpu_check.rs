//! Headless validation of the production Vulkan display pipeline; opens no window.
use eframe::egui::{Color32, ColorImage};
use egui_wgpu::{wgpu, WgpuSetup};
use graphite_studio::render::{gpu, GpuDisplayPyramid};
fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    struct Wake(std::thread::Thread);
    impl std::task::Wake for Wake {
        fn wake(self: std::sync::Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = std::task::Waker::from(std::sync::Arc::new(Wake(std::thread::current())));
    let mut cx = std::task::Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(v) => return v,
            std::task::Poll::Pending => std::thread::park(),
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = graphite_studio::performance::AccelerationMode::from_args(std::env::args())
        .unwrap_or_default();
    let mut config = gpu::configuration_for(mode, false);
    let WgpuSetup::CreateNew(setup) = &mut config.wgpu_setup else {
        unreachable!()
    };
    setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
    let WgpuSetup::CreateNew(copy) = config.wgpu_setup.clone() else {
        unreachable!()
    };
    let instance = wgpu::Instance::new(copy.instance_descriptor);
    let state = block_on(egui_wgpu::RenderState::create(
        &config,
        &instance,
        None,
        Default::default(),
    ))?;
    let info = state.adapter.get_info();
    let scope = state.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut display = GpuDisplayPyramid::new(
        state.clone(),
        &ColorImage::filled([256, 192], Color32::from_rgb(100, 60, 40)),
    )?;
    display.upload_patch(
        [20, 30],
        &ColorImage::filled([64, 48], Color32::from_rgb(30, 40, 90)),
    )?;
    state.device.poll(wgpu::PollType::wait_indefinitely())?;
    if let Some(error) = block_on(scope.pop()) {
        return Err(error.to_string().into());
    }
    let report=format!("Profile: {}\nBackend: {:?}\nAdapter: {}\nDevice: {:?}\nMaximum queued frames: {:?}\nProduction display pyramid: {} levels\nFull upload, partial upload and GPU downsampling: passed\nNo window or desktop input used.\n",mode.label(),info.backend,info.name,info.device_type,config.surface.desired_maximum_frame_latency,display.mip_count());
    if let Some(path) = std::env::args().nth(1) {
        std::fs::write(path, &report)?;
    }
    println!("{report}");
    Ok(())
}
