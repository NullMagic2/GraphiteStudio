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
    expansion: Option<[usize;4]>,
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
            expansion: None,
            tips: HashMap::new(),
            layers: HashMap::new(),
            strokes: HashMap::new(),
            bases: HashMap::new(),
        }
    }
    pub fn mirror(width:usize,height:usize,horizontal:bool)->Self {
        let mut out=Self::new(width,height,true);out.reflection=Some(horizontal);out
    }
    pub fn expand(width:usize,height:usize,new_width:usize,new_height:usize,dx:usize,dy:usize)->Self {
        let mut out=Self::new(width,height,true);
        out.expansion=Some([new_width,new_height,dx,dy]); out
    }
    pub fn expanded(&self)->bool { self.expansion.is_some() }
    pub fn size(&self)->(usize,usize) {
        if let Some([w,h,_,_])=self.expansion {(w,h)}
        else if self.reflection.is_some() {(self.width,self.height)} else {(self.height,self.width)}
    }
    pub fn orientation(&self,x:f32,y:f32)->(f32,f32) {
        if self.expanded(){return (x,y);}
        if self.reflection.is_some(){(x,-y)}else{(-x,-y)}
    }
    pub fn index(&self, i: usize) -> usize {
        let (x, y) = (i % self.width, i / self.width);
        if let Some([w,_,dx,dy])=self.expansion {return (y+dy)*w+x+dx;}
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
        if let Some([_,_,dx,dy])=self.expansion {return p+Vec2::new(dx as f32,dy as f32);}
        if let Some(horizontal)=self.reflection {
            return if horizontal {Vec2::new(self.width as f32-p.x,p.y)}else{Vec2::new(p.x,self.height as f32-p.y)};
        }
        if self.clockwise {
            Vec2::new(self.height as f32 - p.y, p.x)
        } else {
            Vec2::new(p.y, self.width as f32 - p.x)
        }
    }
    pub fn grid<T: Copy + Default>(&self, values: &mut Vec<T>) {
        assert_eq!(values.len(), self.width * self.height);
        let (w,h)=self.size();
        let mut rotated = vec![T::default(); w*h];
        for (i, &value) in values.iter().enumerate() {
            rotated[self.index(i)] = value;
        }
        *values = rotated;
    }
    fn tiled_grid<T:Copy+Default>(&self,values:&mut Vec<T>) {
        let Some([w,h,dx,dy])=self.expansion else {self.grid(values);return;};
        let mut grown=Vec::with_capacity(w*h);
        for y in 0..h {for x in 0..w {
            let sx=(x as i64-dx as i64).rem_euclid(self.width as i64) as usize;
            let sy=(y as i64-dy as i64).rem_euclid(self.height as i64) as usize;
            grown.push(values[sy*self.width+sx]);
        }}
        *values=grown;
    }
    pub fn sparse(&self, old: &SparseDepositState) -> SparseDepositState {
        let (w,h)=self.size();
        let mut out = SparseDepositState::new(w,h);
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
                let angle = if self.expanded() {0.} else if self.clockwise { 90. } else { -90. };
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
        let old_page=doc.page_bounds();
        if self.expanded() {
            let old_current=std::mem::take(&mut doc.surface.current_height);
            self.tiled_grid(&mut doc.paper_albedo);
            self.tiled_grid(&mut doc.color_grain);
            self.tiled_grid(&mut doc.surface.rest_height);
            self.tiled_grid(&mut doc.surface.fiber);
            self.tiled_grid(&mut doc.surface.contact_support);
            self.tiled_grid(&mut doc.surface.edge_grain);
            doc.surface.current_height=doc.surface.rest_height.clone();
            for (i,v) in old_current.into_iter().enumerate(){doc.surface.current_height[self.index(i)]=v;}
            macro_rules! grow {($($field:ident),*)=>{$(self.grid(&mut doc.surface.$field);)*};}
            grow!(abrasion,graphite_mass,clay_mass,wax_mass,loose_mass,compacted_mass,orientation_x,orientation_y,color_r_mass,color_g_mass,color_b_mass);
            for layer in &mut doc.layers {layer.deposit=self.sparse(&layer.deposit);layer.vectors=self.layer(&layer.vectors);}
            doc.selection.quarter_turn(self);
            let (w,h)=self.size();
            doc.spec.width_px=w;doc.spec.height_px=h;
            doc.spec.width_mm=w as f32*25.4/doc.spec.dpi;doc.spec.height_mm=h as f32*25.4/doc.spec.dpi;
            let origin=self.point(Vec2::new(old_page[0] as f32,old_page[1] as f32));
            doc.page=Some([origin.x as usize,origin.y as usize,old_page[2],old_page[3]]);
            doc.mark_all_dirty();return;
        }
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
        if doc.page.is_some() {
            let a=self.point(Vec2::new(old_page[0] as f32,old_page[1] as f32));
            let b=self.point(Vec2::new((old_page[0]+old_page[2]) as f32,(old_page[1]+old_page[3]) as f32));
            let min=a.min(b);let size=(b-a).abs();
            doc.page=Some([min.x as usize,min.y as usize,size.x as usize,size.y as usize]);
        }
        doc.mark_all_dirty();
    }
}
