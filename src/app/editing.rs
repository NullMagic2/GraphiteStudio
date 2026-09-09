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
    action_pointer_down: bool,
}
pub(super) struct Transform {
    flip: [bool;2],
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
    pub(super) fn translate_edit_workspace(&mut self,map:&mut crate::core::orientation::QuarterTurn,delta:Vec2) {
        if let Some(p)=&mut self.editing.vector_pick {*p+=delta;}
        if let Some(points)=&mut self.editing.lasso {for p in points {*p+=delta;}}
        if let Some(t)=&mut self.editing.transform {
            t.cutout.bounds=t.cutout.bounds.translate(delta);
            t.target=t.target.translate(delta);t.affected=t.affected.translate(delta);
            if let Some((_,start,original))=&mut t.drag {*start+=delta;*original=original.translate(delta);}
            if let Some((original,_))=&mut t.vectors {*original=map.layer(original);}
            t.tx.expand(map);
        }
    }
    pub(super) fn transform_dragging(&self)->bool {self.editing.transform.as_ref().is_some_and(|t|t.drag.is_some() || t.rotation_drag.is_some())}
    pub(super) fn mirror_selection_or_document(&mut self,ctx:&egui::Context,horizontal:bool) {
        self.finish_stroke();
        if self.editing.transform.is_none() && self.editing.selected_path.is_none() && self.document.selection.polygon.is_empty() {
            self.history.mirror_document(&mut self.document,horizontal);
            self.tabs[self.active_tab].modified=true;
            self.stroke_engine.clear_smudger();
            self.status="Mirrored the whole drawing. Ctrl+Z restores it.".into();
        }else{
            self.begin_transform(ctx);
            if let Some(t)=&mut self.editing.transform {t.flip[usize::from(!horizontal)]^=true;}
            self.status="Mirrored selection. Checkmark keeps it; X restores the original.".into();
        }
        ctx.request_repaint();
    }
    pub(super) fn transform_scale_percent(&self)->Option<f32> {
        let t=self.editing.transform.as_ref()?;
        Some((t.target.area()/t.cutout.bounds.area().max(0.0001)).sqrt()*100.)
    }

    pub(super) fn set_transform_scale_percent(&mut self,percent:f32) {
        let Some(current)=self.transform_scale_percent() else{return;};
        if !percent.is_finite(){return;}
        let t=self.editing.transform.as_mut().unwrap();
        let factor=(percent.clamp(1.,400.)/current.max(0.0001))
            .max(1./t.target.width().min(t.target.height()).max(0.0001));
        t.target=Rect::from_center_size(t.target.center(),t.target.size()*factor);
    }

    pub(super) fn select_tool(&mut self, tool: ToolKind) {
        self.finish_liquify(true);
        self.quick_controls.close_size();
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
        self.cancel_transform_and_deselect();
    }

    pub(super) fn cancel_transform_and_deselect(&mut self) {
        self.cancel_transform();
        self.clear_edit_selection();
        self.leave_selection_tool();
    }

    fn clear_edit_selection(&mut self) {
        self.document.selection = Default::default();
        self.editing.selected_path = None;
        self.editing.vector_pick = None;
        self.editing.lasso = None;
    }

    fn leave_selection_tool(&mut self) {
        if matches!(self.settings.tool, ToolKind::Lasso | ToolKind::VectorSelect) {
            self.settings.tool = ToolKind::Pencil;
        }
    }

    fn confirm_transform_and_deselect(&mut self) {
        self.apply_transform();
        self.leave_selection_tool();
    }

