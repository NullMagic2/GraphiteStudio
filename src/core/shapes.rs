use eframe::egui::Vec2;
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeKind {
    Line,
    Circle,
    Triangle,
    Square,
}
impl ShapeKind {
    pub const ALL: [Self; 4] = [Self::Line, Self::Circle, Self::Triangle, Self::Square];
    pub fn label(self) -> &'static str {
        match self {
            Self::Line => "Line",
            Self::Circle => "Circle",
            Self::Triangle => "Triangle",
            Self::Square => "Square",
        }
    }
    pub fn points(self, start: Vec2, end: Vec2) -> Vec<Vec2> {
        let delta = end - start;
        let side = delta.x.abs().max(delta.y.abs());
        let end = start
            + Vec2::new(
                if delta.x < 0. { -side } else { side },
                if delta.y < 0. { -side } else { side },
            );
        let min = start.min(end);
        let max = start.max(end);
        match self {
            Self::Line => vec![start, start + delta],
            Self::Square => vec![
                min,
                Vec2::new(max.x, min.y),
                max,
                Vec2::new(min.x, max.y),
                min,
            ],
            Self::Triangle => {
                let top = Vec2::new((min.x + max.x) * 0.5, min.y);
                vec![top, max, Vec2::new(min.x, max.y), top]
            }
            Self::Circle => {
                let center = (min + max) * 0.5;
                let steps = (side * 0.5).ceil().clamp(32., 256.) as usize;
                (0..=steps)
                    .map(|i| {
                        let a = i as f32 / steps as f32 * std::f32::consts::TAU;
                        center + Vec2::new(a.cos(), a.sin()) * side * 0.5
                    })
                    .collect()
            }
        }
    }

    /// Constrain geometry in paper coordinates so rotating the view does not
    /// change the angle or proportions stored in the drawing.
    pub fn points_with_snap(self, start: Vec2, end: Vec2, snap: bool) -> Vec<Vec2> {
        if !snap {
            return self.points(start, end);
        }
        let delta = end - start;
        match self {
            Self::Line => {
                let step = std::f32::consts::PI / 12.;
                let sector = ((delta.y.atan2(delta.x) / step).round() as i32).rem_euclid(24);
                // Retain exact horizontal, vertical and diagonal vectors.
                let direction = if sector % 3 == 0 {
                    [
                        Vec2::new(1., 0.),
                        Vec2::new(1., 1.),
                        Vec2::new(0., 1.),
                        Vec2::new(-1., 1.),
                        Vec2::new(-1., 0.),
                        Vec2::new(-1., -1.),
                        Vec2::new(0., -1.),
                        Vec2::new(1., -1.),
                    ][(sector / 3) as usize]
                } else {
                    let angle = sector as f32 * step;
                    Vec2::new(angle.cos(), angle.sin())
                };
                let scale = delta.dot(direction) / direction.length_sq();
                vec![start, start + direction * scale]
            }
            Self::Triangle => {
                let side = delta.x.abs().max(delta.y.abs());
                let height = side * 3.0_f32.sqrt() * 0.5;
                let opposite = start
                    + Vec2::new(
                        if delta.x < 0. { -side } else { side },
                        if delta.y < 0. { -height } else { height },
                    );
                let min = start.min(opposite);
                let max = start.max(opposite);
                let top = Vec2::new((min.x + max.x) * 0.5, min.y);
                vec![top, max, Vec2::new(min.x, max.y), top]
            }
            // These tools already constrain both dimensions to the same size.
            Self::Circle | Self::Square => self.points(start, end),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shift_constrains_all_eight_line_directions_and_regular_shapes() {
        let start = Vec2::new(40., 50.);
        for delta in [
            Vec2::new(80., 5.),
            Vec2::new(80., 65.),
            Vec2::new(5., 80.),
            Vec2::new(-65., 80.),
            Vec2::new(-80., 5.),
            Vec2::new(-80., -65.),
            Vec2::new(5., -80.),
            Vec2::new(65., -80.),
        ] {
            let points = ShapeKind::Line.points_with_snap(start, start + delta, true);
            let d = points[1] - points[0];
            assert!(d.x == 0. || d.y == 0. || (d.x.abs() - d.y.abs()).abs() < 0.0001);
            assert!(d.dot(delta) > 0.);
            for shape in ShapeKind::ALL {
                assert_eq!(
                    shape.points(start, start + delta),
                    shape.points_with_snap(start, start + delta, false)
                );
            }
            let t = ShapeKind::Triangle.points_with_snap(start, start + delta, true);
            let lengths: Vec<_> = t.windows(2).map(|p| (p[1] - p[0]).length()).collect();
            assert!((lengths[0] - lengths[1]).abs() < 0.0001);
            assert!((lengths[1] - lengths[2]).abs() < 0.0001);
            for shape in [ShapeKind::Circle, ShapeKind::Square] {
                assert_eq!(
                    shape.points(start, start + delta),
                    shape.points_with_snap(start, start + delta, true)
                );
            }
        }
        assert_eq!(
            ShapeKind::Line.points_with_snap(start, start, true),
            vec![start, start]
        );
    }

    #[test]
    fn shift_snaps_to_nearest_of_twenty_four_angles() {
        let start = Vec2::new(31., 47.);
        for sector in 0..24 {
            for offset in [-7., -1., 0., 1., 7.] {
                let angle = (sector as f32 * 15. + offset).to_radians();
                let end = start + Vec2::new(angle.cos(), angle.sin()) * 120.;
                let points = ShapeKind::Line.points_with_snap(start, end, true);
                let d = points[1] - points[0];
                let expected = (sector as f32 * 15.).to_radians();
                let direction = Vec2::new(expected.cos(), expected.sin());
                assert!((d.normalized() - direction).length() < 0.00001);
                assert_eq!(points[0], start);
            }
        }
    }
}
