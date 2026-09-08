use eframe::egui::{Color32, ColorImage};

use crate::{
    core::{
        document::{BlendMode, DirtyRect, Document},
        material::PixelDepositState,
    },
    render::renderer::DocumentRenderer,
};

#[derive(Debug, Default)]
pub struct RasterRenderer {
    image: Option<ColorImage>,
}

impl RasterRenderer {
    fn paint_region(document: &Document, rect: DirtyRect, pixels: &mut [Color32]) {
        let width = rect.width();
        if pixels.is_empty() || width == 0 {
            return;
        }
        let active = document.active_layer_index();
        let visible: Vec<_> = document.layers.iter().enumerate()
            .filter(|(index, layer)| layer.visible && layer.opacity > 0
                && (*index == active || layer.deposit.allocated_tile_count() > 0))
            .collect();
        // Large refreshes use a bounded number of workers. Small drawing patches stay
        // on this thread to avoid scheduling overhead. Every pixel uses the same shader.
        static WORKERS: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
        let workers = *WORKERS.get_or_init(|| {
            std::thread::available_parallelism()
                .map(|n| n.get().min(4))
                .unwrap_or(1)
        });
        let rows_per_chunk = if pixels.len() >= 262_144 {
            rect.height().div_ceil(workers)
        } else {
            rect.height()
        };
        let shade_rows = |start_y: usize, rows: &mut [Color32]| {
            let mut spans = Vec::with_capacity(visible.len());
            for (row_index, row) in rows.chunks_mut(width).enumerate() {
                let y = start_y + row_index;
                if visible.len() == 1 && visible[0].0 == active {
                    let layer = visible[0].1;
                    let opacity = layer.opacity as f32 / 255.;
                    for (offset, color) in row.iter_mut().enumerate() {
                        let i = document.index(rect.min_x + offset, y);
                        let rgb = composite_deposit(paper_pixel_rgb(document, i),
                            document.surface.deposit_pixel(i), layer.blend_mode, opacity);
                        *color = Color32::from_rgb(to_u8(rgb[0]), to_u8(rgb[1]), to_u8(rgb[2]));
                    }
                    continue;
                }
                let mut local_x = 0;
                while local_x < width {
                    let x = rect.min_x + local_x;
                    let len = (crate::core::material::LAYER_TILE_SIDE - x % crate::core::material::LAYER_TILE_SIDE).min(width - local_x);
                    spans.clear();
                    for &(index, layer) in &visible {
                        let span = if index == active { None } else {
                            let Some(span) = layer.deposit.row_span(x, y, len) else { continue; };
                            Some(span)
                        };
                        spans.push((layer.blend_mode, layer.opacity as f32 / 255., span));
                    }
                    for offset in 0..len {
                        let i = document.index(x + offset, y);
                        let mut rgb = paper_pixel_rgb(document, i);
                        for &(mode, opacity, span) in &spans {
                            let deposit = span.map_or_else(|| document.surface.deposit_pixel(i), |s| s[offset]);
                            rgb = composite_deposit(rgb, deposit, mode, opacity);
                        }
                        row[local_x + offset] = Color32::from_rgb(to_u8(rgb[0]), to_u8(rgb[1]), to_u8(rgb[2]));
                    }
                    local_x += len;
                }
            }
        };
        if rows_per_chunk == rect.height() {
            shade_rows(rect.min_y, pixels);
        } else {
            std::thread::scope(|scope| {
                for (chunk_index, rows) in pixels.chunks_mut(rows_per_chunk * width).enumerate() {
                    let shade_rows = &shade_rows;
                    scope
                        .spawn(move || shade_rows(rect.min_y + chunk_index * rows_per_chunk, rows));
                }
            });
        }
    }
}

impl DocumentRenderer for RasterRenderer {
    fn reset(&mut self) {
        self.image = None;
    }

    fn render_full(&mut self, document: &Document) -> ColorImage {
        let mut image = ColorImage::filled(
            [document.spec.width_px, document.spec.height_px],
            Color32::WHITE,
        );
        Self::paint_region(
            document,
            DirtyRect::full(document.spec.width_px, document.spec.height_px),
            &mut image.pixels,
        );
        self.image = Some(image.clone());
        image
    }

