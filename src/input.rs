use eframe::egui::{self, Pos2, Rect, Vec2};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PenPhase {
    Down,
    Move,
    Up,
    Cancel,
    #[default]
    Hover,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PenSample {
    pub id: u32,
    pub phase: PenPhase,
    pub time_ms: u32,
    pub client_px: [f32; 2],
    pub pressure: Option<f32>,
    pub tilt_xy: Option<[f32; 2]>,
    pub rotation_deg: Option<f32>,
}

/// Stateful pressure bridge between egui's platform input events and the drawing engine.
///
/// egui exposes pressure-capable touch/pen packets as `Event::Touch { force: Some(..) }`.
/// On Windows this is fed by the platform pointer/ink path when the backend/driver exposes it.
/// Mouse input remains usable through the configured fallback pressure.
#[derive(Debug, Clone)]
pub struct PressureInputState {
    active_pressure: Option<f32>,
    active_tilt_deg: Option<f32>,
    active_azimuth_deg: Option<f32>,
    active_rotation_deg: Option<f32>,
    device_rotation_seen: bool,
    device_pressure_seen: bool,
    device_orientation_seen: bool,
    native_id: Option<u32>,
    native_last: Option<PenSample>,
    native_filtered_pressure: Option<f32>,
    pen_frames: Vec<PointerFrame>,
    suppress_promoted_mouse: bool,
    touch_navigation: bool,
}

impl Default for PressureInputState {
    fn default() -> Self {
        Self {
            active_pressure: None,
            active_tilt_deg: None,
            active_azimuth_deg: None,
            active_rotation_deg: None,
            device_rotation_seen: false,
            device_pressure_seen: false,
            device_orientation_seen: false,
            native_id: None,
            native_last: None,
            native_filtered_pressure: None,
            pen_frames: Vec::new(),
            suppress_promoted_mouse: false,
            touch_navigation: false,
        }
    }
}

impl PressureInputState {
    pub fn take_pen_frames(&mut self) -> Vec<PointerFrame> {
        std::mem::take(&mut self.pen_frames)
    }

    pub fn native_owns_pointer(&self) -> bool {
        self.native_id.is_some() || self.suppress_promoted_mouse || !self.pen_frames.is_empty()
    }

    fn ingest_pen(
        &mut self,
        packet: PenSample,
        base: PointerFrame,
        rect: Rect,
        ppp: f32,
        gamma: f32,
    ) {
        if packet.phase == PenPhase::Cancel {
            if packet.id != 0 && self.native_id.is_some() && self.native_id != Some(packet.id) {
                return;
            }
            if self.native_id.is_some() {
                self.pen_frames.push(PointerFrame {
                    primary_released: true,
                    ..Default::default()
                });
            }
            self.native_id = None;
            self.native_last = None;
            self.native_filtered_pressure = None;
            self.clear_live();
            return;
        }
        // Hover explicitly means the nib is no longer touching. Some tablet
        // driver/UI handoffs omit Up; release that contact before the next Down.
        // Never synthesize a drawing press from an unowned Move packet.
        if (self.native_id == Some(packet.id) && packet.phase == PenPhase::Hover)
            || (self.native_id.is_some() && packet.phase == PenPhase::Down)
        {
            self.pen_frames.push(PointerFrame { primary_released: true, ..Default::default() });
            self.native_id = None;
            self.native_last = None;
            self.native_filtered_pressure = None;
            self.clear_live();
        }
        let position = Pos2::new(packet.client_px[0] / ppp, packet.client_px[1] / ppp);
        let over_canvas = rect.contains(position);
        if packet.phase == PenPhase::Hover {
            if self.native_id.is_none() && over_canvas {
                let orientation = packet
                    .tilt_xy
                    .map(|xy| tilt_orientation(xy, self.active_azimuth_deg));
                self.active_tilt_deg = orientation.map(|o| o.0);
                self.active_azimuth_deg = orientation.map(|o| o.1);
                self.device_orientation_seen |= orientation.is_some();
                self.active_rotation_deg = valid_rotation(packet.rotation_deg);
                self.device_rotation_seen |= self.active_rotation_deg.is_some();
            }
            return;
        }
        if packet.phase == PenPhase::Down {
            if !over_canvas || self.native_id.is_some() {
                return;
            }
            self.native_id = Some(packet.id);
            self.native_last = None;
            self.native_filtered_pressure = None;
        }
        if self.native_id != Some(packet.id) {
            return;
        }
        self.suppress_promoted_mouse = true;
        let dt_ms = self
            .native_last
            .map(|p| packet.time_ms.wrapping_sub(p.time_ms))
            .unwrap_or(100);
        let pressure = packet.pressure.map(|raw| {
            let normalized = normalize_pressure(raw);
            let filtered = match self.native_filtered_pressure {
                Some(previous) if normalized > 0.0 => {
                    // Time-based filtering preserves the same feel at 60, 120 or 240 Hz.
                    let alpha = 1.0 - (-(dt_ms.max(1) as f32) / 3.0).exp();
                    previous + (normalized - previous) * alpha
                }
                _ => normalized,
            };
            self.native_filtered_pressure = Some(filtered);
            filtered.powf(gamma.clamp(0.55, 1.8))
        });
        let orientation = packet
            .tilt_xy
            .map(|xy| tilt_orientation(xy, self.active_azimuth_deg));
        self.active_pressure = pressure;
        self.active_rotation_deg = valid_rotation(packet.rotation_deg);
        self.device_rotation_seen |= self.active_rotation_deg.is_some();
        self.active_tilt_deg = orientation.map(|o| o.0);
        self.active_azimuth_deg = orientation.map(|o| o.1);
        self.device_pressure_seen |= pressure.is_some();
        self.device_orientation_seen |= orientation.is_some();
        let previous_position = self
            .native_last
            .map(|p| Pos2::new(p.client_px[0] / ppp, p.client_px[1] / ppp));
        self.pen_frames.push(PointerFrame {
            position: Some(position),
            delta: previous_position
                .map(|p| position - p)
                .unwrap_or(Vec2::ZERO),
            primary_pressed: packet.phase == PenPhase::Down,
            // Include the last real position/force before releasing the stroke.
            primary_down: true,
            primary_released: packet.phase == PenPhase::Up,
            pressure: pressure.unwrap_or(base.pressure),
            pressure_from_device: pressure.is_some(),
            tilt_deg: self.active_tilt_deg,
            azimuth_deg: self.active_azimuth_deg,
            rotation_deg: self.active_rotation_deg,
            orientation_from_device: orientation.is_some(),
            over_canvas,
            middle_down: false,
            secondary_down: false,
            scroll_y: 0.0,
            ..base
        });
        self.native_last = Some(packet);
        if packet.phase == PenPhase::Up {
            self.native_id = None;
            self.native_last = None;
            self.native_filtered_pressure = None;
            self.clear_live();
        }
    }

    fn clear_live(&mut self) {
        self.active_pressure = None;
        self.active_tilt_deg = None;
        self.active_azimuth_deg = None;
        self.active_rotation_deg = None;
    }

    fn observe(&mut self, input: &egui::InputState, canvas_rect: Rect) {
        for event in &input.events {
            let egui::Event::Touch {
                phase, pos, force, ..
            } = event
            else {
                continue;
            };

            match phase {
                egui::TouchPhase::Start | egui::TouchPhase::Move => {
                    // Only claim tablet pressure for contact that is actually over the drawing
                    // viewport. This prevents a pressure-capable touch elsewhere in the UI from
                    // leaking into a subsequent mouse stroke.
                    if canvas_rect.contains(*pos) {
                        if let Some(raw_force) = *force {
                            let normalized = normalize_pressure(raw_force);
                            self.device_pressure_seen = true;
                            self.active_pressure = Some(match self.active_pressure {
                                // Keep only enough smoothing to suppress packet jitter. v0.12
                                // used 0.42 here, which visibly lagged quick pressure ramps and made
                                // the pencil feel mechanically uniform. Rising force responds a bit
                                // faster than release so deliberate accents appear immediately.
                                Some(previous) => {
                                    let alpha = if normalized >= previous { 0.74 } else { 0.62 };
                                    previous + (normalized - previous) * alpha
                                }
                                None => normalized,
                            });
                        }
                    }
                }
                egui::TouchPhase::End | egui::TouchPhase::Cancel => {
                    self.active_pressure = None;
                    self.active_tilt_deg = None;
                    self.active_azimuth_deg = None;
                    self.active_rotation_deg = None;
                }
            }
        }
    }

    pub fn active_rotation_deg(&self) -> Option<f32> {
        self.active_rotation_deg
    }
    pub fn device_rotation_seen(&self) -> bool {
        self.device_rotation_seen
    }

    pub fn active_pressure(&self) -> Option<f32> {
        self.active_pressure
    }

    pub fn device_pressure_seen(&self) -> bool {
        self.device_pressure_seen
    }

    pub fn active_tilt_deg(&self) -> Option<f32> {
        self.active_tilt_deg
    }

    pub fn active_azimuth_deg(&self) -> Option<f32> {
        self.active_azimuth_deg
    }

    pub fn device_orientation_seen(&self) -> bool {
        self.device_orientation_seen
    }

    fn effective_orientation(&self) -> (Option<f32>, Option<f32>, bool) {
        match (self.active_tilt_deg, self.active_azimuth_deg) {
            (Some(tilt), azimuth) => (
                Some(tilt.clamp(0.0, 89.0)),
                azimuth.map(|a| a.rem_euclid(360.0)),
                true,
            ),
            (None, Some(azimuth)) => (None, Some(azimuth.rem_euclid(360.0)), true),
            (None, None) => (None, None, false),
        }
    }

    fn effective_pressure(&self, fallback: f32) -> (f32, bool) {
        match self.active_pressure {
            Some(pressure) => (pressure.clamp(0.0, 1.0), true),
            None => (fallback.clamp(0.01, 1.0), false),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PointerFrame {
    pub position: Option<Pos2>,
    pub delta: Vec2,
    pub primary_down: bool,
    pub primary_pressed: bool,
    pub primary_released: bool,
    pub middle_down: bool,
    pub secondary_down: bool,
    pub space_down: bool,
    pub scroll_y: f32,
    pub over_canvas: bool,
    /// Per-frame pressure already resolved to device force or the mouse fallback.
    pub pressure: f32,
    pub pressure_from_device: bool,
    /// Live stylus tilt in degrees away from vertical when the backend exposes it.
    pub tilt_deg: Option<f32>,
    /// Live stylus azimuth in degrees when the backend exposes it.
    pub azimuth_deg: Option<f32>,
    pub rotation_deg: Option<f32>,
    pub orientation_from_device: bool,
    pub touch_navigation: bool,
    /// Current finger center, scale change, and center movement in screen points.
    pub zoom_gesture: Option<(Pos2, f32, Vec2)>,
    /// Two-finger twist in radians for this frame, independent of pen barrel rotation.
    pub twist_radians: f32,
}

impl PointerFrame {
    pub fn capture(
        ui: &egui::Ui,
        canvas_rect: Rect,
        pressure_state: &mut PressureInputState,
        mouse_pressure: f32,
        pen_pressure_gamma: f32,
    ) -> Self {
        let mut frame = ui.input(|input| {
            if pressure_state.native_id.is_none() {
                pressure_state.observe(input, canvas_rect);
            }
            let position = input.pointer.hover_pos();
            let over_canvas = position.is_some_and(|p| canvas_rect.contains(p));
            let (pressure, pressure_from_device) =
                pressure_state.effective_pressure(mouse_pressure);
            let (tilt_deg, azimuth_deg, orientation_from_device) =
                pressure_state.effective_orientation();
            let touch = input.multi_touch();
            // Keep the remaining finger from drawing when the other finger lifts first.
            if !input.any_touches() {
                pressure_state.touch_navigation = false;
            }
            if pressure_state.native_id.is_none()
                && touch.is_some_and(|t| canvas_rect.contains(t.start_pos))
            {
                pressure_state.touch_navigation = true;
            }
            let zoom_gesture = if pressure_state.native_id.is_some() {
                None
            } else if let Some(touch) = touch.filter(|_| pressure_state.touch_navigation) {
                Some((touch.center_pos, touch.zoom_delta, touch.translation_delta))
            } else if over_canvas && (input.zoom_delta() - 1.0).abs() > 0.00001 {
                // Also accept bridges that translate pinch into Ctrl+wheel or Zoom events.
                position.map(|p| (p, input.zoom_delta(), Vec2::ZERO))
            } else {
                None
            };

            Self {
                position,
                delta: input.pointer.delta(),
                primary_down: input.pointer.primary_down(),
                primary_pressed: input.pointer.primary_pressed() && over_canvas,
                primary_released: input.pointer.primary_released(),
                middle_down: input.pointer.middle_down(),
                secondary_down: input.pointer.button_down(egui::PointerButton::Secondary),
                space_down: input.key_down(egui::Key::Space),
                scroll_y: if over_canvas {
                    input.smooth_scroll_delta.y
                } else {
                    0.0
                },
                over_canvas,
                pressure: if pressure_from_device && pressure_state.native_id.is_none() {
                    pressure.powf(pen_pressure_gamma.clamp(0.55, 1.8))
                } else {
                    pressure
                },
                pressure_from_device,
                tilt_deg,
                azimuth_deg,
                rotation_deg: pressure_state.active_rotation_deg,
                orientation_from_device,
                touch_navigation: pressure_state.touch_navigation,
                zoom_gesture,
                twist_radians: if pressure_state.native_id.is_none()
                    && pressure_state.touch_navigation
                {
                    touch.map_or(0., |t| t.rotation_delta)
                } else {
                    0.
                },
            }
        });
        // Hold off synthesized mouse/touch events until all pen-up promotions have drained.
        if !frame.primary_down && !frame.primary_released && !frame.primary_pressed {
            pressure_state.suppress_promoted_mouse = false;
        }
        #[cfg(windows)]
        for packet in crate::native_pen::drain() {
            pressure_state.ingest_pen(
                packet,
                PointerFrame {
                    pressure: mouse_pressure,
                    ..frame
                },
                canvas_rect,
                ui.ctx().pixels_per_point(),
                pen_pressure_gamma,
            );
        }
        if !ui.input(|i| i.focused) {
            pressure_state.ingest_pen(
                PenSample {
                    phase: PenPhase::Cancel,
                    ..Default::default()
                },
                frame,
                canvas_rect,
                1.0,
                pen_pressure_gamma,
            );
            pressure_state.clear_live();
            frame.primary_down = false;
            frame.primary_pressed = false;
            frame.primary_released = true;
        }
        if let Some(last) = pressure_state.pen_frames.last().copied() {
            frame = last;
        }
        if pressure_state.native_owns_pointer() {
            frame.zoom_gesture = None;
            frame.twist_radians = 0.;
            frame.touch_navigation = false;
        }
        frame
    }

    pub fn wants_pan(self) -> bool {
        self.over_canvas
            && (self.secondary_down || self.middle_down || (self.space_down && self.primary_down))
    }
}

/// Windows tilt axes are angles of the barrel from the screen normal. Convert their
/// tangents into polar tilt. The engine's rear facet extends opposite its azimuth,
/// so reverse the barrel bearing to put broad contact beneath the leaning pencil.
fn tilt_orientation(xy: [f32; 2], previous_bearing: Option<f32>) -> (f32, f32) {
    let x = xy[0].clamp(-89.9, 89.9).to_radians().tan();
    let y = xy[1].clamp(-89.9, 89.9).to_radians().tan();
    let radius = x.hypot(y);
    let tilt = radius.atan().to_degrees().min(89.0);
    let bearing = if radius < 0.0175 {
        previous_bearing.unwrap_or(0.0)
    } else {
        (y.atan2(x).to_degrees() + 180.0).rem_euclid(360.0)
    };
    (tilt, bearing)
}

fn valid_rotation(angle: Option<f32>) -> Option<f32> {
    angle.filter(|a| a.is_finite()).map(|a| a.rem_euclid(360.0))
}

fn normalize_pressure(raw: f32) -> f32 {
    // Keep device normalization nearly linear. The graphite engine owns the physical response
    // curve; applying a second gamma curve here made it impossible to reason about or calibrate
    // pressure consistently across input, contact width and material transfer.
    const DEAD_ZONE: f32 = 0.006;
    ((raw.clamp(0.0, 1.0) - DEAD_ZONE) / (1.0 - DEAD_ZONE)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_pen_id_can_recover_a_missing_release() {
        let mut state=PressureInputState::default();
        feed(&mut state,sample(PenPhase::Down,100,40.,Some(0.3)));
        state.take_pen_frames();
        let mut next=sample(PenPhase::Down,120,80.,Some(0.2)); next.id=9;
        feed(&mut state,next);
        let frames=state.take_pen_frames();
        assert_eq!(frames.len(),2);
        assert!(frames[0].primary_released && frames[1].primary_pressed);
        assert_eq!(state.native_id,Some(9));
    }
    #[test]
    fn tablet_hover_and_new_down_recover_missing_up_without_phantom_strokes() {
        for phase in [PenPhase::Hover,PenPhase::Down] {
            let mut state=PressureInputState::default();
            feed(&mut state,sample(PenPhase::Down,100,40.,Some(0.9)));
            state.take_pen_frames();
            feed(&mut state,sample(phase,120,80.,Some(0.1)));
            let frames=state.take_pen_frames();
            assert!(frames[0].primary_released && frames[0].position.is_none());
            if phase == PenPhase::Hover {
                assert!(state.native_id.is_none());
                feed(&mut state,sample(PenPhase::Move,121,90.,Some(0.1)));
                assert!(state.take_pen_frames().is_empty());
                feed(&mut state,sample(PenPhase::Down,122,90.,Some(0.1)));
                assert!(state.take_pen_frames()[0].primary_pressed);
            } else {
                assert!(frames[1].primary_pressed);
                assert!((frames[1].pressure-normalize_pressure(0.1)).abs()<0.00001);
            }
        }
    }
    #[test]
    fn barrel_rotation_is_separate_from_tilt_and_zero_is_valid() {
        let mut state = PressureInputState::default();
        let mut packet = sample(PenPhase::Down, 100, 40., Some(0.5));
        packet.rotation_deg = Some(0.);
        feed(&mut state, packet);
        let first = state.take_pen_frames()[0];
        assert_eq!(first.rotation_deg, Some(0.));
        assert_eq!(first.tilt_deg, Some(45.));
        packet.phase = PenPhase::Move;
        packet.time_ms = 104;
        packet.rotation_deg = Some(120.);
        feed(&mut state, packet);
        let rotated = state.take_pen_frames()[0];
        assert_eq!(rotated.rotation_deg, Some(120.));
        assert_eq!(rotated.azimuth_deg, first.azimuth_deg);
        packet.phase = PenPhase::Up;
        feed(&mut state, packet);
        assert_eq!(state.active_rotation_deg(), None);
        assert!(state.device_rotation_seen());
        assert_eq!(valid_rotation(Some(f32::NAN)), None);
    }

    fn sample(phase: PenPhase, time_ms: u32, x: f32, pressure: Option<f32>) -> PenSample {
        PenSample {
            id: 7,
            phase,
            time_ms,
            client_px: [x, 40.0],
            pressure,
            tilt_xy: Some([45.0, 0.0]),
            rotation_deg: None,
        }
    }
    fn feed(state: &mut PressureInputState, packet: PenSample) {
        state.ingest_pen(
            packet,
            PointerFrame {
                pressure: 0.48,
                ..Default::default()
            },
            Rect::from_min_max(Pos2::ZERO, Pos2::new(300.0, 300.0)),
            2.0,
            1.0,
        );
    }

    #[test]
    fn native_packets_preserve_full_strokes_between_frames_and_scale_coordinates() {
        let mut state = PressureInputState::default();
        feed(&mut state, sample(PenPhase::Down, 100, 40.0, Some(0.1)));
        feed(&mut state, sample(PenPhase::Move, 104, 60.0, Some(0.8)));
        feed(&mut state, sample(PenPhase::Up, 108, 80.0, Some(0.0)));
        let packets = state.take_pen_frames();
        assert_eq!(packets.len(), 3);
        assert!(packets[0].primary_pressed && packets[2].primary_released);
        assert_eq!(packets[1].position, Some(Pos2::new(30.0, 20.0)));
        assert!(packets[1].pressure > packets[0].pressure);
        assert_eq!(packets[2].pressure, 0.0);
        assert_eq!(packets[1].tilt_deg, Some(45.0));
        assert_eq!(state.active_pressure(), None);
        assert!(state.native_owns_pointer()); // synthesized mouse-up is still suppressed
    }

    #[test]
    fn absent_pressure_and_tilt_use_fallback_and_cancel_releases_contact() {
        let mut state = PressureInputState::default();
        let mut packet = sample(PenPhase::Down, 0, 40.0, None);
        packet.tilt_xy = None;
        feed(&mut state, packet);
        let frame = state.take_pen_frames()[0];
        assert!(!frame.pressure_from_device && !frame.orientation_from_device);
        assert_eq!(frame.pressure, 0.48);
        feed(
            &mut state,
            PenSample {
                phase: PenPhase::Cancel,
                ..Default::default()
            },
        );
        assert!(state.take_pen_frames()[0].primary_released);
        assert!(state.native_id.is_none());
        feed(&mut state, sample(PenPhase::Move, 20, 90.0, Some(0.9)));
        assert!(state.take_pen_frames().is_empty());
    }

    #[test]
    fn tilt_axes_cover_quadrants_without_an_upright_flip() {
        for (xy, bearing) in [
            ([45., 0.], 180.),
            ([0., 45.], 270.),
            ([-45., 0.], 0.),
            ([0., -45.], 90.),
        ] {
            let (tilt, angle) = tilt_orientation(xy, None);
            assert!((tilt - 45.0).abs() < 0.01 && (angle - bearing).abs() < 0.01);
        }
        assert_eq!(tilt_orientation([0., 0.], Some(271.)), (0., 271.));
        let (tilt, _) = tilt_orientation([45., 45.], None);
        assert!((tilt - 54.7356).abs() < 0.01);
        assert!(tilt_orientation([90., -90.], None).0.is_finite());
    }

    #[test]
    fn pressure_filter_is_stable_across_packet_rates() {
        let filtered = |step: u32| {
            let mut state = PressureInputState::default();
            feed(&mut state, sample(PenPhase::Down, 0, 40.0, Some(0.1)));
            for time in (step..=24).step_by(step as usize) {
                feed(&mut state, sample(PenPhase::Move, time, 40.0, Some(0.8)));
            }
            state.active_pressure().unwrap()
        };
        assert!((filtered(4) - filtered(8)).abs() < 0.00001);
    }

    fn touch(id: u64, phase: egui::TouchPhase, x: f32) -> egui::Event {
        touch_at(id, phase, Pos2::new(x, 100.))
    }

    fn touch_at(id: u64, phase: egui::TouchPhase, pos: Pos2) -> egui::Event {
        egui::Event::Touch {
            device_id: egui::TouchDeviceId(99),
            id: egui::TouchId(id),
            phase,
            pos,
            force: None,
        }
    }

    #[test]
    fn pinch_zoom_keeps_ui_scale_and_blocks_remaining_finger() {
        let ctx = egui::Context::default();
        let mut state = PressureInputState::default();
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400., 300.));
        let mut capture = |events: Vec<egui::Event>| {
            let mut frame = PointerFrame::default();
            let _ = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(rect),
                    events,
                    ..Default::default()
                },
                |ui| {
                    frame = PointerFrame::capture(ui, rect, &mut state, 0.48, 1.0);
                },
            );
            frame
        };
        capture(vec![
            egui::Event::PointerMoved(Pos2::new(100., 100.)),
            touch(1, egui::TouchPhase::Start, 100.),
            touch(2, egui::TouchPhase::Start, 200.),
        ]);
        capture(vec![]);
        let f = capture(vec![
            touch(1, egui::TouchPhase::Move, 75.),
            touch(2, egui::TouchPhase::Move, 225.),
        ]);
        assert!(f.touch_navigation);
        let (center, zoom, translation) = f.zoom_gesture.unwrap();
        assert_eq!(center, Pos2::new(150., 100.));
        assert!((zoom - 1.5).abs() < 0.001);
        assert_eq!(translation, Vec2::ZERO);
        let pan = capture(vec![
            touch(1, egui::TouchPhase::Move, 105.),
            touch(2, egui::TouchPhase::Move, 255.),
        ]);
        let (center, zoom, translation) = pan.zoom_gesture.unwrap();
        assert_eq!(center, Pos2::new(180., 100.));
        assert!((zoom - 1.).abs() < 0.001);
        assert_eq!(translation, Vec2::new(30., 0.));
        let combined = capture(vec![
            touch_at(1, egui::TouchPhase::Move, Pos2::new(150., 120.)),
            touch_at(2, egui::TouchPhase::Move, Pos2::new(330., 120.)),
        ]);
        let (center, zoom, translation) = combined.zoom_gesture.unwrap();
        assert_eq!(center, Pos2::new(240., 120.));
        assert!((zoom - 1.2).abs() < 0.001);
        assert_eq!(translation, Vec2::new(60., 20.));
        let twist = capture(vec![
            touch_at(1, egui::TouchPhase::Move, Pos2::new(240., 30.)),
            touch_at(2, egui::TouchPhase::Move, Pos2::new(240., 210.)),
        ]);
        assert!(twist.touch_navigation);
        assert!((twist.twist_radians - std::f32::consts::FRAC_PI_2).abs() < 0.001);
        assert!((twist.zoom_gesture.unwrap().1 - 1.).abs() < 0.001);
        assert_eq!(twist.zoom_gesture.unwrap().2, Vec2::ZERO);
        assert!(capture(vec![touch(2, egui::TouchPhase::End, 330.)])
            .zoom_gesture
            .is_none());
        assert!(capture(vec![touch(1, egui::TouchPhase::Move, 120.)])
            .zoom_gesture
            .is_none());
        assert!(capture(vec![touch(2, egui::TouchPhase::End, 225.)]).touch_navigation);
        assert!(!capture(vec![touch(1, egui::TouchPhase::End, 75.)]).touch_navigation);
        assert_eq!(ctx.zoom_factor(), 1.0);
    }

    #[test]
    fn pressure_normalization_is_monotonic() {
        let a = normalize_pressure(0.10);
        let b = normalize_pressure(0.50);
        let c = normalize_pressure(0.90);
        assert!(a < b && b < c);
    }

    #[test]
    fn pressure_dead_zone_reaches_zero() {
        assert_eq!(normalize_pressure(0.0), 0.0);
        assert_eq!(normalize_pressure(0.004), 0.0);
    }

    #[test]
    fn device_normalization_does_not_hide_midrange_force() {
        let mid = normalize_pressure(0.50);
        assert!(mid > 0.49 && mid < 0.51);
    }
}
