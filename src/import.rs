//! Opening documents is separate from restoring exact native material state.
//! Raster images become a normal drawing layer, retaining their color and alpha.
mod psd;

use crate::{
    core::{
        document::{BlendMode, CanvasSpec, Document},
        material::PixelDepositState,
        paper::PaperPreset,
        pencil::{PencilTipState, ToolSettings},
        stroke::StrokeEngine,
    },
    project::Project,
};
use image::ImageDecoder;
use std::{fs::File, io::BufReader, path::Path};

pub const OPEN_EXTENSIONS: &[&str] = &["graphite", "psd", "png", "jpg", "jpeg", "bmp"];

// Retain ordinary 8-bit layer samples in four bytes rather than sixteen while
// other layers are decoded. Only exactly representable samples are compacted:
// higher-depth colors and fractional masks keep their original float precision.
enum RasterPixels {
    Bytes(Vec<[u8; 4]>),
    Float(Vec<[f32; 4]>),
}
impl From<Vec<[f32; 4]>> for RasterPixels {
    fn from(pixels: Vec<[f32; 4]>) -> Self {
        if pixels
            .iter()
            .flatten()
            .all(|&v| v.is_finite() && v == (v * 255.).round().clamp(0., 255.) as u8 as f32 / 255.)
        {
            Self::Bytes(
                pixels
                    .iter()
                    .map(|p| p.map(|v| (v * 255.).round() as u8))
                    .collect(),
            )
        } else {
            Self::Float(pixels)
        }
    }
}
impl RasterPixels {
    fn len(&self) -> usize {
        match self {
            Self::Bytes(p) => p.len(),
            Self::Float(p) => p.len(),
        }
    }
    fn get(&self, index: usize) -> [f32; 4] {
        match self {
            Self::Bytes(p) => p[index].map(|v| v as f32 / 255.),
            Self::Float(p) => p[index],
        }
    }
    fn iter(&self) -> impl Iterator<Item = [f32; 4]> + '_ {
        (0..self.len()).map(|i| self.get(i))
    }
    fn into_iter(self) -> Box<dyn Iterator<Item = [f32; 4]>> {
        match self {
            Self::Bytes(p) => Box::new(p.into_iter().map(|p| p.map(|v| v as f32 / 255.))),
            Self::Float(p) => Box::new(p.into_iter()),
        }
    }
}

struct RasterLayer {
    name: String,
    width: usize,
    height: usize,
    left: i32,
    top: i32,
    shape: Option<crate::core::vector::shape::Shape>,
    pixels: RasterPixels,
    visible: bool,
    opacity: u8,
    blend: BlendMode,
}
struct RasterDocument {
    width: usize,
    height: usize,
    dpi: f32,
    layers: Vec<RasterLayer>,
}

impl RasterLayer {
    fn set_shape(
        &mut self,
        shape: crate::core::vector::shape::Shape,
        w: usize,
        h: usize,
    ) -> Result<(), String> {
        let bounds = shape.bounds();
        let outside = shape.inverted || shape.initial_fill;
        let left = if outside {
            0
        } else {
            (bounds.min.x.floor() as i32).clamp(0, w as i32)
        };
        let top = if outside {
            0
        } else {
            (bounds.min.y.floor() as i32).clamp(0, h as i32)
        };
        let right = if outside {
            w as i32
        } else {
            (bounds.max.x.ceil() as i32).clamp(left, w as i32)
        };
        let bottom = if outside {
            h as i32
        } else {
            (bounds.max.y.ceil() as i32).clamp(top, h as i32)
        };
        self.left = left;
        self.top = top;
        self.width = (right - left) as usize;
        self.height = (bottom - top) as usize;
        self.pixels = (if self.width * self.height == 0 {
            vec![]
        } else {
            shape.rgba(
                self.width as u32,
                self.height as u32,
                tiny_skia::Transform::from_translate(-(left as f32), -(top as f32)),
            )
        })
        .into();
        self.shape = Some(shape);
        Ok(())
    }
}

pub struct OpenedDocument {
    pub project: Project,
    /// Only exact Graphite projects may overwrite their source through Save.
    pub editable_source: bool,
    pub description: String,
}

/// Progress reflects completed import stages, not an estimated time remaining.
#[derive(Clone, Copy, Debug)]
pub struct OpenProgress {
    pub fraction: f32,
    pub stage: &'static str,
}