    fn render_region(&mut self, document: &Document, rect: DirtyRect) -> ColorImage {
        if self
            .image
            .as_ref()
            .is_none_or(|img| img.size != [document.spec.width_px, document.spec.height_px])
        {
            self.render_full(document);
        }

        let mut patch = ColorImage::filled([rect.width(), rect.height()], Color32::WHITE);
        Self::paint_region(document, rect, &mut patch.pixels);
        let image = self.image.as_mut().expect("renderer initialized");
        for (local_y, row) in patch.pixels.chunks(rect.width().max(1)).enumerate() {
            let start = (rect.min_y + local_y) * document.spec.width_px + rect.min_x;
            image.pixels[start..start + row.len()].copy_from_slice(row);
        }
        patch
    }

    fn rgba8(&mut self, document: &Document) -> Vec<u8> {
        // Exports must reflect blend/visibility edits even before the next preview
        // refresh. The preview still updates only dirty regions during drawing.
        self.render_full(document);

        let image = self.image.as_ref().expect("renderer image initialized");
        let mut bytes = Vec::with_capacity(image.pixels.len() * 4);
        for pixel in &image.pixels {
            bytes.extend_from_slice(&pixel.to_array());
        }
        bytes
    }

    fn rgb16(&mut self, document: &Document) -> Vec<u16> {
        let mut samples = Vec::with_capacity(document.spec.pixel_count() * 3);
        for y in 0..document.spec.height_px {
            for x in 0..document.spec.width_px {
                let rgb = shade_pixel_rgb(document, x, y);
                samples.push(to_u16(rgb[0]));
                samples.push(to_u16(rgb[1]));
                samples.push(to_u16(rgb[2]));
            }
        }
        samples
    }
}

#[cfg(test)]
fn shade_pixel_color32(document: &Document, x: usize, y: usize) -> Color32 {
    let rgb = shade_pixel_rgb(document, x, y);
    Color32::from_rgb(to_u8(rgb[0]), to_u8(rgb[1]), to_u8(rgb[2]))
}

fn shade_pixel_rgb(document: &Document, x: usize, y: usize) -> [f32; 3] {
    let i = document.index(x, y);
    let mut rgb = paper_pixel_rgb(document, i);
    for (layer_index, layer) in document.layers.iter().enumerate() {
        if layer.visible && layer.opacity > 0 {
            rgb = composite_deposit(
                rgb,
                document.layer_deposit_pixel(layer_index, i),
                layer.blend_mode,
                layer.opacity as f32 / 255.,
            );
        }
    }
    rgb
}

fn paper_pixel_rgb(document: &Document, i: usize) -> [f32; 3] {
    let rest_height = document.surface.rest_height[i];
    let height = document.surface.current_height[i];
    let fiber = document.surface.fiber[i];
    let abrasion = document.surface.abrasion[i];

    // Paper is a shared physical support underneath the Photoshop-like drawing layers.
    let relief = (height - 0.5) * 0.020 + (fiber - 0.5) * 0.008;
    let permanent_change = (height - rest_height).abs() * 0.030 + abrasion * 0.018;
    let brightness = (1.0 + relief - permanent_change).clamp(0.82, 1.04);
    let base_paper = document.paper_albedo[i];
    let textured = [
        (base_paper[0] * brightness * (document.paper_color_rgb[0] as f32 / 255.0)).clamp(0.0, 1.0),
        (base_paper[1] * brightness * (document.paper_color_rgb[1] as f32 / 255.0)).clamp(0.0, 1.0),
        (base_paper[2] * brightness * (document.paper_color_rgb[2] as f32 / 255.0)).clamp(0.0, 1.0),
    ];
    let opacity=document.paper_texture_opacity.clamp(0.,1.);
    if opacity>=1. {return textured;}
    std::array::from_fn(|c| {
        let color=document.paper_color_rgb[c] as f32/255.;
        color+(textured[c]-color)*opacity
    })
}

fn composite_deposit(
    base: [f32; 3],
    deposit: PixelDepositState,
    mode: BlendMode,
    opacity: f32,
) -> [f32; 3] {
    let (body, density) = deposit_optics(deposit);
    if density == 0.0 {
        return base;
    }
    blend_color(base, body, density * opacity, mode)
}

