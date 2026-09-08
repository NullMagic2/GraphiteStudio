//! Versioned, self-contained native projects. Pixel material is saved exactly;
//! paths retain the inputs needed to regenerate it after later edits.
use crate::core::{
    brush::BrushTip,
    document::Document,
    paper::CustomPaperTextureSource,
    pencil::{PencilTipState, ToolSettings},
    stroke::StrokeEngine,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{self, BufReader, BufWriter, Read, Write},
    path::Path,
    sync::Arc,
};
const MAGIC: &[u8; 9] = b"GRAPHITE\x01";
const MAX_FILE: u64 = 512 * 1024 * 1024;
const MAX_DECODED: u64 = 2 * 1024 * 1024 * 1024;
// Buffer the JSON side of compression too: serde emits/reads many tiny tokens.
const CODEC_BUFFER: usize = 256 * 1024;
#[derive(Debug, Serialize, Deserialize)]
pub struct Project {
    pub document: Document,
    pub settings: ToolSettings,
    pub tip: PencilTipState,
    pub engine: StrokeEngine,
    pub brushes: Vec<Arc<BrushTip>>,
    pub custom_paper: Option<CustomPaperTextureSource>,
    pub zoom: f32,
    pub pan: [f32; 2],
}
#[derive(Serialize)]
pub struct ProjectRef<'a> {
    pub document: &'a Document,
    pub settings: &'a ToolSettings,
    pub tip: &'a PencilTipState,
    pub engine: &'a StrokeEngine,
    pub brushes: &'a [Arc<BrushTip>],
    pub custom_paper: &'a Option<CustomPaperTextureSource>,
    pub zoom: f32,
    pub pan: [f32; 2],
}
struct Limited<W> {
    inner: W,
    left: u64,
}
impl<W: Write> Write for Limited<W> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if b.len() as u64 > self.left {
            return Err(io::Error::other("Project exceeds the supported save size"));
        }
        let n = self.inner.write(b)?;
        self.left -= n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
