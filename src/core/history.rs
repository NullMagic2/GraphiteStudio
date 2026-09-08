use super::{
    document::{DirtyRect, Document},
    material::PixelSurfaceState,
};

#[derive(Debug, Clone)]
struct PixelChange {
    index: usize,
    before: PixelSurfaceState,
    after: PixelSurfaceState,
}

#[derive(Debug, Default)]
pub struct EditTransaction {
    // Capture each pixel once, then append its original state contiguously.
    // Broad tools avoid hashing and allocating buckets for every new pixel.
    remembered: Vec<u64>,
    vectors_before: Option<std::sync::Arc<super::vector::VectorLayer>>,
    layer_id: Option<u64>,
    before: Vec<(usize, PixelSurfaceState)>,
}

impl EditTransaction {
    #[inline]
    pub fn remember(&mut self, index: usize, document: &Document) {
        debug_assert!(self.layer_id.is_none_or(|id| id == document.active_layer_id()));
        if self.remembered.is_empty() {
            self.remembered = vec![0; document.spec.pixel_count().div_ceil(64)];
        }
        let bit = 1u64 << (index % 64);
        let word = &mut self.remembered[index / 64];
        if *word & bit != 0 { return; }
        *word |= bit;
        if self.vectors_before.is_none() {
            self.vectors_before = Some(
                document.layers[document.active_layer_index()]
                    .vectors
                    .clone(),
            );
        }
        let layer_id = document.active_layer_id();
        match self.layer_id {
            Some(existing) => debug_assert_eq!(
                existing, layer_id,
                "one stroke transaction must stay on one layer"
            ),
            None => self.layer_id = Some(layer_id),
        }
        self.before.push((index, document.surface.pixel(index)));
    }

    /// Keep the material outside an edited path's old/new footprint bit-identical.
    pub fn restore_outside(&self, document: &mut Document, regions: &[eframe::egui::Rect]) {
        let w = document.spec.width_px;
        for &(index, before) in &self.before {
            let p = eframe::egui::Pos2::new((index % w) as f32 + 0.5, (index / w) as f32 + 0.5);
            if !regions.iter().any(|r| r.contains(p)) {
                document.surface.set_pixel(index, before);
            }
        }
    }
    pub fn is_empty(&self) -> bool {
        self.before.is_empty()
    }

    /// Merge a completed outline into a larger undo/replay transaction at fixed opacity.
    pub fn finish_with_opacity(self, document: &mut Document, parent: &mut Self, opacity: f32) {
        for (index, before) in self.before {
            let after = document.surface.pixel(index);
            document.surface.set_pixel(index, before);
            parent.remember(index, document);
            document
                .surface
                .set_pixel(index, before.with_edit_opacity(after, opacity));
        }
    }

    /// Discard a provisional finger stroke when a second finger starts navigation.
    pub fn rollback(self, document: &mut Document) {
        if self.layer_id != Some(document.active_layer_id()) {
            return;
        }
        let mut bounds: Option<DirtyRect> = None;
        for (index, before) in self.before {
            document.surface.set_pixel(index, before);
            let (x, y) = (index % document.spec.width_px, index / document.spec.width_px);
            let pixel = DirtyRect::new(x, y, x + 1, y + 1);
            bounds = Some(bounds.map_or(pixel, |b| b.union(pixel)));
        }
        if let Some(vectors) = self.vectors_before {
            let active = document.active_layer_index();
            document.layers[active].vectors = vectors;
        }
        if let Some(bounds) = bounds { document.mark_dirty(bounds); }
    }

    fn commit(self, document: &Document) -> Option<HistoryEntry> {
        if self.before.is_empty() {
            return None;
        }
        let layer_id = self.layer_id.unwrap_or_else(|| document.active_layer_id());
        let mut changes = Vec::with_capacity(self.before.len());
        for (index, before) in self.before {
            let after = document.surface.pixel(index);
            if before != after {
                changes.push(PixelChange {
                    index,
                    before,
                    after,
                });
            }
        }
        (!changes.is_empty()
            || self.vectors_before.as_ref().is_some_and(|v| {
                !std::sync::Arc::ptr_eq(v, &document.layers[document.active_layer_index()].vectors)
            }))
        .then_some(HistoryEntry {
            mirror: None,
            copy: None,
            merge: None,
            turn: None,
            layer_id,
            changes,
            vectors_before: self.vectors_before.unwrap_or_default(),
            vectors_after: document.layers[document.active_layer_index()]
                .vectors
                .clone(),
        })
    }
}

#[derive(Debug, Clone)]
struct HistoryEntry {
    mirror: Option<bool>,
    copy: Option<Box<super::layer_copy::LayerCopy>>,
    merge: Option<Box<super::layer_merge::LayerMerge>>,
    turn: Option<bool>,
    vectors_before: std::sync::Arc<super::vector::VectorLayer>,
    vectors_after: std::sync::Arc<super::vector::VectorLayer>,
    layer_id: u64,
    changes: Vec<PixelChange>,
}

