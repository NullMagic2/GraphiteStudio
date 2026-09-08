use eframe::egui::{ColorImage, TextureId};
use egui_wgpu::{wgpu, RenderState};

/// GPU-owned multiresolution display reconstruction for the document preview.
///
/// Level 0 contains the exact 8-bit rendered document image. Lower levels are generated on the
/// GPU with an explicit linear-light box reconstruction: encoded sRGB is decoded to linear RGB,
/// averaged, and encoded back to sRGB. This avoids bright/dark aliasing from high-frequency
/// graphite microstructure when the sheet is displayed below native resolution.
///
/// The pyramid is display-only. It never feeds back into paper/deposit simulation or exports.
pub struct GpuDisplayPyramid {
    render_state: RenderState,
    texture: wgpu::Texture,
    mip_views: Vec<wgpu::TextureView>,
    mip_bind_groups: Vec<wgpu::BindGroup>,
    pipeline: wgpu::RenderPipeline,
    texture_id: TextureId,
    width: u32,
    height: u32,
    mip_count: u32,
}

impl std::fmt::Debug for GpuDisplayPyramid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuDisplayPyramid")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("mip_count", &self.mip_count)
            .field("texture_id", &self.texture_id)
            .finish_non_exhaustive()
    }
}

impl GpuDisplayPyramid {
    pub fn new(render_state: RenderState, image: &ColorImage) -> Result<Self, String> {
        let [width, height] = image.size;
        if width == 0 || height == 0 {
            return Err("display pyramid cannot be created for an empty image".to_owned());
        }
        let width = u32::try_from(width).map_err(|_| "document width exceeds GPU limits")?;
        let height = u32::try_from(height).map_err(|_| "document height exceeds GPU limits")?;
        let mip_count = mip_count(width, height);
        let device = &render_state.device;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("graphite_display_pyramid"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: mip_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        let mip_views = (0..mip_count)
            .map(|level| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("graphite_display_pyramid_mip"),
                    format: Some(wgpu::TextureFormat::Rgba8Unorm),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    usage: None,
                    aspect: wgpu::TextureAspect::All,
                    base_mip_level: level,
                    mip_level_count: Some(1),
                    base_array_layer: 0,
                    array_layer_count: Some(1),
                })
            })
            .collect::<Vec<_>>();

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("graphite_linear_light_downsample_shader"),
            source: wgpu::ShaderSource::Wgsl(LINEAR_LIGHT_DOWNSAMPLE_WGSL.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("graphite_display_pyramid_bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("graphite_display_pyramid_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("graphite_display_pyramid_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let mip_bind_groups = mip_views
            .iter()
            .take(mip_count.saturating_sub(1) as usize)
            .map(|source_view| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("graphite_display_pyramid_bind_group"),
                    layout: &bind_group_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(source_view),
                    }],
                })
            })
            .collect::<Vec<_>>();

        let full_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("graphite_display_pyramid_full_view"),
            format: Some(wgpu::TextureFormat::Rgba8Unorm),
            dimension: Some(wgpu::TextureViewDimension::D2),
            usage: Some(wgpu::TextureUsages::TEXTURE_BINDING),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: Some(mip_count),
            base_array_layer: 0,
            array_layer_count: Some(1),
        });

        let sampler_descriptor = wgpu::SamplerDescriptor {
            label: Some("graphite_display_pyramid_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            // Reconstruct enlarged graphite continuously instead of exposing square nearest-
            // neighbor texels. Minification also blends between adjacent pyramid levels.
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            lod_min_clamp: 0.0,
            lod_max_clamp: (mip_count.saturating_sub(1)) as f32,
            ..Default::default()
        };

        let texture_id = render_state
            .renderer
            .write()
            .register_native_texture_with_sampler_options(device, &full_view, sampler_descriptor);

        let mut pyramid = Self {
            render_state,
            texture,
            mip_views,
            mip_bind_groups,
            pipeline,
            texture_id,
            width,
            height,
            mip_count,
        };
        pyramid.upload_full(image)?;
        Ok(pyramid)
    }

    pub fn texture_id(&self) -> TextureId {
        self.texture_id
    }

    pub fn size(&self) -> [usize; 2] {
        [self.width as usize, self.height as usize]
    }

    pub fn mip_count(&self) -> u32 {
        self.mip_count
    }

    pub fn upload_full(&mut self, image: &ColorImage) -> Result<(), String> {
        if image.size != self.size() {
            return Err(format!(
                "display pyramid size mismatch: GPU is {}×{}, image is {}×{}",
                self.width, self.height, image.size[0], image.size[1]
            ));
        }
        let bytes = color_image_bytes(image);
        self.write_base_region(0, 0, self.width, self.height, &bytes);
        self.regenerate_lower_levels(None);
        Ok(())
    }

    pub fn upload_patch(&mut self, origin: [usize; 2], image: &ColorImage) -> Result<(), String> {
        let [patch_width, patch_height] = image.size;
        let x = u32::try_from(origin[0]).map_err(|_| "patch x exceeds GPU limits")?;
        let y = u32::try_from(origin[1]).map_err(|_| "patch y exceeds GPU limits")?;
        let width = u32::try_from(patch_width).map_err(|_| "patch width exceeds GPU limits")?;
        let height = u32::try_from(patch_height).map_err(|_| "patch height exceeds GPU limits")?;
        if x.saturating_add(width) > self.width || y.saturating_add(height) > self.height {
            return Err("display patch lies outside the GPU document texture".to_owned());
        }
        if width == 0 || height == 0 {
            return Ok(());
        }

        let bytes = color_image_bytes(image);
        self.write_base_region(x, y, width, height, &bytes);

        // Propagate only the dirty footprint through the mip chain. v0.7/v0.8 rebuilt every mip
        // over the whole sheet after each dab; on a 300-DPI A4 document that was unnecessary GPU
        // work during drawing. A conservative two-pixel expansion keeps odd-dimension box mapping
        // correct without sacrificing the linear-light reconstruction.
        self.regenerate_lower_levels(Some((x, y, width, height)));
        Ok(())
    }

    fn write_base_region(&self, x: u32, y: u32, width: u32, height: u32, bytes: &[u8]) {
        self.render_state.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    fn regenerate_lower_levels(&self, dirty_base: Option<(u32, u32, u32, u32)>) {
        if self.mip_count <= 1 {
            // Queue::write_texture begins on the next submit; an empty submit starts it promptly.
            self.render_state
                .queue
                .submit(std::iter::empty::<wgpu::CommandBuffer>());
            return;
        }

        let device = &self.render_state.device;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("graphite_display_pyramid_encoder"),
        });

        let mut dirty = dirty_base;
        for level in 1..self.mip_count as usize {
            let target_width = (self.width >> level).max(1);
            let target_height = (self.height >> level).max(1);
            let scissor = dirty.map(|(x, y, width, height)| {
                let x0 = (x / 2).saturating_sub(2);
                let y0 = (y / 2).saturating_sub(2);
                let x1 = ((x.saturating_add(width).saturating_add(1)) / 2)
                    .saturating_add(2)
                    .min(target_width);
                let y1 = ((y.saturating_add(height).saturating_add(1)) / 2)
                    .saturating_add(2)
                    .min(target_height);
                let sx = x0.min(target_width.saturating_sub(1));
                let sy = y0.min(target_height.saturating_sub(1));
                let sw = x1.saturating_sub(sx).max(1).min(target_width - sx);
                let sh = y1.saturating_sub(sy).max(1).min(target_height - sy);
                (sx, sy, sw, sh)
            });

            let color_attachment = Some(wgpu::RenderPassColorAttachment {
                view: &self.mip_views[level],
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    // A partial update must preserve untouched mip pixels. Full rebuilds still
                    // clear the destination because every fragment will be regenerated.
                    load: if dirty.is_some() {
                        wgpu::LoadOp::Load
                    } else {
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                    },
                    store: wgpu::StoreOp::Store,
                },
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("graphite_display_pyramid_pass"),
                color_attachments: &[color_attachment],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.mip_bind_groups[level - 1], &[]);
            if let Some((sx, sy, sw, sh)) = scissor {
                pass.set_scissor_rect(sx, sy, sw, sh);
                dirty = Some((sx, sy, sw, sh));
            }
            pass.draw(0..3, 0..1);
        }

        self.render_state
            .queue
            .submit(std::iter::once(encoder.finish()));
    }
}