/// Complete a sibling temporary file before atomically replacing the requested file.
pub fn save(path: &Path, project: &ProjectRef<'_>) -> Result<(), String> {
    validate_ref(project)?;
    let name = path
        .file_name()
        .ok_or("Choose a project filename")?
        .to_string_lossy();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temp = path.with_file_name(format!(".{name}.{}.{}.tmp", std::process::id(), nonce));
    let result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        file.write_all(MAGIC).map_err(|e| e.to_string())?;
        {
            let compressed = Limited {
                inner: BufWriter::new(&mut file),
                left: MAX_FILE,
            };
            let encoder = flate2::write::GzEncoder::new(compressed, flate2::Compression::fast());
            let mut writer = Limited {
                inner: BufWriter::with_capacity(CODEC_BUFFER, encoder),
                left: MAX_DECODED,
            };
            serde_json::to_writer(&mut writer, project).map_err(|e| e.to_string())?;
            let encoder = writer.inner.into_inner().map_err(|e| e.to_string())?;
            let mut compressed = encoder.finish().map_err(|e| e.to_string())?;
            compressed.flush().map_err(|e| e.to_string())?;
            drop(compressed);
        }
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        std::fs::rename(&temp, path).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}
pub fn load(path: &Path) -> Result<Project, String> {
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("psd"))
    {
        return load_psd(path);
    }
    let file = File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_FILE + MAGIC.len() as u64 {
        return Err("Project file exceeds 512 MB".into());
    }
    decode(file)
}
fn decode(mut file: impl Read) -> Result<Project, String> {
    let mut magic = [0; 9];
    file.read_exact(&mut magic)
        .map_err(|_| "Not a complete Graphite project")?;
    if &magic != MAGIC {
        return Err("Not a supported Graphite project version".into());
    }
    let decoder = flate2::read::GzDecoder::new(BufReader::new(file));
    let mut reader = BufReader::with_capacity(CODEC_BUFFER, decoder.take(MAX_DECODED + 1));
    let mut project: Project = serde_json::from_reader(&mut reader)
        .map_err(|e| format!("Project data is incomplete or invalid: {e}"))?;
    if reader.get_ref().limit() == 0 {
        return Err("Project data exceeds the supported size".into());
    }
    validate(
        &project.document,
        &project.settings,
        &project.tip,
        &project.brushes,
    )?;
    if !project.zoom.is_finite()
        || !(0.03..=32.).contains(&project.zoom)
        || project.pan.iter().any(|p| !p.is_finite() || p.abs() > 1e7)
    {
        return Err("Invalid saved drawing view".into());
    }
    let (w, h) = (
        project.document.spec.width_px,
        project.document.spec.height_px,
    );
    project
        .document
        .selection
        .set(project.document.selection.polygon.clone(), w, h);
    // Masks are derived; store each polygon once instead of writing a full mask per stroke.
    // Vector replay rebuilds a retained stroke's selection when needed.
    project.document.mark_all_dirty();
    Ok(project)
}
fn tip_valid(t: &PencilTipState) -> bool {
    let n = t.profile.resolution as usize;
    (12..=64).contains(&n)
        && t.profile.height.len() == n * n
        && t.profile.pending_wear.len() == n * n
        && t.core_diameter_mm.is_finite()
        && (1.0..=PencilTipState::MAX_CORE_DIAMETER_MM).contains(&t.core_diameter_mm)
        && t.profile
            .height
            .iter()
            .chain(t.profile.pending_wear.iter())
            .all(|v| v.is_finite())
}
fn brush_valid(b: &BrushTip) -> bool {
    b.width > 0
        && b.height > 0
        && b.width <= 8192
        && b.height <= 8192
        && b.width * b.height == b.mask.len()
        && b.mask.len() <= 16_777_216
}
pub(crate) fn settings_valid(s: &ToolSettings) -> bool {
    s.tip_sharpness.is_finite()
        && (0.0..=1.0).contains(&s.tip_sharpness)
        && s.shape_opacity.is_finite()
        && (0.0..=1.0).contains(&s.shape_opacity)
        && [
            s.pencil_core_diameter_mm,
            s.pencil_geometry_scale,
            s.smudge_size_px,
            s.eraser_diameter_mm,
            s.brush_size_px,
            s.tissue_size_px,
            s.shape_width_px,
        ]
        .iter()
        .all(|v| v.is_finite() && *v > 0. && *v <= 1e6)
        && [
            s.brush_angle_deg,
            s.tilt_deg,
            s.azimuth_deg,
            s.mouse_pressure,
            s.pen_pressure_gamma,
            s.flow,
            s.smudge_strength,
            s.particle_variation,
            s.eraser_strength,
            s.line_smoothing,
            s.tissue_load,
            s.tissue_random_graphite,
        ]
        .iter()
        .all(|v| v.is_finite() && v.abs() < 1e6)
        && s.brush_tip.as_ref().is_none_or(|b| brush_valid(b))
        && s.pencil_texture.as_ref().is_none_or(|b| brush_valid(b))
}
fn selection_valid(s: &crate::core::selection::Selection) -> bool {
    s.polygon.len() <= 4096
        && s.polygon
            .iter()
            .all(|p| p.x.is_finite() && p.y.is_finite() && p.x.abs() < 1e7 && p.y.abs() < 1e7)
}
fn sparse_valid(s: &crate::core::material::SparseDepositState, w: usize, h: usize) -> bool {
    let tiles = w.div_ceil(32) * h.div_ceil(32);
    s.width == w
        && s.height == h
        && s.tiles.len() <= tiles
        && s.tiles.iter().all(|(i, t)| *i < tiles && t.len() == 1024)
}
fn validate(
    doc: &Document,
    settings: &ToolSettings,
    tip: &PencilTipState,
    brushes: &[Arc<BrushTip>],
) -> Result<(), String> {
    let (w, h) = (doc.spec.width_px, doc.spec.height_px);
    let n = w.checked_mul(h).ok_or("Invalid paper size")?;
    if w == 0
        || h == 0
        || n > 12_000_000
        || !doc.spec.dpi.is_finite()
        || !(1.0..=2400.0).contains(&doc.spec.dpi)
        || !doc.spec.width_mm.is_finite()
        || !doc.spec.height_mm.is_finite()
    {
        return Err("Unsupported project paper size".into());
    }
    let s = &doc.surface;
    let fields = [
        &s.rest_height,
        &s.fiber,
        &s.current_height,
        &s.abrasion,
        &s.graphite_mass,
        &s.clay_mass,
        &s.wax_mass,
        &s.loose_mass,
        &s.compacted_mass,
        &s.orientation_x,
        &s.orientation_y,
        &s.color_r_mass,
        &s.color_g_mass,
        &s.color_b_mass,
    ];
    if fields
        .iter()
        .any(|v| v.len() != n || v.iter().any(|f| !f.is_finite()))
        || s.contact_support.len() != n
        || s.edge_grain.len() != n
        || doc.color_grain.len() != n
        || doc.paper_albedo.len() != n
        || doc.paper_albedo.iter().flatten().any(|v| !v.is_finite())
    {
        return Err("Project material channels do not match the paper".into());
    }
    if doc.layers.is_empty()
        || doc.layers.len() > 256
        || doc.active_layer >= doc.layers.len()
        || doc.next_layer_id <= doc.layers.iter().map(|l| l.id).max().unwrap_or(0)
        || doc.revision == u64::MAX
    {
        return Err("Invalid project layer structure".into());
    }
    let mut ids = std::collections::HashSet::new();
    for layer in &doc.layers {
        if !ids.insert(layer.id)
            || layer.id == 0
            || !sparse_valid(&layer.deposit, w, h)
            || layer
                .vectors
                .base
                .as_ref()
                .is_some_and(|b| !sparse_valid(b, w, h))
            || layer.vectors.strokes.len() > 100_000
        {
            return Err("Invalid saved layer".into());
        }
        for stroke in &layer.vectors.strokes {
            if !settings_valid(&stroke.settings)
                || !tip_valid(&stroke.tip)
                || !selection_valid(&stroke.selection)
                || stroke.points.len() > 2_000_000
                || stroke.points.iter().any(|p| {
                    [
                        p.x,
                        p.y,
                        p.pressure,
                        p.tilt_deg,
                        p.azimuth_deg,
                        p.rotation_deg.unwrap_or(0.),
                    ]
                    .iter()
                    .any(|v| !v.is_finite() || v.abs() > 1e7)
                })
            {
                return Err("Invalid saved path or brush".into());
            }
        }
    }
    if !settings_valid(settings)
        || !tip_valid(tip)
        || !selection_valid(&doc.selection)
        || brushes.len() > 512
        || brushes.iter().any(|b| !brush_valid(b))
        || brushes.iter().map(|b| b.mask.len()).sum::<usize>() > 64 * 1024 * 1024
    {
        return Err("Invalid project tool settings".into());
    }
    Ok(())
}

