//! Software rasterization of real egui output for headless UI regression previews.
use eframe::egui::{self, ColorImage, TextureId};
use std::collections::HashMap;

#[derive(Default)]
pub struct Preview {
    textures: HashMap<TextureId, ColorImage>,
}
impl Preview {
    pub fn update(&mut self, output: &egui::FullOutput) {
        for (id, delta) in &output.textures_delta.set {
            let egui::ImageData::Color(image) = &delta.image;
            if let Some([ox, oy]) = delta.pos {
                let target = self.textures.get_mut(id).unwrap();
                for y in 0..image.size[1] {
                    for x in 0..image.size[0] {
                        target[(ox + x, oy + y)] = image[(x, y)];
                    }
                }
            } else {
                self.textures.insert(*id, (**image).clone());
            }
        }
    }
    pub fn save(&self, ctx: &egui::Context, output: &egui::FullOutput, path: &str, size: [u32; 2]) {
        let mut image =
            image::RgbaImage::from_pixel(size[0], size[1], image::Rgba([245, 245, 245, 255]));
        for primitive in ctx.tessellate(output.shapes.clone(), output.pixels_per_point) {
            let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive else {
                continue;
            };
            let Some(texture) = self.textures.get(&mesh.texture_id) else {
                continue;
            };
            for triangle in mesh.indices.chunks_exact(3) {
                let v = [
                    mesh.vertices[triangle[0] as usize],
                    mesh.vertices[triangle[1] as usize],
                    mesh.vertices[triangle[2] as usize],
                ];
                let p = v.map(|v| v.pos * output.pixels_per_point);
                let cross = |a: egui::Vec2, b: egui::Vec2| a.x * b.y - a.y * b.x;
                let area = cross(p[1] - p[0], p[2] - p[0]);
                if area.abs() < 0.00001 {
                    continue;
                }
                let clip = primitive.clip_rect * output.pixels_per_point;
                let xmin = p
                    .iter()
                    .map(|p| p.x)
                    .fold(f32::INFINITY, f32::min)
                    .max(clip.left())
                    .floor()
                    .max(0.) as u32;
                let xmax = p
                    .iter()
                    .map(|p| p.x)
                    .fold(f32::NEG_INFINITY, f32::max)
                    .min(clip.right())
                    .ceil()
                    .min(size[0] as f32) as u32;
                let ymin = p
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::INFINITY, f32::min)
                    .max(clip.top())
                    .floor()
                    .max(0.) as u32;
                let ymax = p
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::NEG_INFINITY, f32::max)
                    .min(clip.bottom())
                    .ceil()
                    .min(size[1] as f32) as u32;
                for y in ymin..ymax {
                    for x in xmin..xmax {
                        let q = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);
                        let weights = [
                            cross(p[1] - q, p[2] - q) / area,
                            cross(p[2] - q, p[0] - q) / area,
                            cross(p[0] - q, p[1] - q) / area,
                        ];
                        if weights.iter().any(|w| *w < 0.) {
                            continue;
                        }
                        let uv = v[0].uv.to_vec2() * weights[0]
                            + v[1].uv.to_vec2() * weights[1]
                            + v[2].uv.to_vec2() * weights[2];
                        let tx = (uv.x * texture.size[0] as f32)
                            .floor()
                            .clamp(0., texture.size[0] as f32 - 1.)
                            as usize;
                        let ty = (uv.y * texture.size[1] as f32)
                            .floor()
                            .clamp(0., texture.size[1] as f32 - 1.)
                            as usize;
                        let sample = texture[(tx, ty)].to_array();
                        let mut source = [0.; 4];
                        for c in 0..4 {
                            source[c] = (0..3)
                                .map(|k| v[k].color.to_array()[c] as f32 * weights[k])
                                .sum::<f32>()
                                * sample[c] as f32
                                / 255.;
                        }
                        let destination = image.get_pixel_mut(x, y);
                        for c in 0..3 {
                            destination.0[c] =
                                (source[c] + destination.0[c] as f32 * (1. - source[3] / 255.))
                                    .clamp(0., 255.) as u8;
                        }
                    }
                }
            }
        }
        image.save(path).unwrap();
    }
}
