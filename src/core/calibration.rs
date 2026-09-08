use super::pencil::PencilGrade;

#[derive(Debug, Clone, Copy)]
pub struct CalibrationCase {
    pub grade: PencilGrade,
    pub core_diameter_mm: f32,
    pub pressure: f32,
    pub tilt_deg: f32,
    pub pass_count: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CalibrationMetrics {
    pub mean_darkness: f32,
    pub occupied_area_px: usize,
    pub edge_roughness_px: f32,
    pub tone_variance: f32,
    pub contact_width_px: f32,
    pub taper_length_px: f32,
    pub final_tip_wear: f32,
}

#[derive(Debug, Default)]
pub struct CalibrationReport {
    pub rows: Vec<(CalibrationCase, CalibrationMetrics)>,
}