// Photoshop-native paths/shape layers remain standard PSD data. This additional
// resource retains Graphite's physical material and pen inputs for exact reopening.
const PSD_MAGIC: &[u8; 8] = b"GSPROJ01";
fn resource_location(file: &mut File) -> Result<(u64, u64), String> {
    use std::io::{Seek, SeekFrom};
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut header = [0; 26];
    file.read_exact(&mut header).map_err(|e| e.to_string())?;
    if &header[..6] != b"8BPS\0\x01" {
        return Err("Not a supported PSD file".into());
    }
    fn u32be(f: &mut File) -> Result<u32, String> {
        let mut b = [0; 4];
        f.read_exact(&mut b).map_err(|e| e.to_string())?;
        Ok(u32::from_be_bytes(b))
    }
    let colors = u32be(file)? as i64;
    file.seek(SeekFrom::Current(colors))
        .map_err(|e| e.to_string())?;
    let length = u32be(file)? as u64;
    let end = file
        .stream_position()
        .map_err(|e| e.to_string())?
        .checked_add(length)
        .ok_or("Invalid PSD size")?;
    if end > file.metadata().map_err(|e| e.to_string())?.len() {
        return Err("Truncated PSD resources".into());
    }
    while file.stream_position().map_err(|e| e.to_string())? < end {
        let mut block = [0; 7];
        file.read_exact(&mut block).map_err(|e| e.to_string())?;
        if &block[..4] != b"8BIM" {
            return Err("Invalid PSD resource signature".into());
        }
        let id = u16::from_be_bytes([block[4], block[5]]);
        let n = block[6] as usize;
        let mut name = vec![0; n];
        file.read_exact(&mut name).map_err(|e| e.to_string())?;
        if (n + 1) % 2 != 0 {
            file.seek(SeekFrom::Current(1)).map_err(|e| e.to_string())?;
        }
        let size = u32be(file)? as u64;
        let pos = file.stream_position().map_err(|e| e.to_string())?;
        if pos + size + (size % 2) > end {
            return Err("Truncated PSD resource".into());
        }
        if id == 4000 && name == b"GraphiteStudio.Project.v1" {
            return Ok((pos, size));
        }
        file.seek(SeekFrom::Current((size + size % 2) as i64))
            .map_err(|e| e.to_string())?;
    }
    Err("This PSD has no Graphite material state to reopen. Its native paths and shape layers can be edited in Photoshop. Open a .graphite project for full material editing here.".into())
}
fn psd_fingerprint(file: &mut File, skip: (u64, u64)) -> Result<u64, String> {
    use std::io::{Seek, SeekFrom};
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut hash = 0xcbf29ce484222325u64;
    let mut pos = 0u64;
    let mut b = [0; 65536];
    loop {
        let n = file.read(&mut b).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        for &byte in &b[..n] {
            if pos < skip.0 || pos >= skip.0 + skip.1 {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
            }
            pos += 1;
        }
    }
    Ok(hash)
}
pub fn save_psd<R: crate::render::DocumentRenderer>(
    path: &Path,
    project: &ProjectRef<'_>,
    renderer: &mut R,
    depth: crate::export::PsdBitDepth,
) -> Result<(), String> {
    use std::io::{Seek, SeekFrom};
    validate_ref(project)?;
    let mut encoder = Limited {
        inner: BufWriter::with_capacity(CODEC_BUFFER,
            flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast())),
        left: MAX_DECODED,
    };
    serde_json::to_writer(&mut encoder, project).map_err(|e| e.to_string())?;
    let encoder = encoder.inner.into_inner().map_err(|e| e.to_string())?;
    let compressed = encoder.finish().map_err(|e| e.to_string())?;
    if compressed.len() as u64 > MAX_FILE {
        return Err("Project data exceeds 512 MB".into());
    }
    let mut payload = PSD_MAGIC.to_vec();
    payload.extend(0u64.to_be_bytes());
    payload.extend(MAGIC);
    payload.extend(compressed);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temp = path.with_file_name(format!(".graphite-{}-{nonce}.tmp.psd", std::process::id()));
    let result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        {
            let mut w = BufWriter::new(&mut file);
            crate::export::write_editable_psd(&mut w, renderer, project.document, depth, &payload)
                .map_err(|e| e.to_string())?;
            w.flush().map_err(|e| e.to_string())?;
        }
        let location = resource_location(&mut file)?;
        let fingerprint = psd_fingerprint(&mut file, location)?;
        file.seek(SeekFrom::Start(location.0 + 8))
            .map_err(|e| e.to_string())?;
        file.write_all(&fingerprint.to_be_bytes())
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        std::fs::rename(&temp, path).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}
fn load_psd(path: &Path) -> Result<Project, String> {
    use std::io::{Seek, SeekFrom};
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let location = resource_location(&mut file)?;
    if location.1 < 25 || location.1 > MAX_FILE + 25 {
        return Err("Unsupported embedded project size".into());
    }
    let fingerprint = psd_fingerprint(&mut file, location)?;
    file.seek(SeekFrom::Start(location.0))
        .map_err(|e| e.to_string())?;
    let mut head = [0; 16];
    file.read_exact(&mut head).map_err(|e| e.to_string())?;
    if &head[..8] != PSD_MAGIC {
        return Err("Unsupported Graphite PSD project version".into());
    }
    if u64::from_be_bytes(head[8..].try_into().unwrap()) != fingerprint {
        return Err("This PSD was changed outside Graphite Studio. Its Photoshop geometry remains usable, but the saved Graphite material state no longer matches. Open the .graphite project to continue here; external pixel/path edits are not imported.".into());
    }
    decode(file.take(location.1 - 16))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{
        document::{BlendMode, CanvasSpec},
        history::EditTransaction,
        paper::{generate_builtin_albedo, PaperPreset, PaperTexturePreset},
        pencil::ToolKind,
        selection::Selection,
        stroke::StrokePoint,
        vector::{VectorLayer, VectorStroke},
    };
    fn project() -> Project {
        let spec = CanvasSpec::from_physical("Saved", 30., 30., 120.);
        let albedo = generate_builtin_albedo(
            spec.width_px,
            spec.height_px,
            spec.dpi,
            PaperTexturePreset::Ivory,
        );
        let mut doc = Document::new(spec, PaperPreset::DrawingMedium, "Ivory", albedo);
        doc.set_paper_color([244, 230, 200]);
        let tip = PencilTipState::default();
        let engine = StrokeEngine::default();
        let mut settings = ToolSettings::default();
        settings.tool = ToolKind::Brush;
        settings.tissue_random_graphite = 0.73;
        settings.shape_line_snap = true;
        settings.transform_keep_aspect = true;
        settings.pencil_color_rgb = [180, 30, 70];
        settings.brush_tip = Some(Arc::new(BrushTip {
            name: "Embedded".into(),
            width: 3,
            height: 2,
            mask: vec![255, 50, 255, 0, 180, 255],
        }));
        settings.pencil_texture = settings.brush_tip.clone();
        for layer in 0..2 {
            if layer == 1 {
                doc.add_layer();
                doc.layers[1].blend_mode = BlendMode::Screen;
                doc.layers[1].opacity = 96;
                doc.layers[1].visible = false;
            }
            let p = StrokePoint {
                x: 30.,
                y: 40. + layer as f32 * 30.,
                pressure: 0.7,
                tilt_deg: 55.,
                azimuth_deg: 35.,
                rotation_deg: Some(62.),
            };
            let stroke = Arc::new(VectorStroke {
                label: "Brush".into(),
                settings: settings.clone(),
                tip: tip.clone(),
                engine: engine.clone(),
                points: vec![
                    p,
                    StrokePoint {
                        x: 100.,
                        pressure: 0.4,
                        ..p
                    },
                ],
                polyline: false,
                selection: Selection::default(),
            });
            stroke.apply(&mut doc, &mut EditTransaction::default());
            doc.layers[layer].vectors = Arc::new(VectorLayer {
                base: None,
                strokes: vec![stroke],
            });
        }
        doc.selection.set(
            vec![
                eframe::egui::Vec2::ZERO,
                eframe::egui::Vec2::new(60., 0.),
                eframe::egui::Vec2::new(60., 120.),
                eframe::egui::Vec2::new(0., 120.),
            ],
            doc.spec.width_px,
            doc.spec.height_px,
        );
        Project {
            document: doc,
            settings: settings.clone(),
            tip,
            engine,
            brushes: vec![settings.brush_tip.unwrap()],
            custom_paper: None,
            zoom: 1.5,
            pan: [12., 15.],
        }
    }
    fn data(p: &Project) -> ProjectRef<'_> {
        ProjectRef {
            document: &p.document,
            settings: &p.settings,
            tip: &p.tip,
            engine: &p.engine,
            brushes: &p.brushes,
            custom_paper: &p.custom_paper,
            zoom: p.zoom,
            pan: p.pan,
        }
    }
    fn path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("graphite-test-{}-{name}", std::process::id()))
    }
    fn assert_same(a: &Project, b: &Project) {
        assert_eq!(b.settings.tissue_random_graphite, a.settings.tissue_random_graphite);
        assert!(b.settings.shape_line_snap);
        assert!(b.settings.transform_keep_aspect);
        assert!(b.document.layers[0].vectors.strokes[0].settings.shape_line_snap);
        assert_eq!(b.document.layers[0].vectors.strokes[0].settings.tissue_random_graphite, 0.73);
        assert_eq!(
            a.document.surface.graphite_mass,
            b.document.surface.graphite_mass
        );
        assert_eq!(a.document.paper_albedo, b.document.paper_albedo);
        assert_eq!(
            a.document.surface.current_height,
            b.document.surface.current_height
        );
        assert_eq!(b.document.layers.len(), 2);
        assert!(!b.document.layers[1].visible);
        assert_eq!(b.document.layers[1].blend_mode, BlendMode::Screen);
        assert_eq!(b.document.layers[1].opacity, 96);
        assert_eq!(b.document.layers[0].opacity, 255);
        assert_eq!(b.document.active_layer_index(), 1);
        assert_eq!(
            b.document.layers[0].vectors.strokes[0].points[0].rotation_deg,
            Some(62.)
        );
        assert_eq!(
            b.document.layers[1].vectors.strokes[0].points[1].pressure,
            0.4
        );
        assert_eq!(b.brushes[0].mask, [255, 50, 255, 0, 180, 255]);
        assert_eq!(b.zoom, 1.5);
        assert_eq!(b.pan, [12., 15.]);
        assert!(!b.document.selection.allows(b.document.index(100, 30)));
        assert!(b.document.selection.allows(b.document.index(20, 30)));
        for i in 0..a.document.spec.pixel_count() {
            assert_eq!(
                a.document.layer_total_deposit(0, i),
                b.document.layer_total_deposit(0, i)
            );
        }
    }
    #[test]
    fn buffered_projects_are_compatible_with_the_previous_unbuffered_codec() {
        let p = project();
        let mut legacy = MAGIC.to_vec();
        let mut old_writer = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        serde_json::to_writer(&mut old_writer, &data(&p)).unwrap();
        legacy.extend(old_writer.finish().unwrap());
        assert_same(&p, &decode(legacy.as_slice()).unwrap());

        let path = path("codec-compatibility.graphite");
        save(&path, &data(&p)).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..MAGIC.len()], MAGIC);
        let old_reader = flate2::read::GzDecoder::new(&bytes[MAGIC.len()..]);
        let mut decoded: Project = serde_json::from_reader(old_reader).unwrap();
        // The old loader also rebuilt derived selection masks after decoding.
        decoded.document.selection.set(decoded.document.selection.polygon.clone(),
            decoded.document.spec.width_px, decoded.document.spec.height_px);
        assert_same(&p, &decoded);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn native_project_round_trip_keeps_material_layers_sensors_masks_and_embedded_brushes() {
        let original = project();
        let path = path("roundtrip.graphite");
        save(&path, &data(&original)).unwrap();
        let reopened = load(&path).unwrap();
        assert_same(&original, &reopened);
        // Save over an existing project safely, rather than relying on a nonexistent-file case.
        save(&path, &data(&reopened)).unwrap();
        assert_same(&original, &load(&path).unwrap());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn original_custom_paper_is_embedded_and_reusable_after_reopening() {
        let mut p = project();
        let image_path = path("paper.png");
        let image = image::RgbaImage::from_fn(7, 9, |x, y| {
            image::Rgba([(x * 30) as u8, (y * 20) as u8, 90, 255])
        });
        image.save(&image_path).unwrap();
        p.custom_paper = Some(crate::core::paper::load_custom_texture_source(&image_path).unwrap());
        let expected = p.custom_paper.as_ref().unwrap().render_albedo(31, 41);
        let path = path("custom.graphite");
        save(&path, &data(&p)).unwrap();
        std::fs::remove_file(image_path).unwrap();
        let reopened = load(&path).unwrap();
        assert_eq!(
            expected,
            reopened.custom_paper.unwrap().render_albedo(31, 41)
        );
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn bad_project_is_rejected_and_failed_save_preserves_previous_file() {
        let mut original = project();
        let path = path("invalid.graphite");
        save(&path, &data(&original)).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        original.document.surface.graphite_mass.pop();
        assert!(save(&path, &data(&original)).is_err());
        assert_eq!(bytes, std::fs::read(&path).unwrap());
        let truncated = path.with_extension("broken.graphite");
        std::fs::write(&truncated, &bytes[..bytes.len() - 7]).unwrap();
        assert!(load(&truncated).is_err());
        std::fs::write(&truncated, b"Not a project").unwrap();
        assert!(load(&truncated).is_err());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(truncated).unwrap();
    }
    #[test]
    fn editable_psd_round_trip_in_both_depths_and_external_edits_are_not_silently_lost() {
        let original = project();
        for depth in [
            crate::export::PsdBitDepth::Eight,
            crate::export::PsdBitDepth::Sixteen,
        ] {
            let path = path(&format!("roundtrip-{}.psd", depth.bits()));
            let mut renderer = crate::render::RasterRenderer::default();
            save_psd(&path, &data(&original), &mut renderer, depth).unwrap();
            assert_same(&original, &load(&path).unwrap());
            let mut bytes = std::fs::read(&path).unwrap();
            let last = bytes.len() - 1;
            bytes[last] ^= 1;
            std::fs::write(&path, bytes).unwrap();
            assert!(load(&path).unwrap_err().contains("changed outside"));
            std::fs::remove_file(path).unwrap();
        }
    }
}

fn validate_ref(p: &ProjectRef<'_>) -> Result<(), String> {
    validate(p.document, p.settings, p.tip, p.brushes)?;
    if !p.zoom.is_finite()
        || !(0.03..=32.).contains(&p.zoom)
        || p.pan.iter().any(|v| !v.is_finite() || v.abs() > 1e7)
        || p.custom_paper
            .as_ref()
            .is_some_and(|s| !s.project_size_valid())
    {
        return Err("Unsupported saved view or custom paper image size".into());
    }
    Ok(())
}
