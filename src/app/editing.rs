use super::*;
use crate::core::{
    selection::Cutout,
    vector::{self, VectorLayer},
};
use std::sync::Arc;
#[derive(Default)]
pub(super) struct EditingState {
    vector_pick: Option<Vec2>,
    pub lasso: Option<Vec<Vec2>>,
    pub transform: Option<Transform>,
    pub guide: bool,
    pub selected_path: Option<usize>,
}
pub(super) struct Transform {
    pub angle: f32,
    pub rotate_mode: bool,
    rotation_drag: Option<(f32, f32)>,
    affected: Rect,
    vectors: Option<(Arc<VectorLayer>, Vec<usize>)>,
    cutout: Cutout,
    tx: EditTransaction,
    texture: TextureHandle,
    target: Rect,
    drag: Option<(usize, Pos2, Rect)>,
}
fn corners(r: Rect) -> [Pos2; 4] {
    [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
    ]
}
impl GraphiteApp {
    pub(super) fn select_tool(&mut self, tool: ToolKind) {
        self.finish_stroke();
        self.cancel_transform_for_tool_change();
        self.shape_drag = None;
        self.editing.lasso = None;
        self.editing.vector_pick = None;
        self.editing.selected_path = None;
        self.settings.tool = if tool == ToolKind::Brush { ToolKind::Pencil } else { tool };
        self.rotate_view = false;
        self.view_rotation_drag = false;
    }

    pub(super) fn cancel_transform_for_tool_change(&mut self) {
        if self.editing.transform.is_some() {
            self.cancel_transform_and_deselect();
        }
    }

    pub(super) fn cancel_transform_and_deselect(&mut self) {
        self.cancel_transform();
        self.document.selection = Default::default();
        self.editing.selected_path = None;
        self.editing.vector_pick = None;
        self.editing.lasso = None;
    }

    pub(super) fn show_transform_actions(&mut self, ui: &mut egui::Ui) {
        if self.editing.transform.is_none() { return; }
        ui.horizontal(|ui| {
            ui.strong("Transform");
            if crate::ui::tool_icons::transform_action_button(ui, true).clicked() {
                self.apply_transform();
            }
            if crate::ui::tool_icons::transform_action_button(ui, false).clicked() {
                self.cancel_transform_and_deselect();
            }
        });
        ui.checkbox(&mut self.settings.transform_keep_aspect, "Keep aspect ratio")
            .on_hover_text("Preserve the selection's proportions while resizing. Shift also constrains resizing.");
        ui.separator();
    }

