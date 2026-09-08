use super::{document::{BlendMode, Document, DrawingLayer}, material::SparseDepositState, orientation::QuarterTurn, vector::VectorLayer};
use std::{collections::BTreeSet, sync::Arc};

/// Retain only selected layers for Undo. Unselected layers and shared paper stay independent.
#[derive(Debug, Clone)]
pub(crate) struct LayerMerge {
    before: Vec<(usize, DrawingLayer)>,
    active_before: u64,
    after: DrawingLayer,
}
impl LayerMerge {
    pub fn prepare(doc: &Document, indices: &[usize]) -> Option<Self> {
        let mut indices = indices.to_vec(); indices.sort_unstable(); indices.dedup();
        if indices.len()<2 || indices.iter().any(|&i| doc.layers.get(i).is_none_or(|l| !l.visible)) { return None; }
        let before: Vec<_> = indices.iter().map(|&i| {
            let mut layer = doc.layers[i].clone();
            if i == doc.active_layer_index() { layer.deposit.capture_from_surface(&doc.surface); }
            (i,layer)
        }).collect();
        let first = &before[0].1;
        let mode = first.blend_mode;
        let independent = matches!(mode, BlendMode::Normal | BlendMode::Multiply | BlendMode::Screen) && before.iter().all(|(_,l)| l.blend_mode==mode);
        let mut after = DrawingLayer {
            id: first.id, name: first.name.clone(), visible: true, opacity: 255,
            blend_mode: if independent { mode } else { BlendMode::Normal },
            vectors: Default::default(), deposit: SparseDepositState::new(doc.spec.width_px,doc.spec.height_px),
        };
        let tiles: BTreeSet<_> = before.iter().flat_map(|(_,l)| l.deposit.tiles.keys().copied()).collect();
        let columns = doc.spec.width_px.div_ceil(32);
        for tile in tiles {
            for y in tile / columns * 32..((tile / columns + 1)*32).min(doc.spec.height_px) {
                for x in tile % columns * 32..((tile % columns + 1)*32).min(doc.spec.width_px) {
                    let i=doc.index(x,y);
                    after.deposit.set(i, crate::render::merge_layer_pixel(doc,&indices,i,after.blend_mode,independent));
                }
            }
        }
        after.vectors=Arc::new(VectorLayer { base:Some(Arc::new(after.deposit.clone())),strokes:Vec::new() });
        Some(Self { before,active_before:doc.active_layer_id(),after })
    }
    pub fn apply(&self, doc: &mut Document, redo: bool) -> bool {
        let ids: Vec<_> = self.before.iter().map(|(_,l)|l.id).collect();
        let Some(current) = doc.layers.iter().position(|l|l.id==self.after.id) else { return false; };
        let insert = if redo {
            let positions: Option<Vec<_>> = ids.iter().map(|id|doc.layers.iter().position(|l|l.id==*id)).collect();
            let Some(positions)=positions else {return false;};
            positions.iter().max().unwrap() + 1 - positions.len()
        } else {
            if doc.layers.iter().any(|l| l.id!=self.after.id && ids.contains(&l.id)) {return false;}
            current
        };
        // Flush any unrelated active material before replacing the working layer.
        doc.activate_layer(current);
        doc.surface.clear_deposit();
        if redo {
            doc.layers.retain(|l|!ids.contains(&l.id));
            doc.layers.insert(insert,self.after.clone());
            doc.active_layer=insert;
            doc.layer_selection.ids=vec![self.after.id];
            doc.layer_selection.anchor=Some(self.after.id);
        } else {
            doc.layers.remove(current);
            for (index,layer) in &self.before { doc.layers.insert((*index).min(doc.layers.len()),layer.clone()); }
            doc.active_layer=doc.layers.iter().position(|l|l.id==self.active_before).unwrap_or(0);
            doc.layer_selection.ids=ids;
            doc.layer_selection.anchor=Some(self.active_before);
        }
        let active=doc.active_layer;
        doc.layers[active].deposit.load_into_surface(&mut doc.surface);
        doc.layers[active].deposit.clear();
        doc.selection=Default::default();
        doc.mark_all_dirty();
        true
    }
    pub fn rotate(&mut self, turn: &mut QuarterTurn) {
        for layer in self.before.iter_mut().map(|(_,l)|l).chain(std::iter::once(&mut self.after)) {
            layer.deposit=turn.sparse(&layer.deposit);
            layer.vectors=turn.layer(&layer.vectors);
        }
    }
}