#[derive(Debug)]
pub struct History {
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    max_entries: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::new(64)
    }
}

impl History {
    fn rotate_state(&mut self, doc: &mut Document, clockwise: bool) {
        self.reorient_state(doc,super::orientation::QuarterTurn::new(doc.spec.width_px,doc.spec.height_px,clockwise));
    }
    fn mirror_state(&mut self,doc:&mut Document,horizontal:bool) {
        self.reorient_state(doc,super::orientation::QuarterTurn::mirror(doc.spec.width_px,doc.spec.height_px,horizontal));
    }
    fn reorient_state(&mut self,doc:&mut Document,mut turn:super::orientation::QuarterTurn) {
        // Transform history before the live document so all original Arc identities stay alive
        // while the cache deduplicates shared vector paths and raster bases.
        for entry in self.undo.iter_mut().chain(&mut self.redo) {
            if let Some(copy)=&mut entry.copy {copy.rotate(&mut turn);continue;}
            if let Some(merge) = &mut entry.merge { merge.rotate(&mut turn); continue; }
            if let Some(clockwise)=&mut entry.turn {
                if turn.reflection.is_some(){*clockwise=!*clockwise;}
                continue;
            }
            if let Some(horizontal)=&mut entry.mirror {
                if turn.reflection.is_none(){*horizontal=!*horizontal;}
                continue;
            }
            for change in &mut entry.changes {
                change.index = turn.index(change.index);
                for pixel in [&mut change.before, &mut change.after] {
                    (pixel.orientation_x,pixel.orientation_y)=turn.orientation(pixel.orientation_x,pixel.orientation_y);
                }
            }
            entry.vectors_before = turn.layer(&entry.vectors_before);
            entry.vectors_after = turn.layer(&entry.vectors_after);
        }
        turn.document(doc);
    }
    pub fn rotate_document(&mut self, doc: &mut Document, clockwise: bool) {
        self.redo.clear();
        self.rotate_state(doc, clockwise);
        self.undo.push(HistoryEntry {
            mirror: None,
            copy: None,
            merge: None,
            turn: Some(clockwise),
            layer_id: doc.active_layer_id(),
            changes: Vec::new(),
            vectors_before: Default::default(),
            vectors_after: Default::default(),
        });
        if self.undo.len() > self.max_entries {
            self.undo.remove(0);
        }
    }
    pub fn new(max_entries: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            max_entries: max_entries.max(1),
        }
    }

    pub fn merge_down(&mut self, doc: &mut Document) -> bool {
        if !doc.can_merge_down() { return false; }
        let upper=doc.active_layer_index();
        self.merge_layers(doc, &[upper-1,upper])
    }
    pub fn copy_layer(&mut self,doc:&mut Document)->bool {
        let Some(copy)=super::layer_copy::LayerCopy::prepare(doc) else{return false;};
        if !copy.apply(doc,true){return false;}
        self.redo.clear();
        self.undo.push(HistoryEntry {mirror:None,copy:Some(Box::new(copy)),merge:None,turn:None,layer_id:doc.active_layer_id(),changes:Vec::new(),vectors_before:Default::default(),vectors_after:Default::default()});
        if self.undo.len()>self.max_entries{self.undo.remove(0);}
        true
    }
    pub fn mirror_document(&mut self,doc:&mut Document,horizontal:bool) {
        self.redo.clear();self.mirror_state(doc,horizontal);
        self.undo.push(HistoryEntry {mirror:Some(horizontal),copy:None,merge:None,turn:None,layer_id:doc.active_layer_id(),changes:Vec::new(),vectors_before:Default::default(),vectors_after:Default::default()});
        if self.undo.len()>self.max_entries{self.undo.remove(0);}
    }
    pub fn merge_selected(&mut self, doc: &mut Document) -> bool {
        let indices=doc.selected_layer_indices();
        self.merge_layers(doc, &indices)
    }
    fn merge_layers(&mut self, doc: &mut Document, indices: &[usize]) -> bool {
        let Some(merge) = super::layer_merge::LayerMerge::prepare(doc, indices) else { return false; };
        if !merge.apply(doc, true) { return false; }
        self.redo.clear();
        self.undo.push(HistoryEntry { mirror: None, copy: None, merge: Some(Box::new(merge)), turn: None, layer_id: doc.active_layer_id(), changes: Vec::new(), vectors_before: Default::default(), vectors_after: Default::default() });
        if self.undo.len() > self.max_entries { self.undo.remove(0); }
        true
    }
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn push(&mut self, tx: EditTransaction, document: &Document) {
        let Some(entry) = tx.commit(document) else {
            return;
        };
        self.redo.clear();
        self.undo.push(entry);
        if self.undo.len() > self.max_entries {
            self.undo.remove(0);
        }
    }

    pub fn undo(&mut self, document: &mut Document) -> bool {
        let Some(entry) = self.undo.pop() else {
            return false;
        };
        if let Some(horizontal)=entry.mirror {
            self.mirror_state(document,horizontal);self.redo.push(entry);return true;
        }
        if let Some(copy)=&entry.copy {
            if !copy.apply(document,false){self.undo.push(entry);return false;}
            self.redo.push(entry);return true;
        }
        if let Some(merge) = &entry.merge {
            if !merge.apply(document, false) { self.undo.push(entry); return false; }
            self.redo.push(entry);
            return true;
        }
        if let Some(clockwise) = entry.turn {
            self.rotate_state(document, !clockwise);
            self.redo.push(entry);
            return true;
        }
        if document.active_layer_id() != entry.layer_id
            && !document.activate_layer_by_id(entry.layer_id)
        {
            // Layer was removed; this history entry can no longer be applied safely.
            return false;
        }
        let dirty = apply_entry(document, &entry, false);
        document.mark_dirty(dirty);
        self.redo.push(entry);
        true
    }

    pub fn redo(&mut self, document: &mut Document) -> bool {
        let Some(entry) = self.redo.pop() else {
            return false;
        };
        if let Some(horizontal)=entry.mirror {
            self.mirror_state(document,horizontal);self.undo.push(entry);return true;
        }
        if let Some(copy)=&entry.copy {
            if !copy.apply(document,true){self.redo.push(entry);return false;}
            self.undo.push(entry);return true;
        }
        if let Some(merge) = &entry.merge {
            if !merge.apply(document, true) { self.redo.push(entry); return false; }
            self.undo.push(entry);
            return true;
        }
        if let Some(clockwise) = entry.turn {
            self.rotate_state(document, clockwise);
            self.undo.push(entry);
            return true;
        }
        if document.active_layer_id() != entry.layer_id
            && !document.activate_layer_by_id(entry.layer_id)
        {
            return false;
        }
        let dirty = apply_entry(document, &entry, true);
        document.mark_dirty(dirty);
        self.undo.push(entry);
        true
    }
}

