//! Lossless quarter turns: relocate samples, never resample or flatten layers.
use super::{
    document::Document,
    material::SparseDepositState,
    vector::{VectorLayer, VectorStroke},
};
use eframe::egui::Vec2;
use std::{collections::HashMap, sync::Arc};

pub struct QuarterTurn {
    pub width: usize,
    pub height: usize,
    pub clockwise: bool,
    pub reflection: Option<bool>,
    tips: HashMap<usize,Arc<super::brush::BrushTip>>,
    layers: HashMap<usize, Arc<VectorLayer>>,
    strokes: HashMap<usize, Arc<VectorStroke>>,
    bases: HashMap<usize, Arc<SparseDepositState>>,
}
impl QuarterTurn {
    pub fn new(width: usize, height: usize, clockwise: bool) -> Self {
        Self {
            width,
            height,
            clockwise,
            reflection: None,
            tips: HashMap::new(),
            layers: HashMap::new(),
            strokes: HashMap::new(),
            bases: HashMap::new(),
        }
    }
    pub fn mirror(width:usize,height:usize,horizontal:bool)->Self {
        let mut out=Self::new(width,height,true);out.reflection=Some(horizontal);out
    }
    pub fn orientation(&self,x:f32,y:f32)->(f32,f32) {
        if self.reflection.is_some(){(x,-y)}else{(-x,-y)}
    }
    pub fn index(&self, i: usize) -> usize {
        let (x, y) = (i % self.width, i / self.width);
        if let Some(horizontal)=self.reflection {
            return if horizontal {y*self.width+self.width-1-x}else{(self.height-1-y)*self.width+x};
        }
        if self.clockwise {
            x * self.height + self.height - 1 - y
        } else {
            (self.width - 1 - x) * self.height + y
        }
    }
    pub fn point(&self, p: Vec2) -> Vec2 {
        if let Some(horizontal)=self.reflection {
            return if horizontal {Vec2::new(self.width as f32-p.x,p.y)}else{Vec2::new(p.x,self.height as f32-p.y)};
        }
        if self.clockwise {
            Vec2::new(self.height as f32 - p.y, p.x)
        } else {
            Vec2::new(p.y, self.width as f32 - p.x)
        }
    }
    pub fn grid<T: Copy>(&self, values: &mut Vec<T>) {
        assert_eq!(values.len(), self.width * self.height);
        let mut rotated = vec![values[0]; values.len()];
        for (i, &value) in values.iter().enumerate() {
            rotated[self.index(i)] = value;
        }
        *values = rotated;
    }
    pub fn sparse(&self, old: &SparseDepositState) -> SparseDepositState {
        let mut out = if self.reflection.is_some(){SparseDepositState::new(self.width,self.height)}else{SparseDepositState::new(self.height, self.width)};
        for (&tile_id, tile) in &old.tiles {
            let tile_x = tile_id % self.width.div_ceil(32) * 32;
            let tile_y = tile_id / self.width.div_ceil(32) * 32;
            for (offset, &state) in tile.iter().enumerate() {
                let x = tile_x + offset % 32;
                let y = tile_y + offset / 32;
                if x < self.width && y < self.height && !state.is_empty() {
                    let mut state = state;
                    (state.orientation_x,state.orientation_y)=self.orientation(state.orientation_x,state.orientation_y);
                    out.set(self.index(y * self.width + x), state);
                }
            }
        }
        out
    }
    pub fn layer(&mut self, old: &Arc<VectorLayer>) -> Arc<VectorLayer> {
        let key = Arc::as_ptr(old) as usize;
        if let Some(found) = self.layers.get(&key) {
            return found.clone();
        }
        let base = old.base.as_ref().map(|b| {
            let key = Arc::as_ptr(b) as usize;
            if let Some(found) = self.bases.get(&key) {
                return found.clone();
            }
            let new = Arc::new(self.sparse(b));
            self.bases.insert(key, new.clone());
            new
        });
        let strokes = old
            .strokes
            .iter()
            .map(|s| {
                let key = Arc::as_ptr(s) as usize;
                if let Some(found) = self.strokes.get(&key) {
                    return found.clone();
                }
                let mut new = (**s).clone();
                if let Some(horizontal)=self.reflection {
                    new=s.mirrored(eframe::egui::pos2(self.width as f32*0.5,self.height as f32*0.5),horizontal,&mut self.tips);
                    let new=Arc::new(new);self.strokes.insert(key,new.clone());return new;
                }
                let angle = if self.clockwise { 90. } else { -90. };
                if let Some(shape)=&mut new.shape {shape.map(|p|self.point(p),1.);}
                for p in &mut new.points {
                    let v = self.point(Vec2::new(p.x, p.y));
                    p.x = v.x;
                    p.y = v.y;
                    p.azimuth_deg = (p.azimuth_deg + angle).rem_euclid(360.);
                    p.rotation_deg = p.rotation_deg.map(|r| (r + angle).rem_euclid(360.));
                }
                new.settings.azimuth_deg = (new.settings.azimuth_deg + angle).rem_euclid(360.);
                new.settings.brush_angle_deg =
                    (new.settings.brush_angle_deg + angle).rem_euclid(360.);
                new.tip.profile.rotate_by(angle);
                // Masks are recreated only when replayed; preserve path restrictions compactly.
                new.selection = Default::default();
                new.selection.polygon =
                    s.selection.polygon.iter().map(|&p| self.point(p)).collect();
                let new = Arc::new(new);
                self.strokes.insert(key, new.clone());
                new
            })
            .collect();
        let new = Arc::new(VectorLayer { base, strokes });
        self.layers.insert(key, new.clone());
        new
    }
    pub fn document(&mut self, doc: &mut Document) {
        self.grid(&mut doc.paper_albedo);
        self.grid(&mut doc.color_grain);
        macro_rules! turn {($($field:ident),*)=>{$(self.grid(&mut doc.surface.$field);)*};}
        turn!(
            rest_height,
            fiber,
            contact_support,
            edge_grain,
            current_height,
            abrasion,
            graphite_mass,
            clay_mass,
            wax_mass,
            loose_mass,
            compacted_mass,
            orientation_x,
            orientation_y,
            color_r_mass,
            color_g_mass,
            color_b_mass
        );
        if self.reflection.is_none() { for value in &mut doc.surface.orientation_x {
            *value = -*value;
        } }
        for value in &mut doc.surface.orientation_y {
            *value = -*value;
        }
        for layer in &mut doc.layers {
            layer.deposit = self.sparse(&layer.deposit);
            layer.vectors = self.layer(&layer.vectors);
        }
        doc.selection.quarter_turn(self);
        if self.reflection.is_none() {
            std::mem::swap(&mut doc.spec.width_px, &mut doc.spec.height_px);
            std::mem::swap(&mut doc.spec.width_mm, &mut doc.spec.height_mm);
        }
        doc.mark_all_dirty();
    }
}