    fn selection_screen_bounds(&self, canvas: Rect) -> Option<Rect> {
        let points = if let Some(t) = &self.editing.transform {
            corners(t.target).map(|p| vector::rotate(p, t.target.center(), t.angle).to_vec2()).to_vec()
        } else if !self.document.selection.polygon.is_empty() {
            self.document.selection.polygon.clone()
        } else {
            let index = self.editing.selected_path?;
            let stroke = self.document.layers[self.document.active_layer_index()].vectors.strokes.get(index)?;
            corners(stroke.bounds(self.document.spec.dpi)).map(|p| p.to_vec2()).to_vec()
        };
        let bounds = points.into_iter().fold(Rect::NOTHING, |r, p| r.union(Rect::from_min_size(self.viewport.document_to_screen(canvas, p), Vec2::ZERO)));
        bounds.is_finite().then_some(bounds)
    }

    pub(super) fn show_transform_actions(&mut self, ui: &egui::Ui, canvas: Rect, visible: Rect) -> Option<Rect> {
        if !ui.input(|i| i.focused) { self.editing.action_pointer_down = false; }
        if self.gallery.open || self.paper_settings.open || self.save_as_open { return None; }
        let bounds = self.selection_screen_bounds(canvas)?;
        let size = Vec2::new(112., 56.);
        let limit = visible.shrink(6.);
        let y = if bounds.bottom() + 12. + size.y <= limit.bottom() { bounds.bottom() + 12. }
            else if bounds.top() - 12. - size.y >= limit.top() { bounds.top() - 12. - size.y }
            else { limit.bottom() - size.y };
        let pos = Pos2::new(bounds.right() - size.x, y).clamp(limit.min, (limit.max - size).max(limit.min));
        let mut action = None;
        let response = egui::Area::new(egui::Id::new("selection_actions"))
            .order(egui::Order::Foreground).fixed_pos(pos).movable(false).constrain(false).fade_in(false)
            .default_size(size).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).inner_margin(4.).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 8.;
                    ui.horizontal(|ui| {
                        if crate::ui::tool_icons::transform_action_button(ui, true).clicked() { action = Some(true); }
                        if crate::ui::tool_icons::transform_action_button(ui, false).clicked() { action = Some(false); }
                    });
                });
            });
        if let Some(confirm) = action {
            if confirm { self.confirm_transform_and_deselect(); } else { self.cancel_transform_and_deselect(); }
            ui.ctx().request_repaint();
        }
        Some(response.response.rect)
    }

    pub(super) fn transform_actions_block_input(&mut self, actions: Option<Rect>, input: PointerFrame) -> bool {
        let over = input.position.is_some_and(|p| actions.is_some_and(|r| r.contains(p)));
        if over && input.primary_pressed { self.editing.action_pointer_down = true; }
        let block = over || self.editing.action_pointer_down;
        if input.primary_released { self.editing.action_pointer_down = false; }
        block
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
                .is_some_and(|p| ui.clip_rect().contains(p))
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
        self.finish_liquify(true);
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
            flip: [false,false],
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
        if let Some(t)=&self.editing.transform {
            let mut bounds=Rect::NOTHING;
            for p in corners(t.target) {bounds.extend_with(vector::rotate(p,t.target.center(),t.angle));}
            if let Some((original,selected))=&t.vectors {
                let mut tips=std::collections::HashMap::new();
                for &index in selected {
                    let mut stroke=original.strokes[index].transformed(t.cutout.bounds,t.target);
                    for (axis,flip) in t.flip.iter().enumerate(){if *flip{stroke=stroke.mirrored(t.target.center(),axis==0,&mut tips);}}
                    bounds=bounds.union(stroke.rotated(t.target.center(),t.angle).bounds(self.document.spec.dpi));
                }
            }
            self.grow_workspace(bounds.expand(4.));
        }
        let Some(mut t) = self.editing.transform.take() else {
            self.clear_edit_selection();
            return;
        };
        if t.target == t.cutout.bounds && t.angle.abs() < 1e-6 && !t.flip.iter().any(|&f|f) {
            t.tx.rollback(&mut self.document);
            self.clear_edit_selection();
            return;
        }
        let vector_mode = t.vectors.is_some();
        if let Some((original, selected)) = t.vectors {
            let mut changed = (*original).clone();
            let mut new_affected = Rect::NOTHING;
            let mut mirrored_tips=std::collections::HashMap::new();
            for index in selected {
                let mut stroke=original.strokes[index].transformed(t.cutout.bounds,t.target);
                for (axis,flip) in t.flip.iter().enumerate(){if *flip {stroke=stroke.mirrored(t.target.center(),axis==0,&mut mirrored_tips);}}
                changed.strokes[index]=Arc::new(stroke.rotated(t.target.center(),t.angle));
                new_affected =
                    new_affected.union(changed.strokes[index].bounds(self.document.spec.dpi));
            }
            vector::replay(&mut self.document, &changed, &mut t.tx);
            t.tx.restore_outside(&mut self.document, &[t.affected, new_affected]);
            let active = self.document.active_layer_index();
            self.document.layers[active].vectors = Arc::new(changed);
        } else {
            t.cutout
                .place_transformed(&mut self.document, t.target, t.angle,t.flip, &mut t.tx);
            self.bake_paths();
        }
        self.clear_edit_selection();
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
            if input.primary_pressed && ui.clip_rect().contains(screen) {
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
                        if r.is_finite() {
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
            let point = pos.to_vec2();
            if input.primary_pressed && ui.clip_rect().contains(screen) {
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
                    uv:Pos2::new(if t.flip[0]{1.-uv.x}else{uv.x},if t.flip[1]{1.-uv.y}else{uv.y}),
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
            if self.shortcuts.bindings.consume(i,Command::RotateSelection) {
                7
            } else if self.shortcuts.bindings.consume(i,Command::Transform) {
                1
            } else if self.shortcuts.bindings.consume(i,Command::Deselect) {
                2
            } else if self.shortcuts.bindings.consume(i,Command::SelectAll) {
                3
            } else if self.shortcuts.bindings.consume(i,Command::Cancel) {
                4
            } else if self.shortcuts.bindings.consume(i,Command::Confirm) {
                5
            } else if self.shortcuts.bindings.consume(i,Command::Delete) {
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
            2 => self.cancel_transform_and_deselect(),
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
            5 => self.confirm_transform_and_deselect(),
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
    pub(super) fn mirror_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for (horizontal,icon,label) in [
                (true,crate::ui::action_icons::ActionIcon::MirrorHorizontal,"Mirror horizontal"),
                (false,crate::ui::action_icons::ActionIcon::MirrorVertical,"Mirror vertical")
            ] {
                if crate::ui::action_icons::labeled_button(ui,icon,label)
                    .on_hover_text("Mirror the selection, or the whole drawing when nothing is selected. Undo restores the original.").clicked() {
                    self.mirror_selection_or_document(ui.ctx(),horizontal);
                }
            }
        });
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
              ui.small("This guide lists default keys. Your current assignments are in Options → Keyboard shortcuts.");
            for (name,help) in [
                ("Pencil","Draw graphite with pressure and tilt. Line smoothing reduces wobble; 0% switches it off."),
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
    #[test]
    fn pixel_and_vector_objects_move_fully_outside_page_and_remain_editable() {
        for pixels in [false,true] {
            let mut a=app();let ctx=egui::Context::default();
            draw(&mut a,&ctx,Pos2::new(80.,90.),Pos2::new(140.,105.));
            let original=a.document.clone();
            if pixels {a.document.selection.set(vec![Vec2::new(60.,70.),Vec2::new(160.,70.),Vec2::new(160.,125.),Vec2::new(60.,125.)],236,236);}
            a.begin_transform(&ctx);
            assert_eq!(a.editing.transform.as_ref().unwrap().vectors.is_none(),pixels);
            a.editing.transform.as_mut().unwrap().target=a.editing.transform.as_ref().unwrap().target.translate(Vec2::new(-300.,-200.));
            a.apply_transform();
            let [px,py,pw,ph]=a.document.page_bounds();
            assert_eq!([pw,ph],[236,236]);
            let mass:f32=a.document.surface.graphite_mass.iter().sum();assert!(mass>0.);
            for y in py..py+ph {for x in px..px+pw {assert_eq!(a.document.surface.graphite_mass[a.document.index(x,y)],0.);}}
            let moved=a.document.surface.graphite_mass.clone();
            a.begin_transform(&ctx);
            assert!(a.editing.transform.is_some(),"Outside object cannot be selected again");
            a.cancel_transform();
            assert!(a.document.surface.graphite_mass==moved);
            assert!(a.history.undo(&mut a.document));
            for y in 0..236 {for x in 0..236 {
                assert_eq!(a.document.surface.graphite_mass[a.document.index(x+px,y+py)],original.surface.graphite_mass[original.index(x,y)]);
            }}
            assert!(a.history.redo(&mut a.document));assert!(a.document.surface.graphite_mass==moved);
            for ext in ["graphite","psd"] {
                let path=std::env::temp_dir().join(format!("graphite-moved-{}-{pixels}.{ext}",std::process::id()));
                a.save_project_path(&path).unwrap();
                let p=graphite_studio::project::load(&path).unwrap();
                assert_eq!(p.document.page,a.document.page);
                assert!(p.document.surface.graphite_mass==moved);
                assert_eq!(p.document.layers[0].vectors.strokes.is_empty(),pixels);
                std::fs::remove_file(path).unwrap();
            }
        }
    }
    #[test]
    fn mirror_buttons_are_only_shown_with_transform_controls() {
        let mut a=app();let ctx=egui::Context::default();
        for tool in [ToolKind::VectorSelect,ToolKind::Pencil,ToolKind::Lasso,ToolKind::Eraser,ToolKind::Smudge,ToolKind::Tissue,ToolKind::Shapes] {
            a.select_tool(tool);
            for _ in 0..3 {
                for label in ["Mirror horizontal","Mirror vertical"] {
                    ctx.data_mut(|d|d.remove::<Rect>(egui::Id::new(("test_action_icon",label))));
                }
                interface_frame(&mut a,&ctx,vec![]);
            }
            for label in ["Mirror horizontal","Mirror vertical"] {
                let visible=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new(("test_action_icon",label))).is_some());
                assert_eq!(visible,matches!(tool,ToolKind::VectorSelect|ToolKind::Lasso),"{label} for {tool:?}");
            }
        }
        a.select_tool(ToolKind::Pencil);
        draw(&mut a,&ctx,Pos2::new(35.,55.),Pos2::new(90.,65.));
        a.begin_transform(&ctx);
        assert!(a.editing.transform.is_some());
        for _ in 0..3 {interface_frame(&mut a,&ctx,vec![]);}
        assert!(ctx.data(|d|d.get_temp::<Rect>(egui::Id::new(("test_action_icon","Mirror horizontal"))).is_some()));
    }
    #[test]
    fn copy_layer_button_retains_paths_opacity_and_independent_edits() {
        let mut a=app();let ctx=egui::Context::default();
        draw(&mut a,&ctx,Pos2::new(35.,55.),Pos2::new(90.,65.));
        a.document.layers[0].opacity=137;
        let original=a.document.layers[0].vectors.clone();let source=a.document.active_layer_id();
        let material=a.document.surface.graphite_mass.clone();
        for _ in 0..3 {interface_frame(&mut a,&ctx,vec![]);}
        let button=ctx.data(|d|d.get_temp::<Rect>(egui::Id::new(("test_action_icon","Copy layer"))).unwrap());
        click_interface(&mut a,&ctx,button.center());
        assert_eq!(a.document.layer_count(),2);assert_eq!(a.document.active_layer_index(),1);
        assert_ne!(a.document.active_layer_id(),source);
        assert_eq!(a.document.layers[1].opacity,137);
        assert_eq!(a.document.layers[1].blend_mode,a.document.layers[0].blend_mode);
        assert!(Arc::ptr_eq(&a.document.layers[1].vectors,&original));
        assert_eq!(a.document.surface.graphite_mass,material);
        // The interface fitted the view; this fixture supplies pixel coordinates directly.
        a.viewport.zoom=1.;
        draw(&mut a,&ctx,Pos2::new(35.,80.),Pos2::new(85.,95.));
        assert_eq!(a.document.layers[0].vectors.strokes.len(),1);
        assert_eq!(a.document.layers[1].vectors.strokes.len(),2);
        assert!(a.history.undo(&mut a.document));assert!(a.history.undo(&mut a.document));
        assert_eq!(a.document.layer_count(),1);assert_eq!(a.document.active_layer_id(),source);
        assert_eq!(a.document.surface.graphite_mass,material);
        assert!(a.history.redo(&mut a.document));assert_eq!(a.document.layers[1].vectors.strokes.len(),1);
        assert!(a.history.redo(&mut a.document));assert_eq!(a.document.layers[1].vectors.strokes.len(),2);
    }
    #[test]
    fn mirror_selection_and_scale_preserve_other_paths_and_cancel_cleanly() {
        for pixels in [false,true] {for horizontal in [false,true] {
            let mut a=app();let ctx=egui::Context::default();
            draw(&mut a,&ctx,Pos2::new(35.,55.),Pos2::new(90.,65.));
            draw(&mut a,&ctx,Pos2::new(40.,95.),Pos2::new(105.,110.));
            let original=a.document.layers[0].vectors.clone();
            let before=a.renderer.rgba8(&a.document);
            a.editing.selected_path=Some(0);
            if pixels {a.document.selection.set(vec![Vec2::new(20.,35.),Vec2::new(115.,35.),Vec2::new(115.,80.),Vec2::new(20.,80.)],a.document.spec.width_px,a.document.spec.height_px);}
            a.begin_transform(&ctx);
            let old=a.editing.transform.as_ref().unwrap().target;
            a.set_transform_scale_percent(150.);
            let target=a.editing.transform.as_ref().unwrap().target;
            assert!((target.width()/old.width()-1.5).abs()<0.0001);assert_eq!(target.center(),old.center());
            a.set_transform_scale_percent(100.);
            a.mirror_selection_or_document(&ctx,horizontal);
            assert!(a.editing.transform.as_ref().unwrap().flip[usize::from(!horizontal)]);
            a.confirm_transform_and_deselect();
            assert!(a.editing.transform.is_none() && a.document.selection.polygon.is_empty());
            if !pixels {
                assert!(Arc::ptr_eq(&original.strokes[1],&a.document.layers[0].vectors.strokes[1]));
                let p=a.document.layers[0].vectors.strokes[0].points[0];let start=original.strokes[0].points[0];
                if horizontal {assert!((p.x-(2.*old.center().x-start.x)).abs()<0.001);}else{assert!((p.y-(2.*old.center().y-start.y)).abs()<0.001);}
            }
            assert!(a.history.undo(&mut a.document));assert_eq!(a.renderer.rgba8(&a.document),before);
            a.editing.selected_path=Some(0);a.mirror_selection_or_document(&ctx,horizontal);
            a.cancel_transform_and_deselect();assert_eq!(a.renderer.rgba8(&a.document),before);
        }}
    }
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
    fn merge_button_preserves_artwork_and_undo_restores_layers_and_editable_paths() {
        let mut a = app(); let ctx = egui::Context::default();
        draw(&mut a, &ctx, Pos2::new(35., 65.), Pos2::new(160., 85.));
        a.handle_layer_panel_action(LayerPanelAction::Add);
        draw(&mut a, &ctx, Pos2::new(55., 80.), Pos2::new(170., 90.));
        let before = a.renderer.rgba8(&a.document);
        let paths: Vec<_> = a.document.layers.iter().map(|l| l.vectors.clone()).collect();
        for _ in 0..3 { interface_frame(&mut a, &ctx, vec![]); }
        let rect = ctx.data(|d| d.get_temp::<Rect>(egui::Id::new("test_merge_down")).unwrap());
        click_interface(&mut a, &ctx, rect.center());
        assert_eq!(a.document.layer_count(), 1);
        let merged = a.renderer.rgba8(&a.document);
        assert!(before.iter().zip(&merged).all(|(&a,&b)| a.abs_diff(b)<=1));
        assert!(a.document.layers[0].vectors.strokes.is_empty());
        assert!(a.history.undo(&mut a.document));
        assert_eq!(a.document.layer_count(), 2);
        assert_eq!(before, a.renderer.rgba8(&a.document));
        for (l,p) in a.document.layers.iter().zip(&paths) { assert!(Arc::ptr_eq(&l.vectors,p)); }
        assert!(a.history.redo(&mut a.document));
        draw(&mut a, &ctx, Pos2::new(45., 130.), Pos2::new(130., 150.));
        assert_ne!(merged, a.renderer.rgba8(&a.document));
        assert!(a.history.undo(&mut a.document));
        assert_eq!(merged, a.renderer.rgba8(&a.document));
        assert!(a.history.undo(&mut a.document));
        assert_eq!(before, a.renderer.rgba8(&a.document));
        assert!(a.history.undo(&mut a.document));
        assert!(a.document.layers[1].vectors.strokes.is_empty());
    }

    #[test]
    fn multiple_layer_selection_and_merge_work_with_ctrl_shift_and_touch_controls() {
        for method in 0..3 {
            let mut a=app(); let ctx=egui::Context::default();
            for l in 0..4 {
                if l>0 {a.handle_layer_panel_action(LayerPanelAction::Add);}
                draw(&mut a,&ctx,Pos2::new(35.,60.+l as f32*20.),Pos2::new(160.,80.+l as f32*20.));
            }
            let before=a.renderer.rgba8(&a.document);
            let paths:Vec<_>=a.document.layers.iter().map(|l|l.vectors.clone()).collect();
            a.tabs[a.active_tab].modified=false;
            let click=|a:&mut GraphiteApp,id:egui::Id,modifiers:egui::Modifiers| {
                for _ in 0..2 {interface_frame(a,&ctx,vec![]);}
                let pos=ctx.data(|d|d.get_temp::<Rect>(id).unwrap().center());
                for pressed in [true,false] {
                    let _=ctx.run_ui(egui::RawInput { screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(1440.,920.))),modifiers,
                        events:vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton {pos,button:egui::PointerButton::Primary,pressed,modifiers}],..Default::default() },|ui|a.interface(ui));
                }
            };
            if method==2 {
                click(&mut a,egui::Id::new("test_layer_multiple"),Default::default());
                for id in 1..=3 {click(&mut a,egui::Id::new(("test_layer_rect",id as u64)),Default::default());}
            } else {
                click(&mut a,egui::Id::new(("test_layer_rect",1u64)),Default::default());
                if method==1 {click(&mut a,egui::Id::new(("test_layer_rect",4u64)),egui::Modifiers::SHIFT);}
                else {for id in [3u64,2,4] {click(&mut a,egui::Id::new(("test_layer_rect",id)),egui::Modifiers::CTRL);}}
            }
            assert_eq!(a.document.selected_layer_indices(),vec![0,1,2,3]);
            assert!(!a.tabs[a.active_tab].modified,"Selecting layers must not dirty the drawing");
            if method==2 {
                let mut preview=crate::ui::test_render::Preview::default();
                let mut view=app();view.document=a.document.clone();view.settings=a.settings.clone();
                let view_ctx=egui::Context::default();
                for pass in 0..3 {let out=interface_frame(&mut view,&view_ctx,vec![]);preview.update(&out);if pass==2 {preview.save(&view_ctx,&out,"../../ui-v241-layer-selection.png",[1440,920]);}}
            }
            click(&mut a,egui::Id::new("test_merge_down"),Default::default());
            assert_eq!(a.document.layer_count(),1);assert!(a.tabs[a.active_tab].modified);
            assert!(before.iter().zip(a.renderer.rgba8(&a.document)).all(|(&a,b)|a.abs_diff(b)<=1));
            assert!(a.history.undo(&mut a.document));assert_eq!(a.document.layer_count(),4);
            assert_eq!(a.document.selected_layer_indices(),vec![0,1,2,3]);
            assert_eq!(a.renderer.rgba8(&a.document),before);
            for (layer,path) in a.document.layers.iter().zip(paths) {assert!(Arc::ptr_eq(&layer.vectors,&path));}
            assert!(a.history.redo(&mut a.document));assert_eq!(a.document.layer_count(),1);
        }
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
    fn transform_check_and_cross_work_for_vectors_and_pixels_with_and_without_panels() {
        for pixels in [false, true] { for confirm in [false, true] { for moved in [false, true] {
            let mut a = app();
            let ctx = egui::Context::default();
            draw(&mut a, &ctx, Pos2::new(35., 55.), Pos2::new(90., 65.));
            let before = a.document.surface.graphite_mass.clone();
            let original = a.document.layers[0].vectors.clone();
            a.settings.tool = if pixels { ToolKind::Lasso } else { ToolKind::VectorSelect };
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
            if moved { t.target = t.target.translate(Vec2::new(45., 60.)); }
            a.material_panel_visible = pixels && confirm && moved;
            a.layers_panel_visible = false;
            a.fullscreen = pixels != confirm;
            for pass in 0..3 {
                let output = interface_frame(&mut a, &ctx, vec![]);
                preview.update(&output);
                if pass == 2 && !pixels && !confirm && moved {
                    preview.save(&ctx, &output, "../../ui-v239-transform.png", [1440, 920]);
                }
                if pass == 2 && a.material_panel_visible {
                    preview.save(&ctx, &output, "../../ui-v239-tool-options.png", [1440, 920]);
                }
            }
            if a.material_panel_visible {
                let aspect = ctx.data(|d| d.get_temp::<Rect>(egui::Id::new("test_aspect_option")).unwrap());
                assert!(aspect.right() < 400., "Aspect option must stay in the left panel");
                click_interface(&mut a, &ctx, aspect.center());
                assert!(a.settings.transform_keep_aspect);
            }
            let rect = ctx.data(|d| d.get_temp::<Rect>(egui::Id::new(("test_transform_action", confirm))).unwrap());
            assert!(rect.height() >= 44. && rect.left() >= 0. && rect.right() < 1440.);
            click_interface(&mut a, &ctx, rect.center());
            assert!(a.editing.transform.is_none());
            assert!(a.document.selection.polygon.is_empty());
            assert!(a.editing.selected_path.is_none());
            assert_eq!(a.settings.tool, ToolKind::Pencil);
            assert!(a.stroke_session.is_none(), "Button must not start a stroke");
            if confirm && moved {
                assert_ne!(before, a.document.surface.graphite_mass);
                assert!(a.history.undo(&mut a.document));
            } else {
                assert!(a.document.selection.polygon.is_empty());
                assert!(a.editing.selected_path.is_none());
            }
            assert_eq!(before, a.document.surface.graphite_mass);
            assert!(Arc::ptr_eq(&original, &a.document.layers[0].vectors));
        }}}
    }

    #[test]
    fn tool_switch_and_actions_clear_standalone_selection_masks() {
        for action in 0..3 {
            let mut a = app();
            let ctx = egui::Context::default();
            draw(&mut a, &ctx, Pos2::new(50., 70.), Pos2::new(100., 80.));
            let before = a.renderer.rgba8(&a.document);
            a.settings.tool = ToolKind::Lasso;
            a.document.selection.set(vec![Vec2::new(30., 40.), Vec2::new(130., 40.), Vec2::new(130., 100.), Vec2::new(30., 100.)], a.document.spec.width_px, a.document.spec.height_px);
            assert!(a.editing.transform.is_none());
            for _ in 0..3 { interface_frame(&mut a, &ctx, vec![]); }
            if action == 0 {
                let pos = ctx.data(|d| d.get_temp::<Rect>(egui::Id::new(("test_tool_icon", ToolKind::Eraser.label()))).unwrap().center());
                click_interface(&mut a, &ctx, pos);
                assert_eq!(a.settings.tool, ToolKind::Eraser);
            } else {
                let pos = ctx.data(|d| d.get_temp::<Rect>(egui::Id::new(("test_transform_action", action == 1))).unwrap().center());
                click_interface(&mut a, &ctx, pos);
                assert_eq!(a.settings.tool, ToolKind::Pencil);
            }
            assert!(a.document.selection.polygon.is_empty());
            assert!(a.document.selection.allows(0));
            assert!(a.editing.selected_path.is_none());
            assert!(a.editing.lasso.is_none());
            assert!(a.stroke_session.is_none());
            assert_eq!(before, a.renderer.rgba8(&a.document));
        }
    }

    #[test]
    fn floating_actions_follow_rotated_selection_and_stay_in_view() {
        let mut a = app();
        let ctx = egui::Context::default();
        draw(&mut a, &ctx, Pos2::new(50., 70.), Pos2::new(100., 80.));
        let _ = ctx.run_ui(Default::default(), |ui| a.begin_transform(ui.ctx()));
        let canvas = Rect::from_min_size(Pos2::new(300., 100.), Vec2::splat(236.));
        let visible = Rect::from_min_size(Pos2::new(280., 80.), Vec2::new(500., 500.));
        for offset in [Vec2::ZERO, Vec2::new(30., 20.), Vec2::new(0., 400.), Vec2::new(-600., -600.), Vec2::new(900., 900.)] {
            let t = a.editing.transform.as_mut().unwrap();
            t.target = Rect::from_min_size(Pos2::new(50., 70.) + offset, Vec2::new(80., 40.));
            t.angle = 0.4;
            a.viewport.rotation = 0.25;
            for _ in 0..3 {
                let _ = ctx.run_ui(egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 700.))), ..Default::default() }, |ui| {
                    let rect = a.show_transform_actions(ui, canvas, visible).unwrap();
                    assert!(visible.contains_rect(rect), "{rect:?} outside {visible:?}");
                });
            }
            let rect = ctx.data(|d| d.get_temp::<Rect>(egui::Id::new(("test_transform_action", true))).unwrap());
            let bounds = a.selection_screen_bounds(canvas).unwrap();
            if offset.length() < 100. { assert!((rect.top() - bounds.bottom() - 16.).abs() < 3.); }
            // A native pen that presses a button and leaves it must never resize/draw underneath.
            assert!(a.transform_actions_block_input(Some(rect), PointerFrame { position: Some(rect.center()), primary_pressed: true, primary_down: true, ..Default::default() }));
            assert!(a.transform_actions_block_input(None, PointerFrame { position: Some(Pos2::ZERO), primary_down: true, ..Default::default() }));
            assert!(a.transform_actions_block_input(None, PointerFrame { primary_released: true, ..Default::default() }));
            assert!(!a.transform_actions_block_input(None, PointerFrame::default()));
        }
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
    fn rotated_canvas_draws_at_document_coordinates_including_outside_paper() {
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
        // Bounding rectangle corner is outside the rotated sheet.
        draw(&mut a, &ctx, Pos2::new(1., 1.), Pos2::new(2., 2.));
        assert_eq!(a.document.layers[0].vectors.strokes.len(),2);
        assert!(a.document.page.is_some());
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
        let fullscreen_ctx=egui::Context::default();
        for pass in 0..3 {
            let output = fullscreen_ctx.run_ui(
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
                preview.save(&fullscreen_ctx, &output, "../../ui-v21-fullscreen.png", [1200, 800]);
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
            app.document.spec.width_px as u32,
            app.document.spec.height_px as u32,
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
