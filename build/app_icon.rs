// Source-native icon, using the same pencil geometry/colors as the toolbar.
// Supersample each Windows icon size independently for crisp small taskbar icons.
fn inside(x: f32, y: f32, points: &[(f32, f32)]) -> bool {
    let mut yes = false;
    let mut previous = points[points.len() - 1];
    for &p in points {
        if (p.1 > y) != (previous.1 > y)
            && x < (previous.0 - p.0) * (y - p.1) / (previous.1 - p.1) + p.0
        {
            yes = !yes;
        }
        previous = p;
    }
    yes
}
fn sample(x: f32, y: f32) -> [u8; 4] {
    let qx = (x - 16.).abs() - 10.;
    let qy = (y - 16.).abs() - 10.;
    let distance = qx.max(0.).hypot(qy.max(0.)) + qx.max(qy).min(0.) - 4.;
    if distance > 0. {
        return [0; 4];
    }
    let mut color = if distance > -0.8 {
        [88, 97, 104, 255]
    } else {
        [245, 243, 236, 255]
    };
    // Leave padding inside the paper tile.
    let x = (x - 3.) / 0.81;
    let y = (y - 3.) / 0.81;
    for (polygon, fill) in [
        (
            &[(6., 21.), (22., 5.), (28., 11.), (12., 27.)][..],
            [214, 153, 40, 255],
        ),
        (
            &[(8., 22.), (23., 7.), (25., 9.), (10., 24.)][..],
            [255, 215, 99, 255],
        ),
        (
            &[(6., 21.), (12., 27.), (3., 30.)][..],
            [211, 171, 124, 255],
        ),
        (&[(4., 26.), (7., 29.), (3., 30.)][..], [46, 49, 52, 255]),
        (
            &[(22., 5.), (25., 2.), (31., 8.), (28., 11.)][..],
            [203, 111, 122, 255],
        ),
        (
            &[(20., 7.), (22., 5.), (28., 11.), (26., 13.)][..],
            [132, 139, 144, 255],
        ),
    ] {
        if inside(x, y, polygon) {
            color = fill;
        }
    }
    color
}
pub fn render(size: usize) -> Vec<u8> {
    let mut pixels = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let mut accum = [0u32; 4];
            for sy in 0..4 {
                for sx in 0..4 {
                    let c = sample(
                        (x as f32 + (sx as f32 + 0.5) / 4.) * 32. / size as f32,
                        (y as f32 + (sy as f32 + 0.5) / 4.) * 32. / size as f32,
                    );
                    for k in 0..3 {
                        accum[k] += c[k] as u32 * c[3] as u32;
                    }
                    accum[3] += c[3] as u32;
                }
            }
            for k in 0..3 {
                pixels.push((accum[k] / accum[3].max(1)) as u8);
            }
            pixels.push((accum[3] / 16) as u8);
        }
    }
    pixels
}
pub fn ico() -> Vec<u8> {
    let sizes = [16usize, 24, 32, 48, 64, 128, 256];
    let mut headers = vec![0, 0, 1, 0, sizes.len() as u8, 0];
    let mut images = Vec::new();
    for size in sizes {
        let rgba = render(size);
        let mask_stride = size.div_ceil(32) * 4;
        let mut dib = Vec::new();
        dib.extend(40u32.to_le_bytes());
        dib.extend((size as i32).to_le_bytes());
        dib.extend((size as i32 * 2).to_le_bytes());
        dib.extend(1u16.to_le_bytes());
        dib.extend(32u16.to_le_bytes());
        dib.extend(0u32.to_le_bytes());
        dib.extend(((size * 4 + mask_stride) * size as usize).to_le_bytes()[..4].iter());
        dib.extend([0u8; 16]);
        for y in (0..size).rev() {
            for x in 0..size {
                let c = &rgba[(y * size + x) * 4..][..4];
                dib.extend([c[2], c[1], c[0], c[3]]);
            }
        }
        for y in (0..size).rev() {
            let mut mask = vec![0u8; mask_stride];
            for x in 0..size {
                if rgba[(y * size + x) * 4 + 3] == 0 {
                    mask[x / 8] |= 0x80 >> (x % 8);
                }
            }
            dib.extend(mask);
        }
        headers.extend([size as u8, size as u8, 0, 0]);
        headers.extend(1u16.to_le_bytes());
        headers.extend(32u16.to_le_bytes());
        headers.extend((dib.len() as u32).to_le_bytes());
        headers.extend(((6 + sizes.len() * 16 + images.len()) as u32).to_le_bytes());
        images.extend(dib);
    }
    headers.extend(images);
    headers
}