pub fn open(path: &Path) -> Result<OpenedDocument, String> {
    open_with_progress(path, &mut |_| {})
}

pub fn open_with_progress(
    path: &Path,
    progress: &mut dyn FnMut(OpenProgress),
) -> Result<OpenedDocument, String> {
    progress(OpenProgress {
        fraction: 0.02,
        stage: "Reading file…",
    });
    let extension = path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !OPEN_EXTENSIONS.contains(&extension.as_str()) {
        return Err("Choose a .graphite, .psd, .png, .jpg, .jpeg or .bmp file.".into());
    }
    progress(OpenProgress {
        fraction: 0.08,
        stage: "Decoding image and layers…",
    });
    if extension == "graphite" {
        return Ok(OpenedDocument {
            project: crate::project::load(path)?,
            editable_source: true,
            description: "editable Graphite project".into(),
        });
    }
    if extension == "psd" {
        if let Ok(project) = crate::project::load(path) {
            return Ok(OpenedDocument {
                project,
                editable_source: true,
                description: "editable Photoshop project".into(),
            });
        }
    }
    let name = path.file_stem().and_then(|n| n.to_str()).unwrap_or("Image");
    let raster = if extension == "psd" {
        psd::decode(&std::fs::read(path).map_err(|e| e.to_string())?)?
    } else {
        let file = File::open(path).map_err(|e| e.to_string())?;
        let mut reader = image::ImageReader::new(BufReader::new(file))
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        reader.limits(image::Limits::no_limits());
        let mut decoder = reader
            .into_decoder()
            .map_err(|e| format!("Image could not be read: {e}"))?;
        let (w, h) = decoder.dimensions();
        check_dimensions(w as usize, h as usize)?;
        let orientation = decoder.orientation().map_err(|e| e.to_string())?;
        let mut decoded = image::DynamicImage::from_decoder(decoder)
            .map_err(|e| format!("Image data is incomplete or invalid: {e}"))?;
        decoded.apply_orientation(orientation);
        let rgba = decoded.to_rgba32f();
        let (width, height) = (rgba.width() as usize, rgba.height() as usize);
        RasterDocument {
            width,
            height,
            dpi: 300.,
            layers: vec![RasterLayer {
                name: name.into(),
                width,
                height,
                left: 0,
                top: 0,
                shape: None,
                pixels: rgba.pixels().map(|p| p.0).collect::<Vec<_>>().into(),
                visible: true,
                opacity: 255,
                blend: BlendMode::Normal,
            }],
        }
    };
    let count = raster.layers.len();
    let project = from_layers(name, raster, progress)?;
    Ok(OpenedDocument {
        project,
        editable_source: false,
        description: if extension == "psd" {
            format!("PSD with {count} separate image layer(s)")
        } else {
            format!("{} image", extension.to_uppercase())
        },
    })
}

fn check_dimensions(width: usize, height: usize) -> Result<(), String> {
    crate::limits::canvas_pixels(width, height).map(|_| ())
}

