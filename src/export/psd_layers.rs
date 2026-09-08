//! PSD layer records and planar channels, following Adobe's file-format specification.
//! 16-bit layer data belongs in the global Lr16 tagged block.
use super::{be_u16, be_u32, invalid_input, write_psd_header_with_resources, PsdBitDepth};
use crate::{
    core::document::{BlendMode, DirtyRect, Document},
    render::{layer_pixel_rgba, DocumentRenderer},
};
use std::io::{self, Seek, SeekFrom, Write};

struct Layer<'a> {
    opacity: u8,
    index: Option<usize>,
    bounds: DirtyRect,
    name: std::borrow::Cow<'a, str>,
    shape: Option<&'a crate::core::vector::VectorStroke>,
    visible: bool,
    mode: BlendMode,
    id: u32,
}

fn bounds(doc: &Document, layer: usize) -> DirtyRect {
    let mut rect = DirtyRect::new(doc.spec.width_px, doc.spec.height_px, 0, 0);
    for y in 0..doc.spec.height_px {
        for x in 0..doc.spec.width_px {
            if doc.layer_total_deposit(layer, doc.index(x, y)) > 1e-8 {
                rect.min_x = rect.min_x.min(x);
                rect.min_y = rect.min_y.min(y);
                rect.max_x = rect.max_x.max(x + 1);
                rect.max_y = rect.max_y.max(y + 1);
            }
        }
    }
    if rect.is_empty() {
        DirtyRect::new(0, 0, 1, 1)
    } else {
        rect
    }
}

fn begin_length<W: Write + Seek>(w: &mut W) -> io::Result<u64> {
    let pos = w.stream_position()?;
    be_u32(w, 0)?;
    Ok(pos)
}
fn finish_length<W: Write + Seek>(w: &mut W, pos: u64) -> io::Result<()> {
    let end = w.stream_position()?;
    let length = u32::try_from(end - pos - 4).map_err(|_| {
        invalid_input("PSD section exceeds 4 GB; reduce document size or layer count")
    })?;
    w.seek(SeekFrom::Start(pos))?;
    be_u32(w, length)?;
    w.seek(SeekFrom::Start(end))?;
    Ok(())
}
fn pad<W: Write + Seek>(w: &mut W, start: u64, multiple: u64) -> io::Result<()> {
    while (w.stream_position()? - start) % multiple != 0 {
        w.write_all(&[0])?;
    }
    Ok(())
}
fn key(mode: BlendMode) -> &'static [u8; 4] {
    match mode {
        BlendMode::Normal => b"norm",
        BlendMode::Multiply => b"mul ",
        BlendMode::Darken => b"dark",
        BlendMode::Screen => b"scrn",
        BlendMode::Lighten => b"lite",
    }
}

#[cfg(test)]
mod opacity_tests {
    use super::*;
    #[test]
    fn native_psd_opacity_is_separate_from_pixel_alpha_at_both_depths() {
        use crate::core::{document::CanvasSpec, paper::PaperPreset};
        let mut doc = Document::new(
            CanvasSpec {
                width_px: 1,
                height_px: 1,
                width_mm: 1.,
                height_mm: 1.,
                dpi: 120.,
                name: "Opacity".into(),
            },
            PaperPreset::DrawingMedium,
            "White",
            vec![[1.; 3]],
        );
        doc.surface.graphite_mass[0] = 0.3;
        doc.surface.loose_mass[0] = 0.3;
        doc.surface.color_r_mass[0] = 0.06;
        doc.surface.color_g_mass[0] = 0.09;
        doc.surface.color_b_mass[0] = 0.12;
        doc.set_layer_opacity(doc.active_layer_id(), 128);
        let raw_alpha = layer_pixel_rgba(&doc, Some(0), 0)[3];
        for depth in [8, 16] {
            let mut stream = std::io::Cursor::new(Vec::new());
            write_info(
                &mut stream,
                &doc,
                &[Layer {
                    opacity: 128,
                    index: Some(0),
                    bounds: DirtyRect::new(0, 0, 1, 1),
                    name: "Layer".into(),
                    shape: None,
                    visible: true,
                    mode: BlendMode::Multiply,
                    id: 1,
                }],
                depth,
            )
            .unwrap();
            let bytes = stream.into_inner();
            // Count + rectangle + channel count + 4 channel descriptors + blend signature/key.
            assert_eq!(bytes[52], 128);
            let extra = u32::from_be_bytes(bytes[56..60].try_into().unwrap()) as usize;
            let alpha = 60 + extra + 3 * (2 + depth as usize / 8) + 2;
            if depth == 8 {
                assert_eq!(bytes[alpha], (raw_alpha * 255.).round() as u8);
            } else {
                assert_eq!(
                    u16::from_be_bytes(bytes[alpha..alpha + 2].try_into().unwrap()),
                    (raw_alpha * 65535.).round() as u16
                );
            }
        }
    }
}

