use super::stroke::StrokePoint;
use eframe::egui::Vec2;

/// Distance-based stabilization. Integrating each linear input segment avoids
/// changing the filter's strength when a driver sends more samples.
pub struct LineSmoother {
    raw: StrokePoint,
    filtered: Vec2,
    distance: f32,
}
impl LineSmoother {
    pub fn new(start: StrokePoint, amount: f32, zoom: f32) -> Self {
        let amount = amount.clamp(0., 1.);
        Self {
            raw: start,
            filtered: Vec2::new(start.x, start.y),
            distance: 32. * amount * amount / zoom.max(0.03),
        }
    }
    pub fn raw(&self) -> StrokePoint {
        self.raw
    }
    pub fn push(&mut self, point: StrokePoint) -> StrokePoint {
        let next = Vec2::new(point.x, point.y);
        let previous = Vec2::new(self.raw.x, self.raw.y);
        let delta = next - previous;
        let length = delta.length();
        if self.distance <= 0.0001 {
            self.filtered = next;
        } else if length > 0.000001 {
            let lag = delta / length * self.distance;
            self.filtered =
                next - lag + (self.filtered - previous + lag) * (-length / self.distance).exp();
        }
        self.raw = point;
        // Pressure and orientation remain measured; only the path is stabilized.
        StrokePoint {
            x: self.filtered.x,
            y: self.filtered.y,
            ..point
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn point(x: f32, y: f32) -> StrokePoint {
        StrokePoint {
            x,
            y,
            pressure: 0.45,
            tilt_deg: 40.,
            azimuth_deg: 70.,
            rotation_deg: Some(25.),
        }
    }
    #[test]
    fn smoothing_reduces_wobble_and_zero_preserves_positions_and_sensors() {
        let mut off = LineSmoother::new(point(0., 0.), 0., 1.);
        let mut smooth = LineSmoother::new(point(0., 0.), 0.8, 1.);
        let mut wobble = 0.;
        for i in 1..=100 {
            let raw = point(i as f32 * 2., if i % 2 == 0 { 4. } else { -4. });
            let exact = off.push(raw);
            assert_eq!((exact.x, exact.y), (raw.x, raw.y));
            let filtered = smooth.push(raw);
            wobble += filtered.y.abs();
            assert_eq!(filtered.pressure, raw.pressure);
            assert_eq!(filtered.tilt_deg, raw.tilt_deg);
            assert_eq!(filtered.azimuth_deg, raw.azimuth_deg);
            assert_eq!(filtered.rotation_deg, raw.rotation_deg);
        }
        assert!(wobble / 100. < 1.);
    }
    #[test]
    fn smoothing_is_consistent_across_sample_density_and_zoom() {
        let mut sparse = LineSmoother::new(point(0., 0.), 0.7, 1.);
        let mut dense = LineSmoother::new(point(0., 0.), 0.7, 1.);
        let a = sparse.push(point(100., 0.));
        let mut b = point(0., 0.);
        for i in 1..=100 {
            b = dense.push(point(i as f32, 0.));
        }
        assert!((a.x - b.x).abs() < 0.001);
        let mut zoomed = LineSmoother::new(point(0., 0.), 0.7, 2.);
        assert!((zoomed.push(point(50., 0.)).x * 2. - a.x).abs() < 0.001);
        assert_eq!(sparse.push(point(100., 0.)).x, a.x);
    }
}
