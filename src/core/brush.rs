//! Bounded ABR sampled-tip reader. Format layout checked against GIMP's ABR
//! loader; independently implemented. Photoshop descriptors/dynamics are not imported.
use std::sync::Arc;

/// A sampled contacting face, evaluated along the continuous stroke and passed
/// to the same paper/material solver as the ordinary graphite tip.
pub(crate) struct ImportedContact<'a> {
    texture: &'a BrushTip,
    half_w: f32,
    half_h: f32,
    sin: f32,
    cos: f32,
}
impl<'a> ImportedContact<'a> {
    pub fn new(
        texture: &'a BrushTip,
        diameter: f32,
        pressure: f32,
        tilt: f32,
        angle: f32,
        sharpness: f32,
    ) -> Self {
        let load = super::contact::pencil_effective_pressure(pressure);
        let size = diameter * (0.35 + 0.65 * load) * (1.1 - 0.25 * sharpness.clamp(0., 1.));
        let aspect = texture.width.max(texture.height) as f32;
        let side = (tilt.clamp(0., 84.) / 84.).powi(2);
        let (sin, cos) = angle.to_radians().sin_cos();
        Self {
            texture,
            half_w: (size * texture.width as f32 / aspect * 0.5).max(0.25),
            half_h: (size * texture.height as f32 / aspect * 0.5 * (1. + 0.8 * side)).max(0.25),
            sin,
            cos,
        }
    }
    pub fn reach(&self) -> f32 {
        self.half_w.hypot(self.half_h) + 1.
    }
    fn local(&self, x: f32, y: f32) -> (f32, f32) {
        (x * self.cos + y * self.sin, -x * self.sin + y * self.cos)
    }
    pub fn may_cover_box(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> bool {
        let mut min = (f32::INFINITY, f32::INFINITY);
        let mut max = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        for (x, y) in [(x0, y0), (x0, y1), (x1, y0), (x1, y1)] {
            let (u, v) = self.local(x, y);
            min.0 = min.0.min(u);
            min.1 = min.1.min(v);
            max.0 = max.0.max(u);
            max.1 = max.1.max(v);
        }
        min.0 <= self.half_w + 1.
            && max.0 >= -self.half_w - 1.
            && min.1 <= self.half_h + 1.
            && max.1 >= -self.half_h - 1.
    }
    #[cfg(test)]
    pub fn sample(
        &self,
        x: f32,
        y: f32,
        profile: &super::pencil::TipProfile,
    ) -> super::contact::TipContactSample {
        self.sample_prepared(
            x,
            y,
            profile,
            (-profile.orientation_deg.to_radians()).sin_cos(),
        )
    }
    pub fn sample_prepared(
        &self,
        x: f32,
        y: f32,
        profile: &super::pencil::TipProfile,
        rotation: (f32, f32),
    ) -> super::contact::TipContactSample {
        let (lx, ly) = self.local(x, y);
        if lx.abs() > self.half_w + 1. || ly.abs() > self.half_h + 1. {
            return Default::default();
        }
        // Integrate four subpixel contacts instead of hard-thresholding the mask.
        // Zero samples remain empty; aspect ratio and disconnected islands survive.
        let mut coverage = 0.;
        for (ox, oy) in [(-0.25, -0.25), (0.25, -0.25), (-0.25, 0.25), (0.25, 0.25)] {
            let (u, v) = self.local(x + ox, y + oy);
            coverage += self
                .texture
                .sample(u / (2. * self.half_w) + 0.5, v / (2. * self.half_h) + 0.5)
                * 0.25;
        }
        if coverage <= 1e-5 {
            return Default::default();
        }
        let u = lx / self.half_w;
        let v = ly / self.half_h;
        let wear = (profile.sample_height_prepared(u, v, rotation)
            / profile.peak_height().max(1e-5))
        .clamp(0., 1.);
        super::contact::TipContactSample {
            coverage,
            pressure_weight: (0.65 + 0.45 * coverage.sqrt()) * (0.7 + 0.3 * wear),
            signed_distance: -1.,
            cross_coordinate_px: ly,
            axial_coordinate_px: lx,
            profile_u: u,
            profile_v: v,
            profile_contact: (0.65 + 0.35 * wear) * coverage.sqrt(),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct BrushTip {
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub mask: Vec<u8>,
}
impl BrushTip {
    /// Interpret the sample as the contacting face's relief, rather than paste
    /// its grayscale pixels onto paper. Low relief leaves a light graphite film;
    /// the physical tip silhouette and paper grain still determine coverage.
    pub fn contact_relief(&self, u: f32, v: f32, rotation: (f32, f32)) -> f32 {
        let (sin, cos) = rotation;
        let x = u * cos - v * sin;
        let y = u * sin + v * cos;
        let height = self.sample(x * 0.5 + 0.5, y * 0.5 + 0.5);
        0.18 + 0.82 * height.sqrt()
    }

    pub fn sample(&self, u: f32, v: f32) -> f32 {
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return 0.;
        }
        let x = u * (self.width - 1) as f32;
        let y = v * (self.height - 1) as f32;
        let (ix, iy) = (x as usize, y as usize);
        let (fx, fy) = (x.fract(), y.fract());
        let p = |x: usize, y: usize| {
            self.mask[y.min(self.height - 1) * self.width + x.min(self.width - 1)] as f32 / 255.
        };
        (p(ix, iy) * (1. - fx) + p(ix + 1, iy) * fx) * (1. - fy)
            + (p(ix, iy + 1) * (1. - fx) + p(ix + 1, iy + 1) * fx) * fy
    }
}
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}
type Result<T> = std::result::Result<T, String>;
impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or("ABR size overflow")?;
        let data = self.bytes.get(self.pos..end).ok_or("Truncated ABR data")?;
        self.pos = end;
        Ok(data)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }
    fn left(&self) -> usize {
        self.bytes.len() - self.pos
    }
}
fn sampled(r: &mut Reader<'_>, name: String, budget: &mut usize) -> Result<Arc<BrushTip>> {
    let (top, left, bottom, right) = (
        r.i32()? as i64,
        r.i32()? as i64,
        r.i32()? as i64,
        r.i32()? as i64,
    );
    let (w, h) = (right - left, bottom - top);
    if w <= 0 || h <= 0 || w > 8192 || h > 8192 || w * h > 16_777_216 {
        return Err("Brush dimensions exceed the supported limit".into());
    }
    if r.u16()? != 8 {
        return Err("This ABR tip is not 8-bit; use an 8-bit sampled brush".into());
    }
    let (w, h) = (w as usize, h as usize);
    let n = w * h;
    if n > *budget {
        return Err("ABR brush library exceeds the 64 MB decoded limit".into());
    }
    *budget -= n;
    let compression = r.u8()?;
    let mask = match compression {
        0 => r.take(n)?.to_vec(),
        1 => {
            let lengths = (0..h)
                .map(|_| r.u16().map(usize::from))
                .collect::<Result<Vec<_>>>()?;
            let mut pixels = Vec::with_capacity(n);
            for len in lengths {
                let mut row = Reader::new(r.take(len)?);
                let start = pixels.len();
                while row.left() > 0 {
                    let code = row.u8()? as i8;
                    match code {
                        0..=127 => {
                            let count = code as usize + 1;
                            if pixels.len() - start + count > w {
                                return Err("Invalid ABR RLE row".into());
                            }
                            pixels.extend_from_slice(row.take(count)?);
                        }
                        -127..=-1 => {
                            let count = (1 - code as i16) as usize;
                            if pixels.len() - start + count > w {
                                return Err("Invalid ABR RLE run".into());
                            }
                            let v = row.u8()?;
                            pixels.resize(pixels.len() + count, v);
                        }
                        -128 => {}
                    }
                }
                if pixels.len() - start != w {
                    return Err("Incomplete ABR RLE row".into());
                }
            }
            pixels
        }
        _ => return Err("Unsupported ABR compression".into()),
    };
    Ok(Arc::new(BrushTip {
        name,
        width: w,
        height: h,
        mask,
    }))
}
pub fn import_abr(bytes: &[u8], label: &str) -> Result<Vec<Arc<BrushTip>>> {
    if bytes.len() > 128 * 1024 * 1024 {
        return Err("ABR file exceeds 128 MB".into());
    }
    let mut r = Reader::new(bytes);
    let version = r.u16()?;
    let subtype = r.u16()?;
    let mut tips = Vec::new();
    let mut budget = 64 * 1024 * 1024;
    match version {
        1|2=>{
            for i in 0..subtype {
                let kind=r.u16()?; let len=r.u32()? as usize; let mut record=Reader::new(r.take(len)?);
                if kind!=2 {continue;}
                record.take(6)?;
                let mut name=format!("{label} · {}",i+1);
                if version==2 {
                    let count=record.u32()? as usize;
                    if count>4096 {return Err("ABR name is too long".into());}
                    let utf=(0..count).map(|_|record.u16()).collect::<Result<Vec<_>>>()?;
                    let text=String::from_utf16_lossy(&utf).trim_end_matches('\0').to_owned();
                    if !text.is_empty(){name=text;}
                }
                record.take(9)?;
                tips.push(sampled(&mut record,name,&mut budget)?);
                if tips.len()>512{return Err("ABR has more than 512 sampled tips".into());}
            }
        },
        6|10 if subtype==1 || subtype==2=>{
            while r.left()>0 {
                if r.take(4)?!=b"8BIM" {return Err("Invalid ABR section signature".into());}
                let tag=r.take(4)?; let len=r.u32()? as usize; let block=r.take(len)?;
                // Modern resource blocks are padded to a four-byte boundary,
                // including descriptors and patterns that we do not decode.
                r.take((4-len%4)%4)?;
                if tag!=b"samp" {continue;}
                let mut samples=Reader::new(block);
                while samples.left()>0 {
                    let size=samples.u32()? as usize;
                    let mut record=Reader::new(samples.take(size)?);
                    record.take(if subtype==1 {47} else {301})?;
                    tips.push(sampled(&mut record,format!("{label} · {}",tips.len()+1),&mut budget)?);
                    if tips.len()>512{return Err("ABR has more than 512 sampled tips".into());}
                    samples.take((4-size%4)%4)?;
                }
            }
        },
        _=>return Err(format!("Unsupported ABR version {version}.{subtype}. Supported sampled tips: v1, v2, v6 and v10.")),
    }
    if tips.is_empty() {
        return Err(
            "No supported sampled tips found. Computed Photoshop brushes are not imported.".into(),
        );
    }
    Ok(tips)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imported_contact_preserves_aspect_rotation_and_antialiases_edges() {
        let texture = BrushTip {
            name: "Bar".into(),
            width: 80,
            height: 20,
            mask: vec![255; 1600],
        };
        let profile = super::super::pencil::PencilTipState::fresh(3.).profile;
        let a = ImportedContact::new(&texture, 200., 1., 0., 0., 0.5);
        let b = ImportedContact::new(&texture, 200., 1., 0., 90., 0.5);
        assert!(a.sample(70., 0., &profile).coverage > 0.9);
        assert_eq!(a.sample(0., 70., &profile).coverage, 0.);
        assert_eq!(b.sample(70., 0., &profile).coverage, 0.);
        assert!(b.sample(0., 70., &profile).coverage > 0.9);
        let edge = a.sample(a.half_w, 0., &profile).coverage;
        assert!(edge > 0. && edge < 1.);
        for y in -120..120 {
            for x in -120..120 {
                if b.sample(x as f32, y as f32, &profile).coverage > 0. {
                    assert!(b.may_cover_box(
                        x as f32 - 4.,
                        y as f32 - 4.,
                        x as f32 + 4.,
                        y as f32 + 4.
                    ));
                }
            }
        }
    }
    fn pixels(rle: bool) -> Vec<u8> {
        let mut b = Vec::new();
        for v in [0i32, 0, 2, 3] {
            b.extend(v.to_be_bytes());
        }
        b.extend(8u16.to_be_bytes());
        b.push(rle as u8);
        if rle {
            b.extend(4u16.to_be_bytes());
            b.extend(2u16.to_be_bytes());
            b.extend([2, 0, 128, 255, 254, 90]);
        } else {
            b.extend([0, 128, 255, 90, 90, 90]);
        }
        b
    }
    fn fixture(version: u16, sub: u16, rle: bool) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(version.to_be_bytes());
        bytes.extend(sub.to_be_bytes());
        if version <= 2 {
            let mut record = vec![0; 6];
            if version == 2 {
                record.extend(4u32.to_be_bytes());
                for c in "Test".encode_utf16() {
                    record.extend(c.to_be_bytes());
                }
            }
            record.extend([0; 9]);
            record.extend(pixels(rle));
            bytes.extend(2u16.to_be_bytes());
            bytes.extend((record.len() as u32).to_be_bytes());
            bytes.extend(record);
        } else {
            let mut record = vec![0; if sub == 1 { 47 } else { 301 }];
            record.extend(pixels(rle));
            let len = record.len();
            let mut block = Vec::new();
            block.extend((len as u32).to_be_bytes());
            block.extend(record);
            block.resize(block.len() + (4 - len % 4) % 4, 0);
            bytes.extend(b"8BIMsamp");
            bytes.extend((block.len() as u32).to_be_bytes());
            bytes.extend(block);
        }
        bytes
    }
    #[test]
    fn reads_legacy_and_modern_raw_and_packbits_tips() {
        for (version, sub) in [(1, 1), (2, 1), (6, 1), (6, 2), (10, 1), (10, 2)] {
            for rle in [false, true] {
                let tips = import_abr(&fixture(version, sub, rle), "Pack").unwrap();
                assert_eq!(tips.len(), 1);
                assert_eq!(tips[0].mask, [0, 128, 255, 90, 90, 90]);
                assert_eq!((tips[0].width, tips[0].height), (3, 2));
            }
        }
    }
    #[test]
    fn modern_sections_skip_padding_before_and_after_samples() {
        for length in [1usize, 2, 3, 4, 43] {
            let mut bytes = vec![0, 10, 0, 2];
            let append = |bytes: &mut Vec<u8>, tag: &[u8], body: &[u8]| {
                bytes.extend(b"8BIM");
                bytes.extend(tag);
                bytes.extend((body.len() as u32).to_be_bytes());
                bytes.extend(body);
                bytes.resize(bytes.len() + (4 - body.len() % 4) % 4, 0);
            };
            append(&mut bytes, b"desc", &vec![1; length]);
            bytes.extend(&fixture(10, 2, true)[4..]);
            append(&mut bytes, b"phry", &vec![2; length]);
            assert_eq!(
                import_abr(&bytes, "Collection").unwrap()[0].mask,
                [0, 128, 255, 90, 90, 90]
            );
            if length % 4 != 0 {
                bytes.pop();
                assert!(import_abr(&bytes, "Collection").is_err());
            }
        }
    }
    #[test]
    fn rejects_truncation_unsupported_versions_and_overlong_rle() {
        let b = fixture(2, 1, true);
        for len in 0..b.len() {
            assert!(import_abr(&b[..len], "Bad").is_err(), "{len}");
        }
        let mut bad = b.clone();
        let n = bad.len();
        bad[n - 2] = 253;
        assert!(import_abr(&bad, "Bad").is_err());
        assert!(import_abr(&[0, 99, 0, 1], "Bad").is_err());
        let mut huge = fixture(2, 1, false);
        let bounds = 4 + 6 + 6 + 4 + 8 + 9;
        huge[bounds + 12..bounds + 16].copy_from_slice(&i32::MAX.to_be_bytes());
        assert!(import_abr(&huge, "Bad").is_err());
    }
    #[test]
    fn sampled_tip_preserves_rectangular_corners_and_shades() {
        let t = BrushTip {
            name: "Square".into(),
            width: 2,
            height: 2,
            mask: vec![255, 255, 255, 255],
        };
        assert_eq!(t.sample(0., 0.), 1.);
        assert_eq!(t.sample(-0.01, 0.5), 0.);
        let t = import_abr(&fixture(2, 1, false), "Test").unwrap().remove(0);
        assert!((t.sample(0.5, 0.) - 128. / 255.).abs() < 1e-6);
    }
}