impl Drop for GpuDisplayPyramid {
    fn drop(&mut self) {
        self.render_state
            .renderer
            .write()
            .free_texture(&self.texture_id);
        self.texture.destroy();
    }
}

fn mip_count(width: u32, height: u32) -> u32 {
    1 + width.max(height).ilog2()
}

fn color_image_bytes(image: &ColorImage) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(image.pixels.len() * 4);
    for pixel in &image.pixels {
        bytes.extend_from_slice(&pixel.to_array());
    }
    bytes
}

const LINEAR_LIGHT_DOWNSAMPLE_WGSL: &str = r#"
@group(0) @binding(0)
var src_tex: texture_2d<f32>;

fn srgb_to_linear_channel(v: f32) -> f32 {
    if (v <= 0.04045) {
        return v / 12.92;
    }
    return pow((v + 0.055) / 1.055, 2.4);
}

fn linear_to_srgb_channel(v_in: f32) -> f32 {
    let v = max(v_in, 0.0);
    if (v <= 0.0031308) {
        return 12.92 * v;
    }
    return 1.055 * pow(v, 1.0 / 2.4) - 0.055;
}

fn decode_srgb(rgb: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        srgb_to_linear_channel(rgb.r),
        srgb_to_linear_channel(rgb.g),
        srgb_to_linear_channel(rgb.b),
    );
}

