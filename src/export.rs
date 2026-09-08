use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

use image::{codecs::jpeg::JpegEncoder, ExtendedColorType, ImageEncoder};
mod psd_layers;
mod psd_vectors;

use crate::{core::document::Document, render::DocumentRenderer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveFormat {
    Png,
    Bmp,
    Jpeg,
    Psd,
}

impl SaveFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "png" => Some(Self::Png),
            "bmp" => Some(Self::Bmp),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "psd" => Some(Self::Psd),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Bmp => "BMP",
            Self::Jpeg => "JPEG",
            Self::Psd => "PSD",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PsdBitDepth {
    Eight,
    Sixteen,
}

impl PsdBitDepth {
    pub const ALL: [Self; 2] = [Self::Eight, Self::Sixteen];

    pub fn bits(self) -> u16 {
        match self {
            Self::Eight => 8,
            Self::Sixteen => 16,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Eight => "PSD 8-bit",
            Self::Sixteen => "PSD 16-bit",
        }
    }
}

impl Default for PsdBitDepth {
    fn default() -> Self {
        Self::Sixteen
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SaveOptions {
    pub psd_bit_depth: PsdBitDepth,
}

impl Default for SaveOptions {
    fn default() -> Self {
        Self {
            psd_bit_depth: PsdBitDepth::Sixteen,
        }
    }
}

/// Save the visible optical rendering of a physical Graphite Studio document.
///
/// Export is intentionally downstream of the material simulation: none of the file encoders know
/// how graphite is deposited, and changing output format cannot alter the document state.
pub fn save_rendered_document<R: DocumentRenderer>(
    renderer: &mut R,
    document: &Document,
    path: &Path,
    options: SaveOptions,
) -> Result<SaveFormat, String> {
    let format = SaveFormat::from_path(path)
        .ok_or_else(|| "unsupported extension; use PNG, BMP, JPEG or PSD".to_owned())?;

    let width = document.spec.width_px as u32;
    let height = document.spec.height_px as u32;

    match format {
        SaveFormat::Png => {
            let rgba = renderer.rgba8(document);
            image::save_buffer_with_format(
                path,
                &rgba,
                width,
                height,
                image::ColorType::Rgba8,
                image::ImageFormat::Png,
            )
            .map_err(|error| error.to_string())?;
        }
        SaveFormat::Bmp => {
            let rgba = renderer.rgba8(document);
            image::save_buffer_with_format(
                path,
                &rgba,
                width,
                height,
                image::ColorType::Rgba8,
                image::ImageFormat::Bmp,
            )
            .map_err(|error| error.to_string())?;
        }
        SaveFormat::Jpeg => {
            let rgba = renderer.rgba8(document);
            let rgb = rgba_to_rgb(&rgba);
            let file = File::create(path).map_err(|error| error.to_string())?;
            let encoder = JpegEncoder::new_with_quality(BufWriter::new(file), 95);
            encoder
                .write_image(&rgb, width, height, ExtendedColorType::Rgb8)
                .map_err(|error| error.to_string())?;
        }
        SaveFormat::Psd => {
            let file = File::create(path).map_err(|error| error.to_string())?;
            let mut writer = BufWriter::new(file);
            psd_layers::write_layered_psd(
                &mut writer,
                renderer,
                document,
                options.psd_bit_depth,
                None,
            )
            .map_err(|error| error.to_string())?;
            writer.flush().map_err(|error| error.to_string())?;
        }
    }

    Ok(format)
}

fn rgba_to_rgb(rgba: &[u8]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
    for pixel in rgba.chunks_exact(4) {
        // The current paper renderer is opaque. Keeping this conversion explicit makes JPEG/PSD
        // behavior deterministic if the display texture later gains an alpha channel.
        let alpha = pixel[3] as u16;
        let inv_alpha = 255_u16.saturating_sub(alpha);
        for &channel in &pixel[..3] {
            let composited = (channel as u16 * alpha + 255 * inv_alpha + 127) / 255;
            rgb.push(composited as u8);
        }
    }
    rgb
}

/// Write a baseline Photoshop PSD (version 1), 8-bit RGB, with a merged image and resolution
/// metadata. Graphite Studio's internal drawing-layer stack is composited by the renderer first;
/// encoding native PSD layer records is a separate exporter milestone.
#[cfg(test)]
fn write_flat_rgb_psd8<W: Write>(
    writer: &mut W,
    rgb: &[u8],
    width: u32,
    height: u32,
    dpi: f32,
) -> std::io::Result<()> {
    let expected = width as usize * height as usize * 3;
    if rgb.len() != expected {
        return Err(invalid_input(
            "RGB byte count does not match PSD dimensions",
        ));
    }
    write_psd_header_and_sections(writer, width, height, dpi, 8)?;

    // Composite image data. Compression 0 means raw planar channel data (R plane, G plane, B).
    be_u16(writer, 0)?;
    let mut row = vec![0_u8; width as usize];
    for channel in 0..3 {
        for y in 0..height as usize {
            let row_start = y * width as usize;
            for x in 0..width as usize {
                row[x] = rgb[(row_start + x) * 3 + channel];
            }
            writer.write_all(&row)?;
        }
    }
    Ok(())
}

/// Write a baseline Photoshop PSD (version 1), true 16-bit/channel RGB, with a merged image and
/// resolution metadata. Samples are emitted as big-endian planar 16-bit values as required by the
/// PSD raw image-data representation.
#[cfg(test)]
fn write_flat_rgb_psd16<W: Write>(
    writer: &mut W,
    rgb: &[u16],
    width: u32,
    height: u32,
    dpi: f32,
) -> std::io::Result<()> {
    let expected = width as usize * height as usize * 3;
    if rgb.len() != expected {
        return Err(invalid_input(
            "RGB sample count does not match PSD dimensions",
        ));
    }
    write_psd_header_and_sections(writer, width, height, dpi, 16)?;

    be_u16(writer, 0)?; // raw/uncompressed
    for channel in 0..3 {
        for y in 0..height as usize {
            let row_start = y * width as usize;
            for x in 0..width as usize {
                be_u16(writer, rgb[(row_start + x) * 3 + channel])?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
fn write_psd_header_and_sections<W: Write>(
    writer: &mut W,
    width: u32,
    height: u32,
    dpi: f32,
    depth: u16,
) -> std::io::Result<()> {
    write_psd_header_with_resources(writer, width, height, dpi, depth, &[])
}
fn write_psd_header_with_resources<W: Write>(
    writer: &mut W,
    width: u32,
    height: u32,
    dpi: f32,
    depth: u16,
    resources: &[u8],
) -> std::io::Result<()> {
    if width == 0 || height == 0 || width > 30_000 || height > 30_000 {
        return Err(invalid_input(
            "PSD version 1 requires dimensions from 1 to 30000 pixels",
        ));
    }
    if depth != 8 && depth != 16 {
        return Err(invalid_input(
            "Graphite Studio PSD export supports 8 or 16 bits/channel",
        ));
    }

    // File header section.
    writer.write_all(b"8BPS")?;
    be_u16(writer, 1)?; // PSD version 1
    writer.write_all(&[0; 6])?;
    be_u16(writer, 3)?; // RGB channels
    be_u32(writer, height)?;
    be_u32(writer, width)?;
    be_u16(writer, depth)?;
    be_u16(writer, 3)?; // RGB color mode

    // Color mode data section: empty for RGB.
    be_u32(writer, 0)?;

    // Image resources section. Resource 1005 stores horizontal/vertical resolution.
    // Resource size: signature 4 + id 2 + padded empty Pascal name 2 + size 4 + payload 16.
    be_u32(
        writer,
        28u32
            .checked_add(u32::try_from(resources.len()).map_err(|_| invalid_input("PSD resources exceed the format's 4 GB field"))?)
            .ok_or_else(|| invalid_input("PSD resources exceed 4 GB"))?,
    )?;
    writer.write_all(b"8BIM")?;
    be_u16(writer, 1005)?;
    writer.write_all(&[0, 0])?; // empty Pascal string, padded to an even byte count
    be_u32(writer, 16)?;
    let fixed_dpi = (dpi.clamp(1.0, 65_535.0) * 65_536.0).round() as u32;
    be_u32(writer, fixed_dpi)?;
    be_u16(writer, 1)?; // pixels per inch
    be_u16(writer, 1)?; // display width unit: inches
    be_u32(writer, fixed_dpi)?;
    be_u16(writer, 1)?; // pixels per inch
    be_u16(writer, 1)?; // display height unit: inches

    writer.write_all(resources)?;
    // Layer and mask information section: this baseline PSD writer stores only the merged visible
    // composite. Graphite Studio drawing layers remain live in-app but are not encoded here yet.
    be_u32(writer, 0)?;
    Ok(())
}

fn invalid_input(message: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message)
}

fn be_u16<W: Write>(writer: &mut W, value: u16) -> std::io::Result<()> {
    writer.write_all(&value.to_be_bytes())
}

fn be_u32<W: Write>(writer: &mut W, value: u32) -> std::io::Result<()> {
    writer.write_all(&value.to_be_bytes())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{write_flat_rgb_psd16, write_flat_rgb_psd8, PsdBitDepth, SaveFormat};

    #[test]
    fn detects_supported_extensions_case_insensitively() {
        assert_eq!(
            SaveFormat::from_path(Path::new("drawing.PNG")),
            Some(SaveFormat::Png)
        );
        assert_eq!(
            SaveFormat::from_path(Path::new("drawing.bmp")),
            Some(SaveFormat::Bmp)
        );
        assert_eq!(
            SaveFormat::from_path(Path::new("drawing.JpEg")),
            Some(SaveFormat::Jpeg)
        );
        assert_eq!(
            SaveFormat::from_path(Path::new("drawing.psd")),
            Some(SaveFormat::Psd)
        );
        assert_eq!(SaveFormat::from_path(Path::new("drawing.tif")), None);
    }

    #[test]
    fn psd_depth_defaults_to_sixteen_bits() {
        assert_eq!(PsdBitDepth::default(), PsdBitDepth::Sixteen);
    }

    #[test]
    fn writes_8_bit_psd_header_and_resolution_resource() {
        let rgb = vec![255, 0, 0, 0, 255, 0];
        let mut bytes = Vec::new();
        write_flat_rgb_psd8(&mut bytes, &rgb, 2, 1, 300.0).unwrap();

        assert_eq!(&bytes[0..4], b"8BPS");
        assert_eq!(u16::from_be_bytes([bytes[4], bytes[5]]), 1);
        assert_eq!(u16::from_be_bytes([bytes[12], bytes[13]]), 3);
        assert_eq!(u32::from_be_bytes(bytes[14..18].try_into().unwrap()), 1);
        assert_eq!(u32::from_be_bytes(bytes[18..22].try_into().unwrap()), 2);
        assert_eq!(u16::from_be_bytes(bytes[22..24].try_into().unwrap()), 8);
        assert!(bytes.windows(4).any(|window| window == b"8BIM"));
    }

    #[test]
    fn writes_true_16_bit_psd_samples() {
        let rgb = vec![0x1234, 0x5678, 0x9abc];
        let mut bytes = Vec::new();
        write_flat_rgb_psd16(&mut bytes, &rgb, 1, 1, 300.0).unwrap();

        assert_eq!(u16::from_be_bytes(bytes[22..24].try_into().unwrap()), 16);
        // With a 1×1 image and raw compression, the final six bytes are the planar R/G/B samples.
        assert_eq!(
            &bytes[bytes.len() - 6..],
            &[0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc]
        );
    }
}

pub(crate) fn write_editable_psd<W: Write + std::io::Seek, R: DocumentRenderer>(
    writer: &mut W,
    renderer: &mut R,
    doc: &Document,
    depth: PsdBitDepth,
    project: &[u8],
) -> std::io::Result<()> {
    psd_layers::write_layered_psd(writer, renderer, doc, depth, Some(project))
}
