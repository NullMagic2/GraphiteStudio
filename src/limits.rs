//! Structural size validation; there are no application pixel quotas.
pub fn canvas_pixels(width: usize, height: usize) -> Result<usize, String> {
    let pixels = width
        .checked_mul(height)
        .ok_or("Image dimensions overflow")?;
    if width == 0
        || height == 0
        || pixels
            .checked_mul(std::mem::size_of::<[f32; 4]>())
            .is_none_or(|bytes| bytes > isize::MAX as usize)
    {
        return Err("Image dimensions are zero or cannot be represented in memory.".into());
    }
    Ok(pixels)
}