fn from_layers(
    name: &str,
    raster: RasterDocument,
    progress: &mut dyn FnMut(OpenProgress),
) -> Result<Project, String> {
    progress(OpenProgress {
        fraction: 0.35,
        stage: "Preparing drawing…",
    });
    use crate::core::{document::DrawingLayer, material::SparseDepositState};
    let RasterDocument {
        width,
        height,
        dpi,
        layers,
    } = raster;
    check_dimensions(width, height)?;
    if layers.is_empty() {
        return Err("A drawing must contain at least one layer.".into());
    }
    let dpi = if dpi.is_finite() && (1.0..=2400.).contains(&dpi) {
        dpi
    } else {
        300.
    };
    let spec = CanvasSpec {
        name: name.into(),
        width_px: width,
        height_px: height,
        width_mm: width as f32 * 25.4 / dpi,
        height_mm: height as f32 * 25.4 / dpi,
        dpi,
    };
    let mut document = Document::new(
        spec,
        PaperPreset::DrawingMedium,
        "White",
        vec![[1.; 3]; width * height],
    );
    document.paper_texture_opacity = 0.;
    document.layers.clear();
    document.active_layer = layers
        .iter()
        .rposition(|l| l.visible)
        .unwrap_or(layers.len() - 1);
    document.next_layer_id = layers.len() as u64 + 1;
    let total_pixels = layers.iter().map(|l| l.pixels.len()).sum::<usize>().max(1);
    let mut completed_pixels = 0;
    for (layer_index, layer) in layers.into_iter().enumerate() {
        if layer.width.checked_mul(layer.height) != Some(layer.pixels.len()) {
            return Err("Layer dimensions do not match its pixels.".into());
        }
        let layer_pixels = layer.pixels.len();
        let mut deposit = SparseDepositState::new(width, height);
        for (i, pixel) in layer.pixels.into_iter().enumerate() {
            if i % 65536 == 0 {
                progress(OpenProgress {
                    fraction: 0.45 + 0.4 * (completed_pixels + i) as f32 / total_pixels as f32,
                    stage: "Preparing layers…",
                });
            }
            let x = layer.left as i64 + (i % layer.width) as i64;
            let y = layer.top as i64 + (i / layer.width) as i64;
            if x < 0 || y < 0 || x >= width as i64 || y >= height as i64 {
                continue;
            }
            let index = y as usize * width + x as usize;
            if pixel.iter().any(|v| !v.is_finite()) {
                return Err("Image contains non-finite color values.".into());
            }
            let alpha = pixel[3].clamp(0., 1.);
            if alpha == 0. {
                continue;
            }
            let state = PixelDepositState::from_rgba(pixel);
            if layer_index == document.active_layer {
                document.surface.set_deposit_pixel(index, state);
            } else {
                deposit.set(index, state);
            }
        }
        completed_pixels += layer_pixels;
        let shape = layer.shape;
        document.layers.push(DrawingLayer {
            id: layer_index as u64 + 1,
            name: layer.name,
            visible: layer.visible,
            opacity: layer.opacity,
            blend_mode: layer.blend,
            deposit,
            vectors: Default::default(),
        });
        if let Some(shape) = shape {
            let settings = ToolSettings::default();
            let points = shape
                .paths
                .iter()
                .flat_map(|p| &p.knots)
                .map(|k| crate::core::stroke::StrokePoint {
                    x: k.anchor.x,
                    y: k.anchor.y,
                    pressure: 1.,
                    tilt_deg: 0.,
                    azimuth_deg: 0.,
                    rotation_deg: None,
                })
                .collect();
            let stroke = crate::core::vector::VectorStroke {
                label: document.layers[layer_index].name.clone(),
                shape: Some(shape),
                settings,
                tip: PencilTipState::default(),
                engine: StrokeEngine::default(),
                points,
                polyline: true,
                selection: Default::default(),
            };
            std::sync::Arc::make_mut(&mut document.layers[layer_index].vectors)
                .strokes
                .push(std::sync::Arc::new(stroke));
        }
    }
    let settings = ToolSettings::default();
    let tip = PencilTipState::fresh(settings.pencil_core_diameter_mm);
    Ok(Project {
        document,
        settings,
        tip,
        engine: StrokeEngine::default(),
        brushes: vec![],
        custom_paper: None,
        zoom: 1.,
        pan: [0.; 2],
    })
}

#[cfg(test)]
mod dimension_tests {
    use super::*;
    #[test]
    fn temporary_layer_storage_compacts_without_quantizing_samples() {
        let bytes: Vec<_> = (0..=255).map(|n| [n as f32 / 255.; 4]).collect();
        let packed = RasterPixels::from(bytes.clone());
        assert!(matches!(packed, RasterPixels::Bytes(_)));
        assert_eq!(packed.into_iter().collect::<Vec<_>>(), bytes);
        for sample in [0.1234567, 0.5, -0.01, 1.01, f32::NAN] {
            assert!(matches!(
                RasterPixels::from(vec![[sample; 4]]),
                RasterPixels::Float(_)
            ));
        }
    }
    #[test]
    fn standard_camera_dimensions_fit_and_invalid_sizes_still_fail() {
        assert!(check_dimensions(4032, 3024).is_ok());
        assert!(check_dimensions(4000, 4000).is_ok());
        assert!(check_dimensions(4001, 4000).is_ok());
        assert!(check_dimensions(12000, 12000).is_ok());
        assert!(check_dimensions(0, 3024).is_err());
        assert!(check_dimensions(usize::MAX, 2).is_err());
    }
}