fn apply_entry(document: &mut Document, entry: &HistoryEntry, use_after: bool) -> DirtyRect {
    let active = document.active_layer_index();
    document.layers[active].vectors = if use_after {
        entry.vectors_after.clone()
    } else {
        entry.vectors_before.clone()
    };
    if entry.changes.is_empty() {
        return DirtyRect::full(document.spec.width_px, document.spec.height_px);
    }
    let width = document.spec.width_px;
    let mut min_x = width;
    let mut min_y = document.spec.height_px;
    let mut max_x = 0usize;
    let mut max_y = 0usize;

    for change in &entry.changes {
        let state = if use_after {
            change.after
        } else {
            change.before
        };
        document.surface.set_pixel(change.index, state);
        let x = change.index % width;
        let y = change.index / width;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x + 1);
        max_y = max_y.max(y + 1);
    }
    DirtyRect::new(min_x, min_y, max_x, max_y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{
        document::CanvasSpec,
        paper::{generate_builtin_albedo, PaperPreset, PaperTexturePreset},
    };

    fn test_document() -> Document {
        let spec = CanvasSpec::from_physical("test", 8.0, 8.0, 100.0);
        let albedo = generate_builtin_albedo(
            spec.width_px,
            spec.height_px,
            spec.dpi,
            PaperTexturePreset::White,
        );
        Document::new(spec, PaperPreset::DrawingMedium, "White", albedo)
    }

    #[test]
    fn rollback_restores_repeated_edits_and_only_invalidates_their_bounds() {
        let mut doc = test_document();
        doc.take_dirty();
        let mut tx = EditTransaction::default();
        let a = doc.index(3, 5);
        let b = doc.index(9, 8);
        tx.remember(a, &doc);
        doc.surface.graphite_mass[a] = 0.5;
        tx.remember(a, &doc);
        doc.surface.graphite_mass[a] = 0.8;
        tx.remember(b, &doc);
        doc.surface.graphite_mass[b] = 0.9;
        assert_eq!(tx.before.len(), 2);
        tx.rollback(&mut doc);
        assert_eq!(doc.surface.graphite_mass[a], 0.);
        assert_eq!(doc.surface.graphite_mass[b], 0.);
        let dirty = doc.take_dirty().unwrap();
        assert_eq!((dirty.min_x, dirty.min_y, dirty.max_x, dirty.max_y), (3, 5, 10, 9));
    }

    #[test]
    fn undo_reactivates_the_layer_that_owned_the_stroke() {
        let mut doc = test_document();
        let original_layer = doc.active_layer_id();
        let mut history = History::default();
        let mut tx = EditTransaction::default();
        tx.remember(0, &doc);
        doc.surface.graphite_mass[0] = 0.4;
        doc.surface.loose_mass[0] = 0.4;
        history.push(tx, &doc);

        doc.add_layer();
        assert_ne!(doc.active_layer_id(), original_layer);
        assert!(history.undo(&mut doc));
        assert_eq!(doc.active_layer_id(), original_layer);
        assert_eq!(doc.surface.graphite_mass[0], 0.0);
    }
}