    fn vector_at(&self, point: Vec2) -> Option<usize> {
        let layer = &self.document.layers[self.document.active_layer_index()];
        if !layer.visible || layer.opacity == 0 {
            return None;
        }
        layer
            .vectors
            .strokes
            .iter()
            .rposition(|s| s.hit_test(point, 5. / self.viewport.zoom, self.document.spec.dpi))
    }
    fn pick_vector(&mut self, ui: &egui::Ui, canvas: Rect, input: PointerFrame) -> bool {
        if self.settings.tool != ToolKind::VectorSelect {
            return false;
        }
        let pos = input
            .position
            .map(|p| self.viewport.screen_to_document(canvas, p));
        if input.primary_pressed
            && input
                .position
                .is_some_and(|p| self.viewport.contains(canvas, p))
        {
            let point = pos.unwrap();
            if let Some(t) = &self.editing.transform {
                let local = vector::rotate(point.to_pos2(), t.target.center(), -t.angle);
                let knob = t.target.center_top() - Vec2::new(0., 24. / self.viewport.zoom);
                let grip = corners(t.target)
                    .iter()
                    .any(|p| p.distance(local) * self.viewport.zoom < 10.)
                    || knob.distance(local) * self.viewport.zoom < 10.;
                let hit = self.vector_at(point);
                if grip
                    || (t.target.contains(local)
                        && (hit.is_none() || hit == self.editing.selected_path))
                {
                    return false;
                }
            }
            self.apply_transform();
            self.editing.vector_pick = Some(point);
        }
        if self.editing.vector_pick.is_some() {
            if let Some(pos) = pos {
                self.editing.vector_pick = Some(pos);
            }
            if input.primary_released {
                let point = self.editing.vector_pick.take().unwrap();
                self.document.selection = Default::default();
                self.editing.selected_path = self.vector_at(point);
                if self.editing.selected_path.is_some() {
                    self.begin_transform(ui.ctx());
                } else {
                    self.status =
                        "No editable stroke here. Select its layer, then click the stroke.".into();
                }
            }
            return true;
        }
        false
    }
    fn finish_free_selection(&mut self, path: Vec<Vec2>, ctx: &egui::Context) {
        self.document.selection.set(
            path,
            self.document.spec.width_px,
            self.document.spec.height_px,
        );
        if !self.document.selection.polygon.is_empty() {
            self.begin_transform(ctx);
        }
    }
    pub(super) fn begin_transform(&mut self, ctx: &egui::Context) {
        if self.editing.transform.is_some() {
            return;
        }
        self.finish_stroke();
        self.shape_drag = None;
        self.editing.lasso = None;
        let original = self.document.layers[self.document.active_layer_index()]
            .vectors
            .clone();
        let chosen: Vec<usize> = if let Some(index) = self
            .editing
            .selected_path
            .filter(|&i| i < original.strokes.len())
        {
            vec![index]
        } else {
            (0..original.strokes.len()).collect()
        };
        let vector_mode = self.document.selection.polygon.is_empty()
            && !chosen.is_empty()
            && (self.editing.selected_path.is_some()
                || original
                    .base
                    .as_ref()
                    .is_none_or(|b| b.occupied_pixel_count() == 0));
        let mut preview_doc = None;
        if vector_mode {
            let mut preview = self.document.clone();
            let selected = VectorLayer {
                base: None,
                strokes: chosen
                    .iter()
                    .map(|&i| original.strokes[i].clone())
                    .collect(),
            };
            vector::replay(&mut preview, &selected, &mut EditTransaction::default());
            preview_doc = Some(preview);
        }
        let affected = chosen.iter().fold(Rect::NOTHING, |r, &i| {
            r.union(original.strokes[i].bounds(self.document.spec.dpi))
        });
        let source = preview_doc.as_ref().unwrap_or(&self.document);
        let captured = Cutout::capture(source).map(|c| {
            c.or_else(|| {
                if !vector_mode {
                    return None;
                }
                let mut bounds = affected.intersect(Rect::from_min_size(
                    Pos2::ZERO,
                    Vec2::new(source.spec.width_px as f32, source.spec.height_px as f32),
                ));
                if !bounds.is_positive() {
                    return None;
                }
                bounds.min = Pos2::new(bounds.min.x.floor(), bounds.min.y.floor());
                bounds.max = Pos2::new(bounds.max.x.ceil(), bounds.max.y.ceil());
                let (width, height) = (bounds.width() as usize, bounds.height() as usize);
                if width * height > 4_000_000 {
                    return None;
                }
                Some(Cutout {
                    bounds,
                    width,
                    height,
                    pixels: vec![Default::default(); width * height],
                })
            })
        });
        let cutout = match captured {
            Ok(Some(c)) => c,
            Ok(None) => {
                self.status = "No selected marks to transform on this layer.".into();
                return;
            }
            Err(e) => {
                self.status = e;
                return;
            }
        };
        let mut bytes = vec![0; cutout.width * cutout.height * 4];
        for y in 0..cutout.height {
            for x in 0..cutout.width {
                if cutout.pixels[y * cutout.width + x].total_deposit() <= 0. {
                    continue;
                }
                let i = source.index(
                    x + cutout.bounds.min.x as usize,
                    y + cutout.bounds.min.y as usize,
                );
                let mut rgba =
                    crate::render::layer_pixel_rgba(source, Some(source.active_layer_index()), i);
                rgba[3] *= source.layers[source.active_layer_index()].opacity as f32 / 255.;
                for c in 0..4 {
                    bytes[(y * cutout.width + x) * 4 + c] =
                        (rgba[c] * 255.).round().clamp(0., 255.) as u8;
                }
            }
        }
        let texture = ctx.load_texture(
            "transform_preview",
            egui::ColorImage::from_rgba_unmultiplied([cutout.width, cutout.height], &bytes),
            TextureOptions::LINEAR,
        );
        let mut tx = EditTransaction::default();
        let vectors = if vector_mode {
            let rest = VectorLayer {
                base: original.base.clone(),
                strokes: original
                    .strokes
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !chosen.contains(i))
                    .map(|(_, s)| s.clone())
                    .collect(),
            };
            vector::replay(&mut self.document, &rest, &mut tx);
            tx.restore_outside(&mut self.document, &[affected]);
            Some((original, chosen))
        } else {
            cutout.lift(&mut self.document, &mut tx);
            None
        };
        let target = cutout.bounds;
        self.editing.transform = Some(Transform {
            angle: 0.,
            rotate_mode: false,
            rotation_drag: None,
            affected,
            vectors,
            cutout,
            tx,
            texture,
            target,
            drag: None,
        });
        self.status=if vector_mode{"Transform: Confirm or Enter keeps the result. Cancel, Esc or switching tools restores the original."}else{"Pixel selection transform: confirming flattens this layer’s editable paths. Cancel, Esc or switching tools restores the original."}.into();
    }
    pub(super) fn begin_rotation(&mut self, ctx: &egui::Context) {
        self.begin_transform(ctx);
        if let Some(t) = &mut self.editing.transform {
            t.rotate_mode = true;
            t.drag = None;
            t.rotation_drag = None;
            self.status="Rotate: drag around the center, or set the angle. Shift snaps to 90°. Enter applies; Esc cancels.".into();
        }
    }
    pub(super) fn cancel_transform(&mut self) {
        if let Some(t) = self.editing.transform.take() {
            t.tx.rollback(&mut self.document);
            self.status = "Transform cancelled.".into();
        }
    }
    pub(super) fn apply_transform(&mut self) {
        let Some(mut t) = self.editing.transform.take() else {
            return;
        };
        if t.target == t.cutout.bounds && t.angle.abs() < 1e-6 {
            t.tx.rollback(&mut self.document);
            return;
        }
        let vector_mode = t.vectors.is_some();
        if let Some((original, selected)) = t.vectors {
            let mut changed = (*original).clone();
            let mut new_affected = Rect::NOTHING;
            for index in selected {
                changed.strokes[index] = Arc::new(
                    original.strokes[index]
                        .transformed(t.cutout.bounds, t.target)
                        .rotated(t.target.center(), t.angle),
                );
                new_affected =
                    new_affected.union(changed.strokes[index].bounds(self.document.spec.dpi));
            }
            vector::replay(&mut self.document, &changed, &mut t.tx);
            t.tx.restore_outside(&mut self.document, &[t.affected, new_affected]);
            let active = self.document.active_layer_index();
            self.document.layers[active].vectors = Arc::new(changed);
        } else {
            t.cutout
                .place_rotated(&mut self.document, t.target, t.angle, &mut t.tx);
            self.bake_paths();
        }
        if !self.document.selection.polygon.is_empty() {
            let polygon = self
                .document
                .selection
                .polygon
                .iter()
                .map(|p| {
                    let p = t.target.min.to_vec2()
                        + (*p - t.cutout.bounds.min.to_vec2()) / t.cutout.bounds.size()
                            * t.target.size();
                    vector::rotate(p.to_pos2(), t.target.center(), t.angle).to_vec2()
                })
                .collect();
            self.document.selection.set(
                polygon,
                self.document.spec.width_px,
                self.document.spec.height_px,
            );
        }
        self.history.push(t.tx, &self.document);
        self.tabs[self.active_tab].modified = true;
        self.status = if vector_mode {
            "Transformed vector paths and regenerated graphite texture. Ctrl+Z restores both."
        } else {
            "Transformed pixel selection. Ctrl+Z restores the original paths and material."
        }
        .into();
    }
    pub(super) fn edit_input(&mut self, ui: &egui::Ui, canvas: Rect, input: PointerFrame) {
        if input.wants_pan() || input.touch_navigation {
            self.editing.vector_pick = None;
            return;
        }
        if self.pick_vector(ui, canvas, input) {
            return;
        }
        let Some(screen) = input.position else {
            if input.primary_released {
                if let Some(t) = &mut self.editing.transform {
                    t.rotation_drag = None;
                    t.drag = None;
                }
                if let Some(path) = self.editing.lasso.take() {
                    self.finish_free_selection(path, ui.ctx());
                }
            }
            return;
        };
        let pos = self.viewport.screen_to_document(canvas, screen).to_pos2();
        if let Some(t) = &mut self.editing.transform {
            let local = vector::rotate(pos, t.target.center(), -t.angle);
            let knob = vector::rotate(
                t.target.center_top() - Vec2::new(0., 24. / self.viewport.zoom),
                t.target.center(),
                t.angle,
            );
            if input.primary_pressed && self.viewport.contains(canvas, screen) {
                if t.rotate_mode || knob.distance(pos) * self.viewport.zoom < 10. {
                    let delta = pos - t.target.center();
                    if delta.length() > 1. {
                        t.rotation_drag = Some((delta.angle(), t.angle));
                    }
                } else {
                    let grip = corners(t.target)
                        .iter()
                        .position(|p| p.distance(local) * self.viewport.zoom < 10.);
                    if let Some(index) = grip.or_else(|| t.target.contains(local).then_some(4)) {
                        t.drag = Some((index, pos, t.target));
                    }
                }
            }
            if input.primary_down {
                if let Some((start, angle)) = t.rotation_drag {
                    let current = (pos - t.target.center()).angle();
                    let delta = (current - start + std::f32::consts::PI)
                        .rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI;
                    let value = angle + delta;
                    t.angle = if ui.input(|i| i.modifiers.shift) {
                        (value / std::f32::consts::FRAC_PI_2).round() * std::f32::consts::FRAC_PI_2
                    } else {
                        value
                    };
                } else if let Some((index, start, original)) = t.drag {
                    if index == 4 {
                        t.target = original.translate(pos - start);
                    } else {
                        let delta = vector::rotate(pos, original.center(), -t.angle)
                            - vector::rotate(start, original.center(), -t.angle);
                        let anchor = corners(original)[(index + 2) % 4];
                        let mut moving = corners(original)[index] + delta;
                        if self.settings.transform_keep_aspect || ui.input(|i| i.modifiers.shift) {
                            let ratio = original.width() / original.height();
                            let v = moving - anchor;
                            let h = v.y.abs().max(v.x.abs() / ratio).max(1.).max(1. / ratio);
                            moving = anchor
                                + Vec2::new(
                                    if v.x < 0. { -h * ratio } else { h * ratio },
                                    if v.y < 0. { -h } else { h },
                                );
                        }
                        let mut r = Rect::from_two_pos(anchor, moving);
                        r.max = r.max.max(r.min + Vec2::splat(1.));
                        let center = vector::rotate(r.center(), original.center(), t.angle);
                        r = r.translate(center - r.center());
                        if r.width() * r.height() <= 16_000_000. {
                            t.target = r;
                        }
                    }
                }
            }
            if input.primary_released {
                t.drag = None;
                t.rotation_drag = None;
            }
        } else if self.settings.tool == ToolKind::Lasso {
            let point = pos.to_vec2().clamp(
                Vec2::ZERO,
                Vec2::new(
                    self.document.spec.width_px as f32,
                    self.document.spec.height_px as f32,
                ),
            );
            if input.primary_pressed && self.viewport.contains(canvas, screen) {
                self.editing.selected_path = None;
                self.editing.lasso = Some(vec![point]);
            }
            if input.primary_down {
                if let Some(path) = &mut self.editing.lasso {
                    if path
                        .last()
                        .is_none_or(|p| (*p - point).length() * self.viewport.zoom > 2.)
                        && path.len() < 4096
                    {
                        path.push(point);
                    }
                }
            }
            if input.primary_released {
                if let Some(path) = self.editing.lasso.take() {
                    self.finish_free_selection(path, ui.ctx());
                }
            }
        }
    }
    pub(super) fn paint_editing(&self, p: &egui::Painter, canvas: Rect) {
        if let Some(t) = &self.editing.transform {
            let points = corners(t.target).map(|p| {
                self.viewport.document_to_screen(
                    canvas,
                    vector::rotate(p, t.target.center(), t.angle).to_vec2(),
                )
            });
            let mut mesh = egui::Mesh::with_texture(t.texture.id());
            for (pos, uv) in points.into_iter().zip([
                Pos2::ZERO,
                Pos2::new(1., 0.),
                Pos2::new(1., 1.),
                Pos2::new(0., 1.),
            ]) {
                mesh.vertices.push(egui::epaint::Vertex {
                    pos,
                    uv,
                    color: Color32::WHITE,
                });
            }
            mesh.indices.extend([0, 1, 2, 0, 2, 3]);
            p.add(egui::Shape::mesh(mesh));
            let blue = Color32::from_rgb(45, 120, 200);
            let mut outline = points.to_vec();
            outline.push(points[0]);
            p.add(egui::Shape::line(outline, Stroke::new(1., blue)));
            for corner in points {
                p.rect(
                    Rect::from_center_size(corner, Vec2::splat(8.)),
                    0.,
                    Color32::WHITE,
                    Stroke::new(1., blue),
                    egui::StrokeKind::Inside,
                );
            }
            let top = self.viewport.document_to_screen(
                canvas,
                vector::rotate(t.target.center_top(), t.target.center(), t.angle).to_vec2(),
            );
            let knob = self.viewport.document_to_screen(
                canvas,
                vector::rotate(
                    t.target.center_top() - Vec2::new(0., 24. / self.viewport.zoom),
                    t.target.center(),
                    t.angle,
                )
                .to_vec2(),
            );
            p.line_segment([top, knob], Stroke::new(1., blue));
            p.circle(knob, 5., Color32::WHITE, Stroke::new(1., blue));
        } else {
            if let Some(record) = self.editing.selected_path.and_then(|i| {
                self.document.layers[self.document.active_layer_index()]
                    .vectors
                    .strokes
                    .get(i)
            }) {
                let points: Vec<_> = record
                    .points
                    .iter()
                    .map(|p| {
                        self.viewport
                            .document_to_screen(canvas, Vec2::new(p.x, p.y))
                    })
                    .collect();
                if points.len() > 1 {
                    p.add(egui::Shape::line(
                        points,
                        Stroke::new(1., Color32::from_rgb(45, 120, 200)),
                    ));
                }
            }
            let path = self
                .editing
                .lasso
                .as_ref()
                .unwrap_or(&self.document.selection.polygon);
            if path.len() > 1 {
                let mut points: Vec<_> = path
                    .iter()
                    .map(|p| self.viewport.document_to_screen(canvas, *p))
                    .collect();
                points.push(points[0]);
                p.add(egui::Shape::line(
                    points.clone(),
                    Stroke::new(2., Color32::WHITE),
                ));
                p.extend(egui::Shape::dashed_line(
                    &points,
                    Stroke::new(1., Color32::BLACK),
                    4.,
                    4.,
                ));
            }
        }
    }
    pub(super) fn editing_shortcuts(&mut self, ui: &egui::Ui) -> bool {
        let key = ui.input_mut(|i| {
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::R) {
                7
            } else if i.consume_key(egui::Modifiers::CTRL, egui::Key::T) {
                1
            } else if i.consume_key(egui::Modifiers::CTRL, egui::Key::D) {
                2
            } else if i.consume_key(egui::Modifiers::CTRL, egui::Key::A) {
                3
            } else if i.key_pressed(egui::Key::Escape) {
                4
            } else if i.key_pressed(egui::Key::Enter) {
                5
            } else if i.key_pressed(egui::Key::Delete) {
                6
            } else {
                0
            }
        });
        match key {
            1 => {
                self.begin_transform(ui.ctx());
                if let Some(t) = &mut self.editing.transform {
                    t.rotate_mode = false;
                    t.rotation_drag = None;
                }
            }
            7 => self.begin_rotation(ui.ctx()),
            2 => {
                self.cancel_transform();
                self.editing.selected_path = None;
                self.document.selection = Default::default();
            }
            3 => {
                self.cancel_transform();
                self.editing.selected_path = None;
                let (w, h) = (
                    self.document.spec.width_px as f32,
                    self.document.spec.height_px as f32,
                );
                self.document.selection.set(
                    vec![
                        Vec2::ZERO,
                        Vec2::new(w, 0.),
                        Vec2::new(w, h),
                        Vec2::new(0., h),
                    ],
                    w as usize,
                    h as usize,
                );
            }
            4 => {
                self.cancel_transform_and_deselect();
                self.shape_drag = None;
            }
            5 => self.apply_transform(),
            6 if !self.document.selection.polygon.is_empty() => {
                self.cancel_transform();
                self.finish_stroke();
                if let Ok(Some(c)) = Cutout::capture(&self.document) {
                    let mut tx = EditTransaction::default();
                    c.lift(&mut self.document, &mut tx);
                    self.bake_paths();
                    self.history.push(tx, &self.document);
                    self.tabs[self.active_tab].modified = true;
                }
            }
            _ => {}
        }
        key != 0
    }
    fn bake_paths(&mut self) {
        let active = self.document.active_layer_index();
        self.document.layers[active].vectors = Arc::new(VectorLayer::raster_base(&self.document));
        self.editing.selected_path = None;
    }
    pub(super) fn path_controls(&mut self, ui: &mut egui::Ui) {
        let layer = self.document.layers[self.document.active_layer_index()]
            .vectors
            .clone();
        if self
            .editing
            .selected_path
            .is_some_and(|i| i >= layer.strokes.len())
        {
            self.editing.selected_path = None;
        }
        egui::CollapsingHeader::new(format!("Editable paths · {}",layer.strokes.len())).default_open(false).show(ui,|ui|{
            ui.small("Click a stroke with Vector selection (V), or choose a path here to show transform handles. Save a project or editable PSD to keep paths after reopening.");
            if ui.selectable_label(self.editing.selected_path.is_none(),"All paths").clicked(){self.cancel_transform();self.document.selection=Default::default();self.editing.selected_path=None;}
            egui::ScrollArea::vertical().id_salt("path_list").max_height(155.).show(ui,|ui|{
                for (i,path) in layer.strokes.iter().enumerate().rev(){
                    if ui.selectable_label(self.editing.selected_path==Some(i),format!("{} · {}",i+1,path.label)).clicked(){self.finish_stroke();self.cancel_transform();self.document.selection=Default::default();self.editing.selected_path=Some(i);self.begin_transform(ui.ctx());}
                }
            });
            ui.small("A free selection edits pixels. Transforming or deleting a pixel region flattens this layer’s paths; Undo restores them.");
        });
    }
    pub(super) fn show_guide(&mut self, ctx: &egui::Context) {
        egui::Window::new("Tool guide").fade_in(false).fade_out(false).default_pos(Pos2::new(90.,90.)).open(&mut self.editing.guide).default_width(440.).show(ctx,|ui|{
            for (name,help) in [
                ("Pencil","Draw graphite with pressure and tilt. Hold the tip still for about 0.65 seconds to straighten the stroke, adjust its endpoint, then lift. Disable Hold to straighten in Pencil controls if preferred. Line smoothing reduces wobble; 0% switches it off."),
                ("Pencil gallery / ABR","Open Pencil gallery to import ABR tips, save current pencil settings, and organize named pencils in folders. Imported samples become graphite contact relief with the pencil's pressure, tilt and paper response. Photoshop stamp dynamics and computed brushes are not imported."),
                ("Tissue","A preloaded tissue lays down a broad, soft graphite stain. Lower Graphite load for gentle shading; repeated passes deepen it. Random graphite varies density in soft patches while keeping the chosen color."),
                ("Smudge / Eraser","Smudge moves existing graphite. Eraser lift 100% fully clears the contacted material on the active layer."),
                ("Shapes","Choose Line, Circle, Triangle or Square. Outlines use the current pencil texture and color. Shape opacity adjusts new outlines independently of layer opacity. Drag to preview and release to commit. Enable Snap shape for lines at 15-degree increments without a modifier. Hold Shift to snap lines or make triangles equilateral; circles and squares keep equal proportions. Esc cancels."),
                ("Vector selection · V","Click a stroke on the active layer to show its resize and rotation handles. Drag inside to move, corners to resize, or the round handle to rotate. Enter applies; Esc cancels. Other paths keep their geometry."),
                ("Free selection","Trace around marks and release to show transform handles automatically. Applying a pixel transform flattens paths on that layer; Cancel or Undo preserves/restores them. Ctrl+D deselects."),
                ("Orientation","Page picture buttons beside paper size set the next drawing orientation. Canvas has matching Portrait/Landscape buttons for the current drawing, including artwork. Ctrl+Z undoes it. Shift while rotating snaps to the four cardinal angles."),
                ("Ctrl+T / Ctrl+R — Transform","Choose an Editable path to move or resize its geometry and redraw the graphite. With no selection, all paths transform together. Free selections transform pixels and flatten paths on that layer. Drag inside the box to move; drag a corner to resize. Ctrl+R rotates by dragging around the center, or use the angle control. Keep aspect ratio preserves proportions when resizing. Shift also constrains resizing or snaps rotation to 90°. Enter applies; Esc cancels. Ctrl+Z restores the result."),
                ("Save and reopen","Ctrl+S saves an editable PSD or .graphite project; Ctrl+Shift+S saves a copy; Ctrl+O opens one in a tab. PSD includes native Photoshop paths and hidden native shape alternatives, plus material state for reopening here. PNG/JPEG/BMP are image exports."),
                ("EasyCanvas","Two-finger drag pans, pinch zooms, and twist rotates the paper view. P or B selects Pencil; E selects Eraser. R selects mouse view rotation; drag around the center, then press R or Esc to return. Fullscreen shows the whole drawing; Esc exits."),
            ]{ui.strong(name);ui.label(help);ui.add_space(7.);}
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn interface_frame(a: &mut GraphiteApp, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
        ctx.run_ui(egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440., 920.))),
            events, ..Default::default()
        }, |ui| a.interface(ui))
    }
    fn click_interface(a: &mut GraphiteApp, ctx: &egui::Context, pos: Pos2) {
        for pressed in [true, false] {
            interface_frame(a, ctx, vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton {
                pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default()
            }]);
        }
    }

    #[test]
    fn selecting_toolbar_tools_cancels_vector_and_pixel_transforms_and_deselects() {
        for pixels in [false, true] { for tool in ToolKind::PALETTE {
            let mut a = app();
            let ctx = egui::Context::default();
            draw(&mut a, &ctx, Pos2::new(35., 55.), Pos2::new(90., 65.));
            let before = a.renderer.rgba8(&a.document);
            let original = a.document.layers[0].vectors.clone();
            a.editing.selected_path = Some(0);
            if pixels {
                a.document.selection.set(vec![Vec2::new(20., 30.), Vec2::new(115., 30.),
                    Vec2::new(115., 90.), Vec2::new(20., 90.)], a.document.spec.width_px, a.document.spec.height_px);
            }
            let _ = ctx.run_ui(Default::default(), |ui| a.begin_transform(ui.ctx()));
            let t = a.editing.transform.as_mut().unwrap();
            t.target = Rect::from_min_size(t.target.min + Vec2::new(45., 65.), t.target.size() * 1.2);
            t.angle = 0.45;
            for _ in 0..3 { interface_frame(&mut a, &ctx, vec![]); }
            let pos = ctx.data(|d| d.get_temp::<Rect>(egui::Id::new(("test_tool_icon", tool.label()))).unwrap().center());
            click_interface(&mut a, &ctx, pos);
            assert_eq!(a.settings.tool, tool);
            assert!(a.editing.transform.is_none());
            assert!(a.editing.selected_path.is_none());
            assert!(a.document.selection.polygon.is_empty());
            assert_eq!(before, a.renderer.rgba8(&a.document));
            assert!(Arc::ptr_eq(&original, &a.document.layers[0].vectors));
            // Cancelling creates no undo entry: one undo still removes the drawing stroke.
            assert!(a.history.undo(&mut a.document));
            assert!(a.document.surface.graphite_mass.iter().all(|&m| m == 0.));
        }}
    }

    #[test]
    fn tool_shortcuts_cancel_pending_transform() {
        for key in [egui::Key::P, egui::Key::B, egui::Key::E, egui::Key::V, egui::Key::R] {
            let mut a = app();
            let ctx = egui::Context::default();
            draw(&mut a, &ctx, Pos2::new(35., 55.), Pos2::new(90., 65.));
            let original_x = a.document.layers[0].vectors.strokes[0].points[0].x;
            let _ = ctx.run_ui(Default::default(), |ui| a.begin_transform(ui.ctx()));
            let t = a.editing.transform.as_mut().unwrap();
            t.target = t.target.translate(Vec2::new(40., 25.));
            interface_frame(&mut a, &ctx, vec![egui::Event::Key { key, physical_key: None,
                pressed: true, repeat: false, modifiers: Default::default() }]);
            assert!(a.editing.transform.is_none());
            assert!(a.editing.selected_path.is_none());
            assert!(a.document.selection.polygon.is_empty());
            assert_eq!(a.document.layers[0].vectors.strokes[0].points[0].x, original_x);
        }
    }

    #[test]
    fn opening_gallery_does_not_confirm_transform_before_choosing_a_pencil() {
        let mut a = app();
        let ctx = egui::Context::default();
        draw(&mut a, &ctx, Pos2::new(35., 55.), Pos2::new(90., 65.));
        let original = a.document.layers[0].vectors.clone();
        let before = a.renderer.rgba8(&a.document);
        let _ = ctx.run_ui(Default::default(), |ui| a.begin_transform(ui.ctx()));
        let t = a.editing.transform.as_mut().unwrap();
        t.target = t.target.translate(Vec2::new(40., 25.));
        a.open_pencil_gallery();
        assert!(a.editing.transform.is_some());
        a.restore_pencil_preferences(); // Uses the same activation path as a gallery tile.
        assert!(a.editing.transform.is_none());
        assert_eq!(before, a.renderer.rgba8(&a.document));
        assert!(Arc::ptr_eq(&original, &a.document.layers[0].vectors));
    }

    #[test]
    fn transform_check_and_cross_work_for_vectors_and_pixels_with_panels_hidden() {
        for pixels in [false, true] { for confirm in [false, true] {
            let mut a = app();
            let ctx = egui::Context::default();
            draw(&mut a, &ctx, Pos2::new(35., 55.), Pos2::new(90., 65.));
            let before = a.document.surface.graphite_mass.clone();
            let original = a.document.layers[0].vectors.clone();
            a.editing.selected_path = Some(0);
            if pixels {
                a.document.selection.set(vec![Vec2::new(20., 30.), Vec2::new(115., 30.),
                    Vec2::new(115., 90.), Vec2::new(20., 90.)], a.document.spec.width_px, a.document.spec.height_px);
            }
            let ctx = egui::Context::default();
            let mut preview = crate::ui::test_render::Preview::default();
            let initial = ctx.run_ui(Default::default(), |ui| a.begin_transform(ui.ctx()));
            preview.update(&initial);
            let t = a.editing.transform.as_mut().unwrap();
            assert_eq!(t.vectors.is_none(), pixels);
            t.target = t.target.translate(Vec2::new(45., 60.));
            a.material_panel_visible = false;
            a.layers_panel_visible = false;
            a.fullscreen = pixels != confirm;
            for pass in 0..3 {
                let output = interface_frame(&mut a, &ctx, vec![]);
                preview.update(&output);
                if pass == 2 && !pixels && !confirm {
                    preview.save(&ctx, &output, "../../ui-v229-transform.png", [1440, 920]);
                }
            }
            let rect = ctx.data(|d| d.get_temp::<Rect>(egui::Id::new(("test_transform_action", confirm))).unwrap());
            assert!(rect.height() >= 44. && rect.left() >= 0. && rect.right() < 1440.);
            click_interface(&mut a, &ctx, rect.center());
            assert!(a.editing.transform.is_none());
            if confirm {
                assert_ne!(before, a.document.surface.graphite_mass);
                assert!(a.history.undo(&mut a.document));
            } else {
                assert!(a.document.selection.polygon.is_empty());
                assert!(a.editing.selected_path.is_none());
            }
            assert_eq!(before, a.document.surface.graphite_mass);
            assert!(Arc::ptr_eq(&original, &a.document.layers[0].vectors));
        }}
    }

    #[test]
    fn escape_cancels_transform_and_selection_in_normal_and_fullscreen_views() {
        for pixels in [false, true] { for fullscreen in [false, true] {
            let mut a = app();
            let ctx = egui::Context::default();
            draw(&mut a, &ctx, Pos2::new(35., 55.), Pos2::new(90., 65.));
            let before = a.renderer.rgba8(&a.document);
            let original = a.document.layers[0].vectors.clone();
            a.editing.selected_path = Some(0);
            if pixels {
                a.document.selection.set(vec![Vec2::new(20., 30.), Vec2::new(115., 30.),
                    Vec2::new(115., 90.), Vec2::new(20., 90.)], a.document.spec.width_px, a.document.spec.height_px);
            }
            let _ = ctx.run_ui(Default::default(), |ui| a.begin_transform(ui.ctx()));
            let t = a.editing.transform.as_mut().unwrap();
            t.target = t.target.translate(Vec2::new(40., 25.));
            a.fullscreen = fullscreen;
            interface_frame(&mut a, &ctx, vec![egui::Event::Key { key: egui::Key::Escape,
                physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }]);
            assert!(!a.fullscreen);
            assert!(a.editing.transform.is_none());
            assert!(a.editing.selected_path.is_none());
            assert!(a.document.selection.polygon.is_empty());
            assert_eq!(before, a.renderer.rgba8(&a.document));
            assert!(Arc::ptr_eq(&original, &a.document.layers[0].vectors));
        }}
    }

    #[test]
    fn aspect_lock_and_shift_resize_vectors_and_pixels_on_rotated_paper() {
        for pixels in [false, true] {
          for (lock, shift) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut a = app();
            let ctx = egui::Context::default();
            draw(&mut a, &ctx, Pos2::new(50., 80.), Pos2::new(130., 100.));
            let before = a.renderer.rgba8(&a.document);
            if pixels {
                a.document.selection.set(vec![Vec2::new(30., 60.), Vec2::new(160., 60.), Vec2::new(160., 130.), Vec2::new(30., 130.)], a.document.spec.width_px, a.document.spec.height_px);
            }
            a.settings.transform_keep_aspect = lock;
            a.viewport.rotation = 0.31;
            let _ = ctx.run_ui(Default::default(), |ui| a.begin_transform(ui.ctx()));
            let t = a.editing.transform.as_mut().unwrap();
            assert_eq!(t.vectors.is_none(), pixels);
            t.angle = 0.23;
            let original = t.target;
            let angle = t.angle;
            let canvas = Rect::from_min_size(Pos2::ZERO, Vec2::splat(236.));
            let from = vector::rotate(original.right_bottom(), original.center(), angle);
            let to = vector::rotate(original.right_bottom() + Vec2::new(15., 30.), original.center(), angle);
            for (point, pressed, released) in [(from, true, false), (to, false, false), (to, false, true)] {
                let screen = a.viewport.document_to_screen(canvas, point.to_vec2());
                let _ = ctx.run_ui(egui::RawInput { modifiers: egui::Modifiers { shift, ..Default::default() }, ..Default::default() }, |ui| a.handle_drawing_input(ui, canvas, PointerFrame {
                    position: Some(screen), primary_pressed: pressed, primary_down: !released, primary_released: released, ..Default::default()
                }));
            }
            let result = a.editing.transform.as_ref().unwrap().target;
            assert!(result.height() > original.height() + 10.);
            let ratio_error = (result.aspect_ratio() - original.aspect_ratio()).abs();
            if lock || shift { assert!(ratio_error < 0.001); } else { assert!(ratio_error > 0.1); }
            a.apply_transform();
            assert_ne!(a.renderer.rgba8(&a.document), before);
            assert!(a.history.undo(&mut a.document));
            assert_eq!(a.renderer.rgba8(&a.document), before);
          }
        }
    }

    #[test]
    fn click_vector_selects_only_one_path_with_working_resize_and_rotation_handles() {
        let mut a = app();
        let ctx = egui::Context::default();
        draw(&mut a, &ctx, Pos2::new(35., 55.), Pos2::new(90., 65.));
        draw(&mut a, &ctx, Pos2::new(135., 155.), Pos2::new(195., 165.));
        let untouched = a.document.layers[0].vectors.strokes[1].clone();
        let before = a.document.surface.graphite_mass.clone();
        a.settings.tool = ToolKind::VectorSelect;
        draw(&mut a, &ctx, Pos2::new(55., 59.), Pos2::new(55., 59.));
        assert_eq!(a.editing.selected_path, Some(0));
        assert!(a.editing.transform.as_ref().unwrap().vectors.is_some());
        let original = a.editing.transform.as_ref().unwrap().target;
        let grip = original.right_bottom();
        draw(&mut a, &ctx, grip, grip + Vec2::new(20., 10.));
        let t = a.editing.transform.as_ref().unwrap();
        assert!(t.target.width() > original.width() + 15.);
        let center = t.target.center();
        let knob = t.target.center_top() - Vec2::new(0., 24.);
        let destination = vector::rotate(knob, center, std::f32::consts::FRAC_PI_2);
        draw(&mut a, &ctx, knob, destination);
        assert!(
            (a.editing.transform.as_ref().unwrap().angle - std::f32::consts::FRAC_PI_2).abs()
                < 0.001
        );
        a.apply_transform();
        assert!(Arc::ptr_eq(
            &untouched,
            &a.document.layers[0].vectors.strokes[1]
        ));
        assert_ne!(before, a.document.surface.graphite_mass);
        assert!(a.history.undo(&mut a.document));
        assert_eq!(before, a.document.surface.graphite_mass);
        draw(&mut a, &ctx, Pos2::new(155., 159.), Pos2::new(155., 159.));
        assert_eq!(a.editing.selected_path, Some(1));
        a.cancel_transform();
        let ctx = egui::Context::default();
        let mut preview = crate::ui::test_render::Preview::default();
        let initial = ctx.run_ui(Default::default(), |ui| a.begin_transform(ui.ctx()));
        preview.update(&initial);
        for i in 0..3 {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440., 920.))),
                    ..Default::default()
                },
                |ui| a.interface(ui),
            );
            preview.update(&output);
            if i == 2 {
                preview.save(&ctx, &output, "../../ui-v22-selection.png", [1440, 920]);
            }
        }
        a.cancel_transform();
        assert_eq!(before, a.document.surface.graphite_mass);
    }

    #[test]
    fn completing_lasso_automatically_offers_transform_without_committing_pixels() {
        let mut a = app();
        let ctx = egui::Context::default();
        draw(&mut a, &ctx, Pos2::new(60., 80.), Pos2::new(120., 100.));
        let before = a.document.surface.graphite_mass.clone();
        a.settings.tool = ToolKind::Lasso;
        let canvas = Rect::from_min_size(Pos2::ZERO, Vec2::splat(236.));
        for (i, p) in [
            Pos2::new(40., 60.),
            Pos2::new(150., 60.),
            Pos2::new(150., 120.),
            Pos2::new(40., 120.),
        ]
        .into_iter()
        .enumerate()
        {
            let _ = ctx.run_ui(Default::default(), |ui| {
                a.handle_drawing_input(
                    ui,
                    canvas,
                    PointerFrame {
                        position: Some(p),
                        primary_pressed: i == 0,
                        primary_down: true,
                        ..Default::default()
                    },
                )
            });
        }
        let _ = ctx.run_ui(Default::default(), |ui| {
            a.handle_drawing_input(
                ui,
                canvas,
                PointerFrame {
                    primary_released: true,
                    ..Default::default()
                },
            )
        });
        assert!(a.editing.transform.is_some());
        assert!(a.editing.transform.as_ref().unwrap().vectors.is_none());
        a.cancel_transform();
        assert_eq!(before, a.document.surface.graphite_mass);
        assert_eq!(a.document.layers[0].vectors.strokes.len(), 1);
    }

    #[test]
    fn paper_settings_orientation_buttons_rotate_existing_artwork_and_undo() {
        fn frame(a: &mut GraphiteApp, ctx: &egui::Context, events: Vec<egui::Event>) {
            let _ = ctx.run_ui(egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440., 920.))),
                events,
                ..Default::default()
            }, |ui| a.interface(ui));
        }
        fn click(a: &mut GraphiteApp, ctx: &egui::Context, label: &str) {
            frame(a, ctx, vec![]);
            let zoom=a.viewport.zoom;
            let id=if a.paper_settings.open {egui::Id::new(("paper_orientation",label=="Landscape"))}else{egui::Id::new(("test_action_icon",label))};
            let pos = ctx.data(|d| d.get_temp::<Rect>(id).unwrap()).center();
            for pressed in [true, false] {
                frame(a, ctx, vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton {
                    pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default(),
                }]);
            }
            frame(a,ctx,vec![]);
            assert_eq!(a.viewport.zoom,zoom,"Orientation must preserve the current zoom");
        }
        let mut a = app();
        a.replace_document(CanvasSpec::from_physical("Portrait", 40., 60., 120.));
        let ctx = egui::Context::default();
        draw(&mut a, &ctx, Pos2::new(50., 60.), Pos2::new(120., 100.));
        let original = a.renderer.rgba8(&a.document);
        let dimensions = (a.document.spec.width_px, a.document.spec.height_px);
        let tab_count = a.tabs.len();
        a.paper_settings.open = true;
        for _ in 0..3 { frame(&mut a, &ctx, vec![]); }
        a.viewport.zoom=2.35;
        click(&mut a, &ctx, "Landscape");
        assert_eq!((a.document.spec.width_px, a.document.spec.height_px), (dimensions.1, dimensions.0));
        assert_eq!(a.tabs.len(), tab_count);
        assert_eq!(a.document.layers[0].vectors.strokes.len(), 1);
        assert!(a.new_landscape);
        let rotated = a.renderer.rgba8(&a.document);
        a.viewport.rotation=0.8;
        click(&mut a, &ctx, "Landscape");
        assert_eq!(a.viewport.rotation,0.);
        assert_eq!(a.renderer.rgba8(&a.document), rotated);
        a.handle_topbar_action(TopbarAction::Undo);
        assert_eq!(a.renderer.rgba8(&a.document), original);
        a.handle_topbar_action(TopbarAction::Redo);
        click(&mut a, &ctx, "Portrait");
        assert_eq!((a.document.spec.width_px, a.document.spec.height_px), dimensions);
        assert_eq!(a.renderer.rgba8(&a.document), original);
        assert!(!a.new_landscape);
        // The same controls are directly available on the main screen.
        a.paper_settings.open=false;
        for _ in 0..3{frame(&mut a,&ctx,vec![]);}
        a.viewport.rotation=0.9;
        a.viewport.zoom=4.25;
        click(&mut a,&ctx,"Landscape");
        assert_eq!(a.viewport.rotation,0.);assert!(a.document.spec.width_px>a.document.spec.height_px);
        a.viewport.rotation=-0.7;
        click(&mut a,&ctx,"Landscape");assert_eq!(a.viewport.rotation,0.);
        click(&mut a,&ctx,"Portrait");assert_eq!(a.renderer.rgba8(&a.document),original);
    }

    #[test]
    fn orientation_supports_new_drawings_and_current_project_roundtrip() {
        let mut a = app();
        let ctx = egui::Context::default();
        a.new_landscape = true;
        a.handle_topbar_action(TopbarAction::NewCanvas {
            preset: CanvasPreset::A5,
            dpi: 36.,
        });
        assert!(a.document.spec.width_px > a.document.spec.height_px);
        draw(&mut a, &ctx, Pos2::new(50., 60.), Pos2::new(120., 100.));
        let original = a.renderer.rgba8(&a.document);
        a.handle_topbar_action(TopbarAction::CanvasOrientation(false));
        assert!(a.document.spec.width_px < a.document.spec.height_px);
        assert_eq!(a.document.layers[0].vectors.strokes.len(), 1);
        for extension in ["graphite", "psd"] {
            let path = std::path::PathBuf::from(format!("../../portrait-v22.{extension}"));
            a.save_project_path(&path).unwrap();
            let loaded = graphite_studio::project::load(&path).unwrap();
            assert_eq!(loaded.document.spec.width_px, a.document.spec.width_px);
            assert_eq!(loaded.document.spec.height_px, a.document.spec.height_px);
            assert_eq!(loaded.document.layers[0].vectors.strokes.len(), 1);
            assert_eq!(
                a.renderer.rgba8(&a.document),
                crate::render::RasterRenderer::default().rgba8(&loaded.document)
            );
        }
        a.handle_topbar_action(TopbarAction::Undo);
        assert!(a.document.spec.width_px > a.document.spec.height_px);
        assert_eq!(a.renderer.rgba8(&a.document), original);
        a.handle_topbar_action(TopbarAction::Redo);
        assert!(a.document.spec.width_px < a.document.spec.height_px);
    }
    #[test]
    fn pencil_shapes_keep_texture_color_opacity_and_editable_roundtrips() {
        use crate::render::layer_pixel_rgba;
        let ctx = egui::Context::default();
        let mut a = app();
        a.settings.tool = ToolKind::Shapes;
        a.settings.shape_width_px = 14.;
        a.settings.pencil_color_rgb = [200, 50, 25];
        a.settings.particle_variation = 0.;
        a.settings.flow = 0.8;
        for kind in crate::core::shapes::ShapeKind::ALL {
            a.replace_document(CanvasSpec::from_physical("Shape test", 50., 50., 120.));
            a.settings.tool = ToolKind::Shapes;
            a.settings.shape = kind;
            a.settings.shape_opacity = 1.;
            let before = a.document.clone();
            draw(&mut a, &ctx, Pos2::new(50., 60.), Pos2::new(170., 150.));
            let stroke = a.document.layers[0].vectors.strokes[0].clone();
            assert_eq!(stroke.settings.tool, ToolKind::Pencil);
            assert_eq!(stroke.settings.flow, a.settings.flow);
            assert_eq!(stroke.settings.grade, a.settings.grade);
            assert_eq!(stroke.settings.tip_sharpness, a.settings.tip_sharpness);
            let full = a.document.clone();
            // Independent pencil-engine execution must produce exactly the same grain.
            let mut manual = before.clone();
            let mut engine = stroke.engine.clone();
            let mut tip = stroke.tip.clone();
            engine.begin_pencil_stroke(stroke.points[0]);
            for pair in stroke.points.windows(2) {
                engine.apply_segment(
                    &mut manual,
                    &stroke.settings,
                    &mut tip,
                    pair[0],
                    pair[1],
                    &mut EditTransaction::default(),
                );
            }
            assert_eq!(manual.surface.graphite_mass, full.surface.graphite_mass);
            let mut half_stroke = (*stroke).clone();
            half_stroke.settings.shape_opacity = 0.5;
            let mut half = before.clone();
            half_stroke.apply(&mut half, &mut EditTransaction::default());
            let mut touched = 0;
            for i in 0..full.spec.pixel_count() {
                let f = layer_pixel_rgba(&full, Some(0), i);
                let h = layer_pixel_rgba(&half, Some(0), i);
                assert!((h[3] - f[3] * 0.5).abs() < 2e-6);
                if f[3] > 0.001 {
                    touched += 1;
                    for c in 0..3 {
                        assert!((f[c] - a.settings.pencil_color_rgb[c] as f32 / 255.).abs() < 2e-5);
                        assert!((f[c] - h[c]).abs() < 2e-5);
                    }
                }
            }
            assert!(touched > 100);
            half_stroke.settings.shape_opacity = 0.;
            let existing = full.surface.clone();
            let mut tx = EditTransaction::default();
            half_stroke.apply(&mut a.document, &mut tx);
            assert!(tx.is_empty());
            assert_eq!(a.document.surface.graphite_mass, existing.graphite_mass);
            assert_eq!(a.document.surface.current_height, existing.current_height);
            assert!(a.history.undo(&mut a.document));
            assert_eq!(
                a.document.surface.graphite_mass,
                before.surface.graphite_mass
            );
            assert!(a.history.redo(&mut a.document));
            assert_eq!(a.document.surface.graphite_mass, full.surface.graphite_mass);
        }
        a.settings.shape_opacity = 0.37;
        draw(&mut a, &ctx, Pos2::new(65., 65.), Pos2::new(180., 155.));
        let record = a.document.layers[0].vectors.strokes.last().unwrap().clone();
        let from = Rect::from_min_size(Pos2::ZERO, Vec2::splat(236.));
        let resized = record.transformed(from, Rect::from_min_size(Pos2::ZERO, Vec2::splat(472.)));
        assert_eq!(resized.settings.shape_opacity, 0.37);
        assert_eq!(
            resized.settings.shape_width_px,
            record.settings.shape_width_px * 2.
        );
        for extension in ["graphite", "psd"] {
            let name = format!("../../shapes-v21.{extension}");
            let path = std::path::Path::new(&name);
            a.save_project_path(path).unwrap();
            let reopened = graphite_studio::project::load(path).unwrap();
            assert_eq!(reopened.settings.shape_opacity, 0.37);
            assert_eq!(
                reopened.document.layers[0]
                    .vectors
                    .strokes
                    .last()
                    .unwrap()
                    .opacity(),
                0.37
            );
            assert_eq!(
                a.document.surface.graphite_mass,
                reopened.document.surface.graphite_mass
            );
        }
    }

    #[test]
    fn rotated_canvas_draws_at_document_coordinates_and_rejects_outside_paper() {
        let mut a = app();
        a.viewport.rotation = 0.73;
        a.viewport.zoom = 1.;
        let canvas = Rect::from_min_size(Pos2::ZERO, Vec2::splat(236.));
        let ctx = egui::Context::default();
        let first = Vec2::new(70., 80.);
        let end = Vec2::new(130., 110.);
        let first_screen = a.viewport.document_to_screen(canvas, first);
        let end_screen = a.viewport.document_to_screen(canvas, end);
        draw(&mut a, &ctx, first_screen, end_screen);
        let record = &a.document.layers[0].vectors.strokes[0];
        assert!((Vec2::new(record.points[0].x, record.points[0].y) - first).length() < 0.001);
        assert!((Vec2::new(record.points[1].x, record.points[1].y) - end).length() < 0.001);
        let before = a.document.surface.graphite_mass.clone();
        // Bounding rectangle corner is outside the rotated sheet.
        draw(&mut a, &ctx, Pos2::new(1., 1.), Pos2::new(2., 2.));
        assert_eq!(before, a.document.surface.graphite_mass);
        a.settings.tool = ToolKind::Pencil;
        let point = a.make_stroke_point(
            first_screen,
            canvas,
            0.5,
            true,
            None,
            Some(45.),
            Some(90.),
            Some(15.),
        );
        assert!((point.azimuth_deg - (90. - 0.73f32.to_degrees())).abs() < 0.001);
        assert_eq!(point.rotation_deg, Some(15.));
    }

    #[test]
    fn tool_shortcuts_view_rotation_and_fullscreen_escape_are_separate() {
        let mut a = app();
        let ctx = egui::Context::default();
        let key = |a: &mut GraphiteApp, key, modifiers| {
            let _ = ctx.run_ui(
                egui::RawInput {
                    modifiers,
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    }],
                    ..Default::default()
                },
                |ui| a.handle_keyboard_shortcuts(ui),
            );
            let _ = ctx.run_ui(Default::default(), |_| {});
        };
        key(&mut a, egui::Key::P, egui::Modifiers::NONE);
        assert_eq!(a.settings.tool, ToolKind::Pencil);
        key(&mut a, egui::Key::E, egui::Modifiers::NONE);
        assert_eq!(a.settings.tool, ToolKind::Eraser);
        key(&mut a, egui::Key::B, egui::Modifiers::CTRL);
        assert_eq!(a.settings.tool, ToolKind::Eraser);
        key(&mut a, egui::Key::B, egui::Modifiers::NONE);
        assert_eq!(a.settings.tool, ToolKind::Pencil);
        key(&mut a, egui::Key::R, egui::Modifiers::NONE);
        assert!(a.rotate_view && a.editing.transform.is_none());
        key(&mut a, egui::Key::P, egui::Modifiers::NONE);
        assert!(!a.rotate_view);
        a.settings.tool = ToolKind::Brush;
        draw(&mut a, &ctx, Pos2::new(60., 60.), Pos2::new(170., 130.));
        key(&mut a, egui::Key::R, egui::Modifiers::CTRL);
        assert!(a.editing.transform.as_ref().unwrap().rotate_mode);
        assert!(!a.rotate_view);
        a.cancel_transform();
        let pigment = a.document.surface.graphite_mass.clone();
        a.viewport.rotation = 0.64;
        a.handle_topbar_action(TopbarAction::Fullscreen);
        let mut preview = crate::ui::test_render::Preview::default();
        for pass in 0..3 {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200., 800.))),
                    ..Default::default()
                },
                |ui| a.interface(ui),
            );
            preview.update(&output);
            if pass == 0 {
                assert!(output.viewport_output.values().any(|v| v
                    .commands
                    .iter()
                    .any(|c| matches!(c, egui::ViewportCommand::Fullscreen(true)))));
            }
            if pass == 2 {
                preview.save(&ctx, &output, "../../ui-v21-fullscreen.png", [1200, 800]);
            }
        }
        assert!(a.fullscreen && a.fullscreen_applied);
        let bounds = a
            .viewport
            .layout(a.fullscreen_size.unwrap(), Vec2::splat(236.));
        assert!(!bounds.overflow[0] && !bounds.overflow[1]);
        key(&mut a, egui::Key::Escape, egui::Modifiers::NONE);
        assert!(!a.fullscreen);
        let output = ctx.run_ui(Default::default(), |ui| a.interface(ui));
        assert!(output.viewport_output.values().any(|v| v
            .commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Fullscreen(false)))));
        assert_eq!(pigment, a.document.surface.graphite_mass);
    }
    pub(super) fn app() -> GraphiteApp {
        let mut app = GraphiteApp::with_render_state(None);
        app.replace_document(CanvasSpec::from_physical("Editing", 50., 50., 120.));
        app.viewport.zoom = 1.;
        app.settings.tool = ToolKind::Brush;
        app.settings.brush_size_px = 12.;
        app
    }
    pub(super) fn draw(app: &mut GraphiteApp, ctx: &egui::Context, a: Pos2, b: Pos2) {
        let canvas = Rect::from_min_size(Pos2::ZERO, Vec2::splat(236.));
        for frame in [
            PointerFrame {
                position: Some(a),
                primary_pressed: true,
                primary_down: true,
                pressure: 0.4,
                pressure_from_device: true,
                rotation_deg: Some(35.),
                ..Default::default()
            },
            PointerFrame {
                position: Some(b),
                primary_down: true,
                pressure: 0.8,
                pressure_from_device: true,
                rotation_deg: Some(65.),
                ..Default::default()
            },
            PointerFrame {
                primary_released: true,
                ..Default::default()
            },
        ] {
            let _ = ctx.run_ui(Default::default(), |ui| {
                app.handle_drawing_input(ui, canvas, frame)
            });
        }
    }
    #[test]
    fn vector_transform_retains_pressure_and_is_undoable_without_disturbing_paper() {
        let mut app = app();
        let ctx = egui::Context::default();
        draw(&mut app, &ctx, Pos2::new(30., 40.), Pos2::new(85., 45.));
        let active = app.document.active_layer_index();
        let original = app.document.layers[active].vectors.clone();
        assert_eq!(original.strokes.len(), 1);
        assert_eq!(original.strokes[0].points[1].pressure, 0.8);
        assert_eq!(original.strokes[0].points[1].rotation_deg, Some(65.));
        let before = app.document.surface.graphite_mass.clone();
        let paper = app.document.surface.current_height.clone();
        let _ = ctx.run_ui(Default::default(), |ui| app.begin_transform(ui.ctx()));
        let t = app.editing.transform.as_mut().unwrap();
        assert!(t.vectors.is_some());
        let old = t.cutout.bounds;
        let new = Rect::from_min_size(old.min + Vec2::new(25., 60.), old.size() * 2.);
        t.target = new;
        app.apply_transform();
        let changed = &app.document.layers[active].vectors.strokes[0];
        assert_eq!(changed.points[1].pressure, 0.8);
        assert_eq!(changed.settings.brush_size_px, 24.);
        assert!(changed.points[0].y > 90.);
        assert_eq!(paper, app.document.surface.current_height);
        assert_ne!(before, app.document.surface.graphite_mass);
        let after = app.document.surface.graphite_mass.clone();
        assert!(app.history.undo(&mut app.document));
        assert_eq!(before, app.document.surface.graphite_mass);
        assert!(Arc::ptr_eq(&original, &app.document.layers[active].vectors));
        assert!(app.history.redo(&mut app.document));
        assert_eq!(after, app.document.surface.graphite_mass);
    }
    #[test]
    fn individual_path_cancel_and_commit_preserve_other_stroke_and_layer() {
        let mut app = app();
        app.settings.tool = ToolKind::Pencil;
        let ctx = egui::Context::default();
        draw(&mut app, &ctx, Pos2::new(25., 30.), Pos2::new(70., 30.));
        draw(&mut app, &ctx, Pos2::new(100., 150.), Pos2::new(160., 150.));
        app.document.add_layer();
        draw(&mut app, &ctx, Pos2::new(20., 190.), Pos2::new(90., 190.));
        let other = app.document.surface.graphite_mass.clone();
        app.document.activate_layer(0);
        let before = app.document.surface.graphite_mass.clone();
        app.editing.selected_path = Some(0);
        let _ = ctx.run_ui(Default::default(), |ui| app.begin_transform(ui.ctx()));
        app.cancel_transform();
        assert_eq!(before, app.document.surface.graphite_mass);
        let _ = ctx.run_ui(Default::default(), |ui| app.begin_transform(ui.ctx()));
        let t = app.editing.transform.as_mut().unwrap();
        t.target = t.target.translate(Vec2::new(0., 55.));
        app.apply_transform();
        assert_eq!(app.document.layers[0].vectors.strokes[1].points[0].y, 150.);
        let w = app.document.spec.width_px;
        for y in 140..160 {
            assert_eq!(
                &before[y * w..(y + 1) * w],
                &app.document.surface.graphite_mass[y * w..(y + 1) * w]
            );
        }
        app.document.activate_layer(1);
        assert_eq!(other, app.document.surface.graphite_mass);
    }
    #[test]
    fn lasso_clips_all_deposition_tools_and_pixel_transform_undo_restores_paths() {
        let mut app = app();
        let ctx = egui::Context::default();
        let (w, h) = (app.document.spec.width_px, app.document.spec.height_px);
        app.document.selection.set(
            vec![
                Vec2::new(60., 20.),
                Vec2::new(100., 20.),
                Vec2::new(100., 90.),
                Vec2::new(60., 90.),
            ],
            w,
            h,
        );
        for tool in [ToolKind::Pencil, ToolKind::Brush, ToolKind::Tissue] {
            app.settings.tool = tool;
            draw(&mut app, &ctx, Pos2::new(30., 50.), Pos2::new(140., 50.));
        }
        for y in 0..h {
            for x in 0..w {
                let i = app.document.index(x, y);
                if !(60..100).contains(&x) {
                    assert_eq!(app.document.surface.total_deposit(i), 0.);
                }
            }
        }
        let original = app.document.layers[0].vectors.clone();
        assert_eq!(original.strokes.len(), 3);
        let _ = ctx.run_ui(Default::default(), |ui| app.begin_transform(ui.ctx()));
        let t = app.editing.transform.as_mut().unwrap();
        assert!(t.vectors.is_none());
        t.target = t.target.translate(Vec2::new(20., 60.));
        app.apply_transform();
        assert!(app.document.layers[0].vectors.strokes.is_empty());
        assert!(app.history.undo(&mut app.document));
        assert!(Arc::ptr_eq(&original, &app.document.layers[0].vectors));
    }
    #[test]
    fn eraser_paths_can_move_and_lasso_protects_unselected_material() {
        let mut app = app();
        let ctx = egui::Context::default();
        app.settings.brush_size_px = 28.;
        draw(&mut app, &ctx, Pos2::new(30., 80.), Pos2::new(190., 80.));
        app.settings.tool = ToolKind::Eraser;
        app.settings.eraser_strength = 1.;
        app.settings.eraser_diameter_mm = 4.;
        draw(&mut app, &ctx, Pos2::new(80., 65.), Pos2::new(80., 95.));
        let hole = app.document.index(80, 80);
        assert_eq!(app.document.surface.total_deposit(hole), 0.);
        app.editing.selected_path = Some(1);
        let _ = ctx.run_ui(Default::default(), |ui| app.begin_transform(ui.ctx()));
        let t = app.editing.transform.as_mut().unwrap();
        assert!(t.vectors.is_some());
        t.target = t.target.translate(Vec2::new(50., 0.));
        app.apply_transform();
        assert!(app.document.surface.total_deposit(hole) > 0.);
        assert_eq!(
            app.document
                .surface
                .total_deposit(app.document.index(130, 80)),
            0.
        );
        app.editing.selected_path = None;
        let (w, h) = (app.document.spec.width_px, app.document.spec.height_px);
        app.document.selection.set(
            vec![
                Vec2::new(60., 30.),
                Vec2::new(100., 30.),
                Vec2::new(100., 120.),
                Vec2::new(60., 120.),
            ],
            w,
            h,
        );
        let before = app.document.surface.graphite_mass.clone();
        for tool in [ToolKind::Eraser, ToolKind::Smudge] {
            app.settings.tool = tool;
            draw(&mut app, &ctx, Pos2::new(30., 80.), Pos2::new(190., 80.));
        }
        for y in 0..h {
            for x in 0..w {
                if !(60..100).contains(&x) {
                    let i = app.document.index(x, y);
                    assert_eq!(app.document.surface.graphite_mass[i], before[i]);
                }
            }
        }
    }
    #[test]
    fn ctrl_r_rotates_selected_path_with_pointer_then_undo_restores_it() {
        let mut app = app();
        let ctx = egui::Context::default();
        draw(&mut app, &ctx, Pos2::new(40., 80.), Pos2::new(120., 80.));
        app.editing.selected_path = Some(0);
        let _ = ctx.run_ui(
            egui::RawInput {
                modifiers: egui::Modifiers::CTRL,
                events: vec![egui::Event::Key {
                    key: egui::Key::R,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::CTRL,
                }],
                ..Default::default()
            },
            |ui| {
                assert!(app.editing_shortcuts(ui));
            },
        );
        let t = app.editing.transform.as_ref().unwrap();
        assert!(t.rotate_mode);
        let center = t.target.center();
        let canvas = Rect::from_min_size(Pos2::ZERO, Vec2::splat(236.));
        for frame in [
            PointerFrame {
                position: Some(center + Vec2::new(45., 0.)),
                primary_pressed: true,
                primary_down: true,
                ..Default::default()
            },
            PointerFrame {
                position: Some(center + Vec2::new(0., 45.)),
                primary_down: true,
                ..Default::default()
            },
            PointerFrame {
                primary_released: true,
                ..Default::default()
            },
        ] {
            let _ = ctx.run_ui(Default::default(), |ui| app.edit_input(ui, canvas, frame));
        }
        assert!(
            (app.editing.transform.as_ref().unwrap().angle - std::f32::consts::FRAC_PI_2).abs()
                < 1e-5
        );
        app.apply_transform();
        let path = &app.document.layers[0].vectors.strokes[0];
        assert!((path.points[0].x - path.points[1].x).abs() < 0.001);
        assert!(path.points[1].y > path.points[0].y);
        assert!(app.history.undo(&mut app.document));
        assert_eq!(app.document.layers[0].vectors.strokes[0].points[0].y, 80.);
    }
    #[test]
    fn saved_project_reopens_in_new_app_and_remains_transformable() {
        let mut first = app();
        let ctx = egui::Context::default();
        draw(&mut first, &ctx, Pos2::new(30., 40.), Pos2::new(100., 40.));
        let path =
            std::env::temp_dir().join(format!("graphite-restart-{}.graphite", std::process::id()));
        first.save_project_path(&path).unwrap();
        let before = first.document.surface.graphite_mass.clone();
        drop(first);
        let mut next = app();
        next.open_project_path(&path).unwrap();
        assert_eq!(before, next.document.surface.graphite_mass);
        assert_eq!(next.document.layers[0].vectors.strokes.len(), 1);
        assert!(!next.tabs[next.active_tab].modified);
        let _ = ctx.run_ui(Default::default(), |ui| next.begin_rotation(ui.ctx()));
        next.editing.transform.as_mut().unwrap().angle = 45f32.to_radians();
        next.apply_transform();
        assert!(next.tabs[next.active_tab].modified);
        next.save_project_path(&path).unwrap();
        let reopened = graphite_studio::project::load(&path).unwrap();
        assert!(reopened.document.layers[0].vectors.strokes[0].points[1].y > 40.);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn pixel_selection_rotation_keeps_paper_and_can_be_cancelled_or_undone() {
        let mut app = app();
        let ctx = egui::Context::default();
        draw(&mut app, &ctx, Pos2::new(40., 80.), Pos2::new(120., 80.));
        let before = app.document.surface.graphite_mass.clone();
        let paper = app.document.surface.current_height.clone();
        let (w, h) = (app.document.spec.width_px, app.document.spec.height_px);
        app.document.selection.set(
            vec![
                Vec2::new(20., 50.),
                Vec2::new(150., 50.),
                Vec2::new(150., 110.),
                Vec2::new(20., 110.),
            ],
            w,
            h,
        );
        let _ = ctx.run_ui(Default::default(), |ui| app.begin_rotation(ui.ctx()));
        assert!(app.editing.transform.as_ref().unwrap().vectors.is_none());
        app.editing.transform.as_mut().unwrap().angle = 1.2;
        app.cancel_transform();
        assert_eq!(before, app.document.surface.graphite_mass);
        let _ = ctx.run_ui(Default::default(), |ui| app.begin_rotation(ui.ctx()));
        app.editing.transform.as_mut().unwrap().angle = std::f32::consts::FRAC_PI_2;
        app.apply_transform();
        let c = Cutout::capture(&app.document).unwrap().unwrap();
        assert!(c.height > c.width * 3);
        assert_eq!(paper, app.document.surface.current_height);
        assert!(app.history.undo(&mut app.document));
        assert_eq!(before, app.document.surface.graphite_mass);
    }
    #[test]
    fn tissue_builds_a_soft_colored_stain_and_zero_load_is_noop() {
        let mut app = app();
        let ctx = egui::Context::default();
        app.settings.tool = ToolKind::Tissue;
        app.settings.pencil_color_rgb = [180, 40, 20];
        app.settings.tissue_size_px = 80.;
        app.settings.tissue_load = 0.;
        draw(&mut app, &ctx, Pos2::new(100., 100.), Pos2::new(100., 100.));
        assert!(app.document.surface.graphite_mass.iter().all(|&m| m == 0.));
        app.settings.tissue_load = 0.6;
        draw(&mut app, &ctx, Pos2::new(100., 100.), Pos2::new(100., 100.));
        let center = app.document.index(100, 100);
        let edge = app.document.index(132, 100);
        assert!(
            app.document.surface.total_deposit(center) > app.document.surface.total_deposit(edge)
        );
        assert!(app.document.surface.total_deposit(edge) > 0.);
        let red = app.document.surface.color_r_mass[center];
        let green = app.document.surface.color_g_mass[center];
        assert!((red / green - 4.5).abs() < 0.001);
    }
    #[test]
    fn shape_checkbox_and_shift_control_saved_geometry_on_rotated_paper() {
        for kind in crate::core::shapes::ShapeKind::ALL {
            for snap in [false, true] {
              for checkbox in [false, true] {
                let mut a = app();
                let ctx = egui::Context::default();
                a.settings.tool = ToolKind::Shapes;
                a.settings.shape = kind;
                a.settings.shape_line_snap = checkbox;
                a.viewport.rotation = 0.47;
                a.viewport.zoom = 1.2;
                let canvas = Rect::from_min_size(Pos2::new(25., 40.), Vec2::splat(300.));
                let start = Vec2::new(40., 40.);
                let end = Vec2::new(160., 55.);
                let from = a.viewport.document_to_screen(canvas, start);
                let to = a.viewport.document_to_screen(canvas, end);
                let before = a.renderer.rgba8(&a.document);
                for (shift, input) in [
                    (false, PointerFrame { position: Some(from), primary_down: true, primary_pressed: true, pressure: 0.6, ..Default::default() }),
                    (true, PointerFrame { position: Some(to), primary_down: true, pressure: 0.6, ..Default::default() }),
                    // Toggle the modifier without moving the pen; raw endpoints must survive.
                    (false, PointerFrame::default()),
                    (snap, PointerFrame { primary_released: true, ..Default::default() }),
                ] {
                    let _ = ctx.run_ui(egui::RawInput {
                        modifiers: egui::Modifiers { shift, ..Default::default() },
                        ..Default::default()
                    }, |ui| a.handle_shape_input(ui, canvas, input));
                }
                assert_eq!(a.document.layers[0].vectors.strokes.len(), 1);
                let points = &a.document.layers[0].vectors.strokes[0].points;
                let expected = kind.points_with_snap(start, end, snap || (checkbox && kind == crate::core::shapes::ShapeKind::Line));
                assert_eq!(points.len(), expected.len());
                for (point, expected) in points.iter().zip(expected) {
                    assert!((Vec2::new(point.x, point.y) - expected).length() < 0.001, "{kind:?}, snap {snap}");
                }
                let finished = a.renderer.rgba8(&a.document);
                assert_ne!(finished, before);
                assert!(a.history.undo(&mut a.document));
                assert_eq!(a.renderer.rgba8(&a.document), before);
                assert!(a.history.redo(&mut a.document));
                assert_eq!(a.renderer.rgba8(&a.document), finished);
              }
            }
        }
    }

    #[test]
    fn all_shapes_survive_zero_pressure_pen_down_and_up_and_refresh_without_motion() {
        for kind in crate::core::shapes::ShapeKind::ALL {
            for contact_pressure in [0., 0.7] {
                let mut a = app();
                let ctx = egui::Context::default();
                let canvas = Rect::from_min_size(Pos2::ZERO, Vec2::splat(236.));
                a.settings.tool = ToolKind::Shapes;
                a.settings.shape = kind;
                a.settings.shape_width_px = 8.;
                a.settings.shape_opacity = 0.6;
                a.settings.pencil_color_rgb = [180, 40, 20];
                let original = a.renderer.rgba8(&a.document);
                for frame in [
                    PointerFrame { position: Some(Pos2::new(40., 40.)), primary_pressed: true, primary_down: true, pressure: 0., ..Default::default() },
                    PointerFrame { position: Some(Pos2::new(140., 110.)), primary_down: true, pressure: contact_pressure, ..Default::default() },
                    PointerFrame { position: Some(Pos2::new(140., 110.)), primary_released: true, pressure: 0., ..Default::default() },
                ] {
                    let output = ctx.run_ui(Default::default(), |ui| {
                        a.ensure_texture(ui.ctx());
                        a.handle_drawing_input(ui, canvas, frame);
                    });
                    if frame.primary_released {
                        assert!(a.shape_drag.is_none());
                        assert!(output.viewport_output.values().any(|v| v.repaint_delay.is_zero()));
                    }
                }
                assert_eq!(a.document.layers[0].vectors.strokes.len(), 1, "{kind:?}");
                let path = &a.document.layers[0].vectors.strokes[0];
                assert_eq!(path.label, kind.label());
                assert!(path.points.iter().all(|p| p.pressure > 0.));
                let committed = a.renderer.rgba8(&a.document);
                assert_ne!(committed, original, "{kind:?} must leave a visible outline");
                let _ = ctx.run_ui(Default::default(), |ui| {
                    a.ensure_texture(ui.ctx());
                    a.handle_drawing_input(ui, canvas, PointerFrame::default());
                });
                assert_eq!(a.renderer.rgba8(&a.document), committed);
                assert!(a.document.take_dirty().is_none());
                assert!(a.history.undo(&mut a.document));
                assert_eq!(a.renderer.rgba8(&a.document), original);
                assert!(a.history.redo(&mut a.document));
                assert_eq!(a.renderer.rgba8(&a.document), committed);
            }
        }
    }

    #[test]
    fn shape_preview_commits_one_editable_path_and_undo_restores_blank() {
        let mut app = app();
        let ctx = egui::Context::default();
        app.settings.tool = ToolKind::Shapes;
        app.settings.shape = crate::core::shapes::ShapeKind::Square;
        draw(&mut app, &ctx, Pos2::new(40., 40.), Pos2::new(95., 75.));
        assert_eq!(app.document.layers[0].vectors.strokes.len(), 1);
        let stroke = &app.document.layers[0].vectors.strokes[0];
        assert!(stroke.polyline);
        assert_eq!(stroke.label, "Square");
        assert_eq!(stroke.points.len(), 5);
        assert_eq!(stroke.points[2].y, 95.);
        assert!(app.history.undo(&mut app.document));
        assert!(app.document.surface.graphite_mass.iter().all(|&m| m == 0.));
        assert!(app.document.layers[0].vectors.strokes.is_empty());
    }
}