fn encode_srgb(rgb: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        linear_to_srgb_channel(rgb.r),
        linear_to_srgb_channel(rgb.g),
        linear_to_srgb_channel(rgb.b),
    );
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    // Full-screen triangle; no vertex buffer needed.
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    return vec4<f32>(positions[vertex_index], 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let src_dim_u = textureDimensions(src_tex);
    let src_dim = vec2<i32>(src_dim_u);
    let dst_dim = max(src_dim / vec2<i32>(2, 2), vec2<i32>(1, 1));
    let dst = vec2<i32>(position.xy);

    // Map the destination pixel's box back to its complete source interval. The interval is two
    // texels on ordinary even dimensions and may become three at the far edge of odd dimensions,
    // so no source row/column silently disappears from the reconstruction.
    let start = vec2<i32>(
        (dst.x * src_dim.x) / dst_dim.x,
        (dst.y * src_dim.y) / dst_dim.y,
    );
    let end = vec2<i32>(
        ((dst.x + 1) * src_dim.x + dst_dim.x - 1) / dst_dim.x,
        ((dst.y + 1) * src_dim.y + dst_dim.y - 1) / dst_dim.y,
    );

    var linear_sum = vec3<f32>(0.0);
    var alpha_sum = 0.0;
    var sample_count = 0.0;

    for (var oy: i32 = 0; oy < 3; oy = oy + 1) {
        for (var ox: i32 = 0; ox < 3; ox = ox + 1) {
            let coord = start + vec2<i32>(ox, oy);
            if (coord.x < end.x && coord.y < end.y && coord.x < src_dim.x && coord.y < src_dim.y) {
                let sample = textureLoad(src_tex, coord, 0);
                linear_sum = linear_sum + decode_srgb(sample.rgb);
                alpha_sum = alpha_sum + sample.a;
                sample_count = sample_count + 1.0;
            }
        }
    }

    let inv_count = 1.0 / max(sample_count, 1.0);
    let linear_rgb = linear_sum * inv_count;
    return vec4<f32>(encode_srgb(linear_rgb), alpha_sum * inv_count);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mip_count_reaches_one_pixel_axis() {
        assert_eq!(mip_count(1, 1), 1);
        assert_eq!(mip_count(2, 1), 2);
        assert_eq!(mip_count(2480, 3508), 12);
    }

    #[test]
    fn color_image_bytes_are_rgba() {
        let image = ColorImage::filled(
            [2, 1],
            eframe::egui::Color32::from_rgba_premultiplied(1, 2, 3, 4),
        );
        assert_eq!(color_image_bytes(&image), vec![1, 2, 3, 4, 1, 2, 3, 4]);
    }
}