/// Composite selected material once, retaining a transparent material base for further editing.
/// Non-adjacent selections move to the highest selected position; intervening layers stay separate.
pub(crate) fn merge_layer_pixel(doc: &Document, indices: &[usize], i: usize, mode: BlendMode, independent: bool) -> PixelDepositState {
    let mut alpha=0.;
    let mut out=PixelDepositState::default();
    let mut base=if mode==BlendMode::Multiply && independent { [1.;3] } else { [0.;3] };
    if !independent {
        base=paper_pixel_rgb(doc,i);
        for l in 0..*indices.last().unwrap() {
            if !indices.contains(&l) && doc.layers[l].visible {
                base=composite_deposit(base,doc.layer_deposit_pixel(l,i),doc.layers[l].blend_mode,doc.layers[l].opacity as f32/255.);
            }
        }
    }
    let mut color=base;
    for &l in indices {
        let p=doc.layer_deposit_pixel(l,i);
        let (rgb,density)=deposit_optics(p);
        let opacity=doc.layers[l].opacity as f32/255.;
        let a=density*opacity;
        alpha+=a*(1.-alpha);
        color=blend_color(color,rgb,a,doc.layers[l].blend_mode);
        out.graphite_mass+=p.graphite_mass*opacity; out.clay_mass+=p.clay_mass*opacity; out.wax_mass+=p.wax_mass*opacity;
        out.loose_mass+=p.loose_mass*opacity; out.compacted_mass+=p.compacted_mass*opacity;
        out.orientation_x+=p.orientation_x*opacity; out.orientation_y+=p.orientation_y*opacity;
    }
    if alpha<=1e-8 { return PixelDepositState::default(); }
    let rgb: [f32;3]=std::array::from_fn(|j| ((color[j]-base[j]*(1.-alpha))/alpha).clamp(0.,1.));
    let scale=-(-alpha.min(1.-f32::EPSILON)).ln_1p()/out.optical_depth().max(1e-10);
    out.graphite_mass*=scale; out.clay_mass*=scale; out.wax_mass*=scale;
    out.loose_mass*=scale; out.compacted_mass*=scale; out.orientation_x*=scale; out.orientation_y*=scale;
    let mass=out.total_deposit();
    out.color_r_mass=rgb[0]*mass; out.color_g_mass=rgb[1]*mass; out.color_b_mass=rgb[2]*mass;
    out
}
/// Straight pigment RGB and transparency, without paper baked into drawing layers.
/// Layer opacity is deliberately separate: PSD writes it in the layer record, so channel alpha
/// must remain the original pigment coverage. The compositor applies opacity exactly once.
pub fn layer_pixel_rgba(document: &Document, layer: Option<usize>, index: usize) -> [f32; 4] {
    let (rgb, alpha) = match layer {
        Some(layer) => deposit_optics(document.layer_deposit_pixel(layer, index)),
        None => (paper_pixel_rgb(document, index), 1.0),
    };
    [rgb[0], rgb[1], rgb[2], alpha]
}

fn deposit_optics(deposit: PixelDepositState) -> ([f32; 3], f32) {
    let graphite = deposit.graphite_mass;
    let clay = deposit.clay_mass;
    let wax = deposit.wax_mass;
    let total = graphite + clay + wax;
    if total <= 1.0e-8 {
        return ([0.0; 3], 0.0);
    }

    let optical_density = (1.0 - (-deposit.optical_depth()).exp()).clamp(0.0, 1.0);

    // Pigment RGB is the selected color carried with the material, including optional
    // grain shades. Grade changes density and mechanics, never the chosen hue or value.
    let pencil_rgb = [
        (deposit.color_r_mass / total).clamp(0.0, 1.0),
        (deposit.color_g_mass / total).clamp(0.0, 1.0),
        (deposit.color_b_mass / total).clamp(0.0, 1.0),
    ];
    (pencil_rgb, optical_density)
}