fn write_info<W: Write + Seek>(
    w: &mut W,
    doc: &Document,
    layers: &[Layer<'_>],
    depth: u16,
) -> io::Result<()> {
    let start = w.stream_position()?;
    be_u16(w, layers.len() as u16)?;
    for layer in layers {
        let r = layer.bounds;
        for v in [r.min_y, r.min_x, r.max_y, r.max_x] {
            be_u32(w, v as u32)?;
        }
        be_u16(w, if layer.shape.is_some() { 0 } else { 4 })?;
        let length = u32::try_from(2 + r.width() * r.height() * (depth as usize / 8))
            .map_err(|_| invalid_input("PSD layer channel exceeds 4 GB"))?;
        for channel in [0_u16, 1, 2, 65535]
            .into_iter()
            .take(if layer.shape.is_some() { 0 } else { 4 })
        {
            be_u16(w, channel)?;
            be_u32(w, length)?;
        }
        w.write_all(b"8BIM")?;
        w.write_all(key(layer.mode))?;
        w.write_all(&[
            layer.opacity,
            0,
            (if layer.visible { 0 } else { 2 }) | (if layer.shape.is_some() { 24 } else { 0 }),
            0,
        ])?;
        let extra = begin_length(w)?;
        be_u32(w, 0)?; // no user mask
        be_u32(w, 0)?; // default blending ranges
        let name: Vec<u8> = layer
            .name
            .chars()
            .take(255)
            .map(|c| if c.is_ascii() { c as u8 } else { b'?' })
            .collect();
        let pascal = w.stream_position()?;
        w.write_all(&[name.len() as u8])?;
        w.write_all(&name)?;
        pad(w, pascal, 4)?;
        // Unicode names, including non-Latin names and supplementary characters.
        w.write_all(b"8BIMluni")?;
        let unicode = begin_length(w)?;
        let units: Vec<u16> = layer.name.encode_utf16().collect();
        be_u32(w, units.len() as u32)?;
        for unit in units {
            be_u16(w, unit)?;
        }
        pad(w, unicode + 4, 4)?;
        finish_length(w, unicode)?;
        w.write_all(b"8BIMlyid")?;
        be_u32(w, 4)?;
        be_u32(w, layer.id)?;
        if let Some(shape) = layer.shape {
            super::psd_vectors::tagged(
                w,
                b"SoCo",
                &super::psd_vectors::solid_color(shape.settings.pencil_color_rgb),
            )?;
            super::psd_vectors::tagged(w, b"vmsk", &super::psd_vectors::shape_mask(shape, doc))?;
        }
        finish_length(w, extra)?;
    }
    // Keep only one cropped layer's optical samples in memory at a time.
    for layer in layers {
        if layer.shape.is_some() {
            continue;
        }
        let r = layer.bounds;
        let mut rgba = Vec::with_capacity(r.width() * r.height());
        for y in r.min_y..r.max_y {
            for x in r.min_x..r.max_x {
                rgba.push(if layer.shape.is_some() {
                    [0.; 4]
                } else {
                    layer_pixel_rgba(doc, layer.index, doc.index(x, y))
                });
            }
        }
        for channel in 0..4 {
            be_u16(w, 0)?; // raw planar channel
            for pixel in &rgba {
                let v = pixel[channel].clamp(0.0, 1.0);
                if depth == 8 {
                    w.write_all(&[(v * 255.0).round() as u8])?;
                } else {
                    be_u16(w, (v * 65535.0).round() as u16)?;
                }
            }
        }
    }
    pad(w, start, 2)
}

pub(super) fn write_layered_psd<W: Write + Seek, R: DocumentRenderer>(
    w: &mut W,
    renderer: &mut R,
    doc: &Document,
    depth: PsdBitDepth,
    project: Option<&[u8]>,
) -> io::Result<()> {
    if doc.layer_count() >= 32767 {
        return Err(invalid_input(
            "PSD supports at most 32766 drawing layers plus paper",
        ));
    }
    let bits = depth.bits();
    let mut resources = super::psd_vectors::resources(doc);
    if let Some(project) = project {
        super::psd_vectors::resource(&mut resources, 4000, "GraphiteStudio.Project.v1", project);
    }
    write_psd_header_with_resources(
        w,
        doc.spec.width_px as u32,
        doc.spec.height_px as u32,
        doc.spec.dpi,
        bits,
        &resources,
    )?;
    // Header helper includes an empty layer/mask length; replace that final marker.
    w.seek(SeekFrom::Current(-4))?;
    let section = begin_length(w)?;
    let mut layers = vec![Layer {
        opacity: 255,
        index: None,
        bounds: DirtyRect::full(doc.spec.width_px, doc.spec.height_px),
        name: "Paper".into(),
        shape: None,
        visible: true,
        mode: BlendMode::Normal,
        id: 0,
    }];
    // PSD records are stored bottom-to-top, matching document storage.
    for (index, l) in doc.layers.iter().enumerate() {
        layers.push(Layer {
            opacity: l.opacity,
            index: Some(index),
            bounds: bounds(doc, index),
            name: l.name.as_str().into(),
            shape: None,
            visible: l.visible,
            mode: l.blend_mode,
            id: l.id as u32,
        });
    }
    // Native, independently editable shape alternatives are hidden so the original
    // textured composite opens unchanged. Photoshop's Paths panel contains every
    // stroke's native centerline, including non-geometric drawing tools.
    for layer in &doc.layers {
        for (i, shape) in layer.vectors.strokes.iter().enumerate().filter(|(_, s)| {
            s.polyline
                || matches!(
                    s.settings.tool,
                    crate::core::pencil::ToolKind::Pencil
                        | crate::core::pencil::ToolKind::Brush
                        | crate::core::pencil::ToolKind::Tissue
                )
        }) {
            if layers.len() >= 32766 {
                return Err(invalid_input("Too many native shape layers for PSD"));
            }
            layers.push(Layer {
                opacity: (layer.opacity as f32 * shape.opacity()).round() as u8,
                index: None,
                bounds: DirtyRect::new(0, 0, 0, 0),
                name: format!("Vector - {} - {} {}", layer.name, i + 1, shape.label).into(),
                shape: Some(shape),
                visible: false,
                mode: layer.blend_mode,
                id: 0x40000000 + layers.len() as u32,
            });
        }
    }
    if bits == 8 {
        let info = begin_length(w)?;
        write_info(w, doc, &layers, bits)?;
        finish_length(w, info)?;
        be_u32(w, 0)?; // global mask
    } else {
        be_u32(w, 0)?; // empty legacy 8-bit layer info
        be_u32(w, 0)?; // global mask
        w.write_all(b"8BIMLr16")?;
        let info = begin_length(w)?;
        write_info(w, doc, &layers, bits)?;
        pad(w, info + 4, 4)?;
        finish_length(w, info)?;
    }
    finish_length(w, section)?;
    be_u16(w, 0)?; // merged image, raw RGB planes
    if bits == 8 {
        let rgba = renderer.rgba8(doc);
        for channel in 0..3 {
            for pixel in rgba.chunks_exact(4) {
                w.write_all(&[pixel[channel]])?;
            }
        }
    } else {
        let rgb = renderer.rgb16(doc);
        for channel in 0..3 {
            for pixel in rgb.chunks_exact(3) {
                be_u16(w, pixel[channel])?;
            }
        }
    }
    Ok(())
}
