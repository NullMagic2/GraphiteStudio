use egui_wgpu::{wgpu, WgpuConfiguration, WgpuSetupCreateNew};
/// Prefer a hardware Vulkan adapter. Retain other available backends when the
/// graphics driver cannot present Vulkan; never prefer software over hardware.
pub fn configuration(dx12_only: bool) -> WgpuConfiguration {
    configuration_for(crate::performance::AccelerationMode::General, dx12_only)
}

pub fn configuration_for(
    mode: crate::performance::AccelerationMode,
    dx12_only: bool,
) -> WgpuConfiguration {
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    if dx12_only {
        setup.instance_descriptor.backends = wgpu::Backends::DX12;
    }
    setup.power_preference = wgpu::PowerPreference::HighPerformance;
    setup.native_adapter_selector = Some(std::sync::Arc::new(move |adapters, surface| {
        adapters
            .iter()
            .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
            .min_by_key(|a| {
                let info = a.get_info();
                (
                    info.device_type == wgpu::DeviceType::Cpu,
                    mode == crate::performance::AccelerationMode::Radeon7900Xtx
                        && !(info.vendor == 0x1002
                            && info.device_type == wgpu::DeviceType::DiscreteGpu),
                    info.backend != wgpu::Backend::Vulkan,
                    info.device_type != wgpu::DeviceType::DiscreteGpu,
                )
            })
            .cloned()
            .ok_or_else(|| {
                "No compatible Vulkan/DirectX adapter. Try --intel-hd for older OpenGL hardware, or --dx12 with a supported driver.".into()
            })
    }));
    WgpuConfiguration {
        surface: if mode == crate::performance::AccelerationMode::Radeon7900Xtx {
            egui_wgpu::SurfaceConfig::LOW_LATENCY
        } else {
            egui_wgpu::SurfaceConfig::HIGH_THROUGHPUT
        },
        wgpu_setup: setup.into(),
        ..Default::default()
    }
}