fn blend_color(base: [f32; 3], pigment: [f32; 3], density: f32, mode: BlendMode) -> [f32; 3] {
    std::array::from_fn(|i| {
        let mixed = match mode {
            BlendMode::Normal => pigment[i],
            // Multiply only pigment, weighted by physical optical density. Sparse
            // graphite remains translucent; paper RGB is never multiplied twice.
            BlendMode::Multiply => base[i] * pigment[i],
            BlendMode::Darken => base[i].min(pigment[i]),
            BlendMode::Screen => 1.0 - (1.0 - base[i]) * (1.0 - pigment[i]),
            BlendMode::Lighten => base[i].max(pigment[i]),
        };
        (base[i] * (1.0 - density) + mixed * density).clamp(0.0, 1.0)
    })
}

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn to_u16(v: f32) -> u16 {
    (v.clamp(0.0, 1.0) * 65_535.0).round() as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_tile_spans_match_pixel_reference_after_layer_changes_and_partial_updates() {
        let spec = CanvasSpec { name: "Tile edges".into(), width_px: 73, height_px: 61,
            width_mm: 18.54, height_mm: 15.49, dpi: 100. };
        let mut doc = Document::new(spec, PaperPreset::DrawingMedium, "White", vec![[1.; 3]; 73*61]);
        for layer in 0..3 {
            if layer > 0 { doc.add_layer(); }
            for y in (layer..61).step_by(3) { for x in (layer..73).step_by(4) {
                let i = doc.index(x, y);
                seed_active_color(&mut doc, i, [0.2 + layer as f32 * 0.2, 0.3, 0.8]);
            } }
        }
        doc.add_layer();
        let mut renderer = RasterRenderer::default();
        for (n, mode) in BlendMode::ALL.into_iter().enumerate() {
            doc.activate_layer(n % 3);
            let id = doc.layers[1].id;
            doc.set_layer_blend_mode(id, mode);
            doc.set_layer_opacity(id, 43 + n as u8 * 40);
            doc.set_layer_visible(0, n % 2 == 0);
            let rendered = renderer.render_full(&doc);
            for y in 0..61 { for x in 0..73 {
                assert_eq!(rendered.pixels[doc.index(x,y)], shade_pixel_color32(&doc,x,y));
            } }
            let i = doc.index(33, 32);
            seed_active_color(&mut doc, i, [0.9, 0.1, 0.2]);
            let patch = renderer.render_region(&doc, DirtyRect::new(17, 19, 69, 55));
            for y in 19..55 { for x in 17..69 {
                assert_eq!(patch.pixels[(y-19)*52+x-17], shade_pixel_color32(&doc,x,y));
            } }
        }
    }

    #[test]
    fn pigment_retains_exact_selected_rgb_across_material_formulations() {
        for rgb in [
            [1., 0., 0.],
            [0., 1., 0.],
            [0., 0., 1.],
            [0.82, 0.15, 0.94],
            [0., 0., 0.],
            [1., 1., 1.],
            [0.75, 0.75, 0.75],
        ] {
            for (g, c, w) in [(0.68, 0.28, 0.04), (0.95, 0.03, 0.02)] {
                let mut doc = tiny_document();
                let i = doc.index(2, 2);
                doc.surface.graphite_mass[i] = g;
                doc.surface.clay_mass[i] = c;
                doc.surface.wax_mass[i] = w;
                doc.surface.compacted_mass[i] = 1.;
                doc.surface.color_r_mass[i] = rgb[0];
                doc.surface.color_g_mass[i] = rgb[1];
                doc.surface.color_b_mass[i] = rgb[2];
                let output = layer_pixel_rgba(&doc, Some(0), i);
                for channel in 0..3 {
                    assert!((output[channel] - rgb[channel]).abs() < 0.000001);
                }
            }
        }
    }

    #[test]
    fn paper_tint_changes_only_the_support_in_preview_and_export() {
        let mut doc = tiny_document();
        let i = doc.index(2, 2);
        seed_active_color(&mut doc, i, [0.6, 0.2, 0.1]);
        let deposit_before = layer_pixel_rgba(&doc, Some(0), i);
        let state_before = doc.surface.pixel(i);
        let white = paper_pixel_rgb(&doc, i);
        doc.set_paper_color([210, 180, 140]);
        let tinted = paper_pixel_rgb(&doc, i);
        for c in 0..3 {
            assert!(
                (tinted[c] - white[c] * doc.paper_color_rgb[c] as f32 / 255.0).abs() < 0.000001
            );
        }
        assert_eq!(deposit_before, layer_pixel_rgba(&doc, Some(0), i));
        assert_eq!(state_before, doc.surface.pixel(i));
        let exported = layer_pixel_rgba(&doc, None, i);
        assert_eq!(&exported[..3], &tinted);
        assert_eq!(exported[3], 1.0);
    }
    use crate::core::{
        document::CanvasSpec,
        paper::{generate_builtin_albedo, PaperPreset, PaperTexturePreset},
    };

    #[test]
    fn paper_texture_opacity_fades_only_visible_paper_and_defaults_for_old_projects() {
        let mut doc=tiny_document();let i=doc.index(2,2);
        doc.paper_albedo[i]=[0.55,0.72,0.81];doc.set_paper_color([210,180,140]);
        seed_active_color(&mut doc,i,[0.6,0.2,0.1]);
        let material=doc.surface.pixel(i);let ink=layer_pixel_rgba(&doc,Some(0),i);
        let full=paper_pixel_rgb(&doc,i);
        assert!(doc.set_paper_texture_opacity(0.));
        let flat=paper_pixel_rgb(&doc,i);assert_eq!(flat,doc.paper_color_rgb.map(|v|v as f32/255.));
        doc.set_paper_texture_opacity(0.5);let half=paper_pixel_rgb(&doc,i);
        for c in 0..3 {assert!((half[c]-(flat[c]+full[c])*0.5).abs()<1e-6);}
        assert_eq!(doc.surface.pixel(i),material);assert_eq!(layer_pixel_rgba(&doc,Some(0),i),ink);
        assert!(!doc.set_paper_texture_opacity(f32::NAN));assert_eq!(doc.paper_texture_opacity,0.5);
        let mut json=serde_json::to_value(&doc).unwrap();json.as_object_mut().unwrap().remove("paper_texture_opacity");
        assert_eq!(serde_json::from_value::<Document>(json).unwrap().paper_texture_opacity,1.);
    }

    fn tiny_document() -> Document {
        let spec = CanvasSpec::from_physical("tiny", 5.0, 5.0, 100.0);
        let albedo = generate_builtin_albedo(
            spec.width_px,
            spec.height_px,
            spec.dpi,
            PaperTexturePreset::White,
        );
        Document::new(spec, PaperPreset::DrawingMedium, "White", albedo)
    }

    fn seed_active_color(document: &mut Document, index: usize, rgb: [f32; 3]) {
        document.surface.graphite_mass[index] = 0.3;
        document.surface.loose_mass[index] = 0.3;
        document.surface.color_r_mass[index] = 0.3 * rgb[0];
        document.surface.color_g_mass[index] = 0.3 * rgb[1];
        document.surface.color_b_mass[index] = 0.3 * rgb[2];
    }

    #[test]
    fn hiding_top_layer_changes_composite() {
        let mut doc = tiny_document();
        let i = doc.index(2, 2);
        seed_active_color(&mut doc, i, [0.7, 0.1, 0.1]);
        doc.add_layer();
        seed_active_color(&mut doc, i, [0.1, 0.1, 0.8]);
        let with_top = shade_pixel_rgb(&doc, 2, 2);
        doc.set_layer_visible(doc.active_layer_index(), false);
        let without_top = shade_pixel_rgb(&doc, 2, 2);
        assert_ne!(with_top, without_top);
    }

    #[test]
    fn layer_opacity_blends_once_in_all_modes_without_changing_pigment() {
        let mut doc = tiny_document();
        let i = doc.index(2, 2);
        seed_active_color(&mut doc, i, [0.7, 0.1, 0.1]);
        doc.add_layer();
        seed_active_color(&mut doc, i, [0.1, 0.2, 0.8]);
        let id = doc.active_layer_id();
        let original = doc.surface.pixel(i);
        let raw = layer_pixel_rgba(&doc, Some(1), i);
        for mode in BlendMode::ALL {
            doc.set_layer_blend_mode(id, mode);
            doc.set_layer_opacity(id, 0);
            let base = shade_pixel_rgb(&doc, 2, 2);
            doc.set_layer_opacity(id, 255);
            let full = shade_pixel_rgb(&doc, 2, 2);
            doc.set_layer_opacity(id, 128);
            let half = shade_pixel_rgb(&doc, 2, 2);
            for c in 0..3 {
                assert!((half[c] - (base[c] + (full[c] - base[c]) * 128. / 255.)).abs() < 1e-6);
            }
            assert_eq!(raw, layer_pixel_rgba(&doc, Some(1), i));
            assert_eq!(original, doc.surface.pixel(i));
            assert_eq!(doc.layers[0].opacity, 255);
        }
        doc.move_active_layer_down();
        assert_eq!(doc.layers[doc.active_layer_index()].id, id);
        assert_eq!(doc.layers[doc.active_layer_index()].opacity, 128);
    }

    #[test]
    fn old_layers_default_to_full_opacity() {
        let doc = tiny_document();
        let mut old = serde_json::to_value(&doc.layers[0]).unwrap();
        old.as_object_mut().unwrap().remove("opacity");
        let layer: crate::core::document::DrawingLayer = serde_json::from_value(old).unwrap();
        assert_eq!(layer.opacity, 255);
    }

    #[test]
    fn multiply_preserves_gaps_and_darkens_overlapping_graphite() {
        let base = [0.8, 0.6, 0.4];
        let pigment = [0.3, 0.2, 0.1];
        for mode in BlendMode::ALL {
            assert_eq!(blend_color(base, pigment, 0.0, mode), base);
        }
        let first = blend_color(base, pigment, 0.4, BlendMode::Multiply);
        let second = blend_color(first, pigment, 0.4, BlendMode::Multiply);
        for i in 0..3 {
            assert!((first[i] - base[i] * (0.6 + 0.4 * pigment[i])).abs() < 1e-7);
            assert!(second[i] < first[i] && first[i] < base[i]);
        }
        assert_eq!(blend_color(base, pigment, 1.0, BlendMode::Normal), pigment);
        assert_eq!(blend_color(base, pigment, 1.0, BlendMode::Darken), pigment);
        assert_eq!(blend_color(base, pigment, 1.0, BlendMode::Lighten), base);
        let screen = blend_color(base, pigment, 1.0, BlendMode::Screen);
        for i in 0..3 {
            assert!(screen[i] >= base[i]);
        }
    }

    #[test]
    fn export_reflects_blend_changes_before_a_preview_refresh() {
        let mut doc = tiny_document();
        let i = doc.index(2, 2);
        seed_active_color(&mut doc, i, [0.5, 0.2, 0.1]);
        doc.add_layer();
        seed_active_color(&mut doc, i, [0.2, 0.3, 0.5]);
        let mut renderer = RasterRenderer::default();
        let multiply = renderer.rgba8(&doc);
        doc.set_layer_blend_mode(doc.active_layer_id(), BlendMode::Normal);
        let normal = renderer.rgba8(&doc);
        assert_ne!(&multiply[i * 4..i * 4 + 3], &normal[i * 4..i * 4 + 3]);
        let mut fresh = RasterRenderer::default();
        assert_eq!(normal, fresh.rgba8(&doc));
    }

    #[test]
    fn parallel_full_and_patch_match_serial_pixels() {
        let spec = CanvasSpec::from_physical("parallel", 130.0, 130.0, 100.0);
        let n = spec.pixel_count();
        let mut doc = Document::new(spec, PaperPreset::DrawingMedium, "White", vec![[1.0; 3]; n]);
        for i in (0..n).step_by(3) {
            seed_active_color(&mut doc, i, [0.5, 0.2, 0.1]);
        }
        let mut renderer = RasterRenderer::default();
        let full = renderer.render_full(&doc);
        for y in 0..doc.spec.height_px {
            for x in 0..doc.spec.width_px {
                assert_eq!(full[(x, y)], shade_pixel_color32(&doc, x, y));
            }
        }
        let rect = DirtyRect::new(9, 11, 60, 70);
        let index = doc.spec.width_px * 20 + 15;
        seed_active_color(&mut doc, index, [0.1, 0.2, 0.7]);
        let patch = renderer.render_region(&doc, rect);
        let mut cold = RasterRenderer::default();
        assert_eq!(patch, cold.render_region(&doc, rect));
        let updated = renderer.render_full(&doc);
        for y in 0..rect.height() {
            for x in 0..rect.width() {
                assert_eq!(patch[(x, y)], updated[(rect.min_x + x, rect.min_y + y)]);
            }
        }
    }
}