#[cfg(test)]
mod previews {
    use super::tests::{app, draw};
    use super::*;
    #[test]
    fn rendered_tools_and_real_transform_handles() {
        let mut app = app();
        app.replace_document(CanvasSpec::from_physical("Tool study", 127., 127., 120.));
        app.viewport.zoom = 1.;
        app.fit_requested = true;
        app.document.set_paper_color([248, 244, 236]);
        let ctx = egui::Context::default();
        ctx.set_visuals(egui::Visuals::light());
        ctx.style_mut_of(egui::Theme::Light, |s| s.animation_time = 0.);
        app.settings.tool = ToolKind::Tissue;
        app.settings.tissue_size_px = 170.;
        app.settings.tissue_load = 0.6;
        draw(&mut app, &ctx, Pos2::new(65., 115.), Pos2::new(210., 115.));
        app.settings.tool = ToolKind::Pencil;
        app.settings.tilt_deg = 55.;
        for i in 0..7 {
            draw(
                &mut app,
                &ctx,
                Pos2::new(40., 75. + i as f32 * 10.),
                Pos2::new(210., 55. + i as f32 * 10.),
            );
        }
        let tips = crate::core::brush::import_abr(
            include_bytes!("../../fixtures/graphite-grain-sample.abr"),
            "Sample",
        )
        .unwrap();
        app.brush_library = tips.clone();
        app.settings.brush_tip = Some(tips[0].clone());
        app.settings.brush_size_px = 52.;
        app.settings.tool = ToolKind::Brush;
        app.settings.pencil_color_rgb = [128, 54, 32];
        draw(&mut app, &ctx, Pos2::new(45., 195.), Pos2::new(210., 210.));
        // More space is available in this study; draw shape preview events against its full canvas.
        let canvas = Rect::from_min_size(Pos2::ZERO, Vec2::splat(600.));
        for (kind, start, end) in [
            (
                crate::core::shapes::ShapeKind::Circle,
                Pos2::new(325., 70.),
                Pos2::new(475., 220.),
            ),
            (
                crate::core::shapes::ShapeKind::Triangle,
                Pos2::new(310., 315.),
                Pos2::new(430., 435.),
            ),
            (
                crate::core::shapes::ShapeKind::Square,
                Pos2::new(70., 340.),
                Pos2::new(190., 460.),
            ),
        ] {
            app.settings.tool = ToolKind::Shapes;
            app.settings.shape = kind;
            app.settings.shape_width_px = 5.;
            app.settings.pencil_color_rgb = [55, 80, 110];
            for frame in [
                PointerFrame {
                    position: Some(start),
                    primary_pressed: true,
                    primary_down: true,
                    pressure: 0.8,
                    ..Default::default()
                },
                PointerFrame {
                    position: Some(end),
                    primary_released: true,
                    pressure: 0.8,
                    ..Default::default()
                },
            ] {
                let _ = ctx.run_ui(Default::default(), |ui| {
                    app.handle_shape_input(ui, canvas, frame)
                });
            }
        }
        let rgba = app.renderer.rgba8(&app.document);
        image::save_buffer(
            "../../tools-v18.png",
            &rgba,
            600,
            600,
            image::ColorType::Rgba8,
        )
        .unwrap();
        app.save_project_path(std::path::Path::new("../../tools-v18.graphite"))
            .unwrap();
        app.save_project_path(std::path::Path::new("../../tools-v18.psd"))
            .unwrap();
        app.editing.selected_path = Some(app.document.layers[0].vectors.strokes.len() - 1);
        app.settings.tool = ToolKind::Lasso;
        let ctx = egui::Context::default();
        ctx.set_visuals(egui::Visuals::light());
        ctx.style_mut_of(egui::Theme::Light, |s| s.animation_time = 0.);
        let mut preview = crate::ui::test_render::Preview::default();
        let initial = ctx.run_ui(Default::default(), |ui| app.begin_transform(ui.ctx()));
        preview.update(&initial);
        for pass in 0..3 {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440., 920.))),
                    ..Default::default()
                },
                |ui| app.interface(ui),
            );
            preview.update(&output);
            if pass == 2 {
                preview.save(&ctx, &output, "../../ui-v18-transform.png", [1440, 920]);
            }
        }
        // Exercise the actual resize handle and apply, not just the transform model.
        let paper = ctx.data(|d| {
            d.get_temp::<Rect>(egui::Id::new("test_paper_rect"))
                .unwrap()
        });
        let original = app.editing.transform.as_ref().unwrap().target;
        let grip = paper.min + original.right_bottom().to_vec2() * app.viewport.zoom;
        for frame in [
            PointerFrame {
                position: Some(grip),
                primary_pressed: true,
                primary_down: true,
                ..Default::default()
            },
            PointerFrame {
                position: Some(grip + Vec2::splat(30.)),
                primary_down: true,
                ..Default::default()
            },
            PointerFrame {
                position: Some(grip + Vec2::splat(30.)),
                primary_released: true,
                ..Default::default()
            },
        ] {
            let _ = ctx.run_ui(Default::default(), |ui| app.edit_input(ui, paper, frame));
        }
        assert!(app.editing.transform.as_ref().unwrap().target.width() > original.width());
        app.apply_transform();
        let rotation = ctx.run_ui(Default::default(), |ui| app.begin_rotation(ui.ctx()));
        preview.update(&rotation);
        app.editing.transform.as_mut().unwrap().angle = 30f32.to_radians();
        for pass in 0..3 {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440., 920.))),
                    ..Default::default()
                },
                |ui| app.interface(ui),
            );
            preview.update(&output);
            if pass == 2 {
                preview.save(&ctx, &output, "../../ui-v18-rotation.png", [1440, 920]);
            }
        }
        app.cancel_transform();
        app.editing.guide = true;
        for pass in 0..3 {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440., 920.))),
                    ..Default::default()
                },
                |ui| app.interface(ui),
            );
            preview.update(&output);
            if pass == 2 {
                preview.save(&ctx, &output, "../../ui-v18-guide.png", [1440, 920]);
            }
        }
    }
}
