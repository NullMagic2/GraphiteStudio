use super::{
    material::{PixelDepositState, SparseDepositState, SurfaceState},
    paper::{generate_contact_response, generate_paper, PaperPreset},
};

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyRect {
    pub min_x: usize,
    pub min_y: usize,
    pub max_x: usize,
    pub max_y: usize,
}

impl DirtyRect {
    pub fn new(min_x: usize, min_y: usize, max_x: usize, max_y: usize) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    pub fn full(width: usize, height: usize) -> Self {
        Self::new(0, 0, width, height)
    }

    pub fn width(self) -> usize {
        self.max_x.saturating_sub(self.min_x)
    }
    pub fn height(self) -> usize {
        self.max_y.saturating_sub(self.min_y)
    }
    pub fn is_empty(self) -> bool {
        self.width() == 0 || self.height() == 0
    }

    pub fn union(self, other: Self) -> Self {
        Self {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct CanvasSpec {
    pub width_px: usize,
    pub height_px: usize,
    pub width_mm: f32,
    pub height_mm: f32,
    pub dpi: f32,
    pub name: String,
}

impl CanvasSpec {
    pub fn a4(dpi: f32) -> Self {
        Self::from_physical("A4", 210.0, 297.0, dpi)
    }
    pub fn a5(dpi: f32) -> Self {
        Self::from_physical("A5", 148.0, 210.0, dpi)
    }
    pub fn letter(dpi: f32) -> Self {
        Self::from_physical("Letter", 215.9, 279.4, dpi)
    }

    pub fn from_physical(name: impl Into<String>, width_mm: f32, height_mm: f32, dpi: f32) -> Self {
        let px_per_mm = dpi / 25.4;
        Self {
            width_px: (width_mm * px_per_mm).round().max(1.0) as usize,
            height_px: (height_mm * px_per_mm).round().max(1.0) as usize,
            width_mm,
            height_mm,
            dpi,
            name: name.into(),
        }
    }

    pub fn pixel_count(&self) -> usize {
        self.width_px.saturating_mul(self.height_px)
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlendMode {
    Normal,
    #[default]
    Multiply,
    Darken,
    Screen,
    Lighten,
    Saturation,
}

impl BlendMode {
    pub const ALL: [Self; 6] = [
        Self::Multiply,
        Self::Normal,
        Self::Darken,
        Self::Screen,
        Self::Lighten,
        Self::Saturation,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Multiply => "Multiply",
            Self::Darken => "Darken",
            Self::Screen => "Screen",
            Self::Lighten => "Lighten",
            Self::Saturation => "Saturation",
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct DrawingLayer {
    /// Global layer alpha, matching PSD's native 0..255 opacity field.
    #[serde(default = "full_layer_opacity")]
    pub opacity: u8,
    pub vectors: std::sync::Arc<super::vector::VectorLayer>,
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub blend_mode: BlendMode,
    /// Deposit channels for an inactive layer. The active layer's channels live in
    /// `Document::surface` so the simulation hot path stays contiguous and unchanged.
    pub deposit: SparseDepositState,
}

impl DrawingLayer {
    fn new(id: u64, name: impl Into<String>, width: usize, height: usize) -> Self {
        Self {
            opacity: full_layer_opacity(),
            vectors: Default::default(),
            id,
            name: name.into(),
            visible: true,
            blend_mode: BlendMode::default(),
            deposit: SparseDepositState::new(width, height),
        }
    }
}

fn full_layer_opacity() -> u8 {
    255
}
fn full_paper_texture_opacity()->f32 {1.}

#[derive(Debug, Clone, Default)]
pub struct LayerSelection {
    pub ids: Vec<u64>,
    pub anchor: Option<u64>,
    pub multiple: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Document {
    /// Original page within an expandable backing canvas: x, y, width, height.
    /// Older files use the entire canvas as their page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<[usize; 4]>,
    /// Workspace selection is per drawing and never part of the saved artwork.
    #[serde(skip)]
    pub layer_selection: LayerSelection,
    pub selection: super::selection::Selection,
    pub spec: CanvasSpec,
    pub paper: PaperPreset,
    pub paper_texture_label: String,
    /// Base support color only. Pencil/smudge/eraser never write visible marks here; graphite and
    /// pencil color live in drawing-layer deposit channels and are composited transparently.
    pub paper_albedo: Vec<[f32; 3]>,
    /// User-selected tint multiplied with the unchanged paper texture.
    pub paper_color_rgb: [u8; 3],
    #[serde(default = "full_paper_texture_opacity")]
    pub paper_texture_opacity: f32,
    /// Stable multiscale pigment variation, precomputed outside the stroke hot path.
    pub color_grain: Vec<u8>,
    pub surface: SurfaceState,
    pub layers: Vec<DrawingLayer>,
    pub(crate) active_layer: usize,
    pub(crate) next_layer_id: u64,
    pub(crate) revision: u64,
    dirty: Option<DirtyRect>,
}

impl Document {
    pub fn new(
        spec: CanvasSpec,
        paper: PaperPreset,
        paper_texture_label: impl Into<String>,
        paper_albedo: Vec<[f32; 3]>,
    ) -> Self {
        let width = spec.width_px;
        let height = spec.height_px;
        let n = width * height;
        assert_eq!(
            paper_albedo.len(),
            n,
            "paper albedo must match document size"
        );
        let (rest_height, fiber) = generate_paper(width, height, spec.dpi, paper);
        let (contact_support, edge_grain) =
            generate_contact_response(width, height, paper, &rest_height, &fiber);
        Self {
            page: None,
            color_grain: super::paper::generate_color_grain(width, height, spec.dpi),
            layer_selection: Default::default(),
            selection: Default::default(),
            spec,
            paper,
            paper_texture_label: paper_texture_label.into(),
            paper_color_rgb: [255; 3],
            paper_texture_opacity: 1.,
            paper_albedo,
            surface: SurfaceState::from_paper(rest_height, fiber, contact_support, edge_grain),
            layers: vec![DrawingLayer::new(1, "Layer 1", width, height)],
            active_layer: 0,
            next_layer_id: 2,
            revision: 1,
            dirty: Some(DirtyRect::full(width, height)),
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn page_bounds(&self) -> [usize;4] {
        self.page.unwrap_or([0,0,self.spec.width_px,self.spec.height_px])
    }
    pub fn page_origin(&self) -> eframe::egui::Vec2 {
        let p=self.page_bounds(); eframe::egui::vec2(p[0] as f32,p[1] as f32)
    }
    #[inline]
    pub fn index(&self, x: usize, y: usize) -> usize {
        y * self.spec.width_px + x
    }
    pub fn active_layer_index(&self) -> usize {
        self.active_layer
    }
    pub fn active_layer_id(&self) -> u64 {
        self.layers[self.active_layer].id
    }
    pub fn active_layer_name(&self) -> &str {
        &self.layers[self.active_layer].name
    }
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    pub fn can_merge_down(&self) -> bool {
        self.active_layer > 0 && self.layers[self.active_layer].visible && self.layers[self.active_layer - 1].visible
    }

    pub fn selected_layer_indices(&self) -> Vec<usize> {
        let mut selected: Vec<_> = self.layers.iter().enumerate()
            .filter_map(|(i,l)| self.layer_selection.ids.contains(&l.id).then_some(i)).collect();
        if selected.is_empty() { selected.push(self.active_layer); }
        selected
    }

    pub fn can_merge_selected(&self) -> bool {
        let indices = self.selected_layer_indices();
        indices.len() >= 2 && indices.iter().all(|&i| self.layers[i].visible)
    }

    pub fn select_layer(&mut self, index: usize, toggle: bool, range: bool) {
        if index >= self.layers.len() { return; }
        let id = self.layers[index].id;
        let mut ids: Vec<_> = self.selected_layer_indices().into_iter().map(|i| self.layers[i].id).collect();
        if range {
            let anchor = self.layer_selection.anchor.and_then(|id| self.layers.iter().position(|l| l.id==id)).unwrap_or(self.active_layer);
            if !toggle { ids.clear(); }
            for i in anchor.min(index)..=anchor.max(index) {
                if !ids.contains(&self.layers[i].id) { ids.push(self.layers[i].id); }
            }
            if self.layer_selection.anchor.is_none() { self.layer_selection.anchor = Some(self.layers[anchor].id); }
        } else if toggle {
            if ids.contains(&id) {
                if ids.len()>1 { ids.retain(|&i| i!=id); }
            } else { ids.push(id); }
            self.layer_selection.anchor = Some(id);
        } else {
            ids = vec![id];
            self.layer_selection.anchor = Some(id);
        }
        let active = if ids.contains(&id) { id } else if ids.contains(&self.active_layer_id()) { self.active_layer_id() } else { *ids.last().unwrap() };
        self.layer_selection.ids = ids;
        self.activate_layer_by_id(active);
    }

    pub fn set_paper_texture(&mut self, label: impl Into<String>, albedo: Vec<[f32; 3]>) {
        if albedo.len() == self.spec.pixel_count() {
            self.paper_texture_label = label.into();
            self.paper_albedo = albedo;
            self.mark_all_dirty();
        }
    }

    pub fn set_paper_color(&mut self, color: [u8; 3]) -> bool {
        if self.paper_color_rgb == color {
            return false;
        }
        self.paper_color_rgb = color;
        self.mark_all_dirty();
        true
    }
    pub fn set_paper_texture_opacity(&mut self,opacity:f32)->bool {
        if !opacity.is_finite(){return false;}
        let opacity=opacity.clamp(0.,1.);
        if self.paper_texture_opacity==opacity{return false;}
        self.paper_texture_opacity=opacity;self.mark_all_dirty();true
    }

    /// Switch active layers. The active layer remains in dense arrays for fast stroke simulation;
    /// inactive layers are serialized into sparse 32×32 tiles to keep multi-layer memory practical.
    pub fn activate_layer(&mut self, index: usize) -> bool {
        if index >= self.layers.len() || index == self.active_layer {
            return false;
        }
        let current = self.active_layer;
        {
            let (layers, surface) = (&mut self.layers, &mut self.surface);
            layers[current].deposit.capture_from_surface(surface);
            surface.clear_deposit();
            layers[index].deposit.load_into_surface(surface);
            layers[index].deposit.clear();
        }
        self.active_layer = index;
        self.mark_all_dirty();
        true
    }

    pub fn activate_layer_by_id(&mut self, id: u64) -> bool {
        let Some(index) = self.layers.iter().position(|layer| layer.id == id) else {
            return false;
        };
        self.activate_layer(index)
    }

    pub fn add_layer(&mut self) -> usize {
        let id = self.next_layer_id;
        self.next_layer_id = self.next_layer_id.wrapping_add(1).max(2);
        let name = format!("Layer {}", id);
        self.layers.push(DrawingLayer::new(
            id,
            name,
            self.spec.width_px,
            self.spec.height_px,
        ));
        let index = self.layers.len() - 1;
        self.activate_layer(index);
        index
    }

    pub fn remove_active_layer(&mut self) -> bool {
        if self.layers.len() <= 1 {
            return false;
        }

        // The active layer's actual deposit is in SurfaceState. Clear/discard it, remove the
        // metadata slot, then load the nearest surviving layer into the active surface buffers.
        self.surface.clear_deposit();
        let removed_index = self.active_layer;
        self.layers.remove(removed_index);
        let new_index = removed_index.min(self.layers.len() - 1);
        self.active_layer = new_index;
        {
            let (layers, surface) = (&mut self.layers, &mut self.surface);
            layers[new_index].deposit.load_into_surface(surface);
            layers[new_index].deposit.clear();
        }
        self.mark_all_dirty();
        true
    }

    pub fn set_layer_visible(&mut self, index: usize, visible: bool) -> bool {
        let Some(layer) = self.layers.get_mut(index) else {
            return false;
        };
        if layer.visible == visible {
            return false;
        }
        layer.visible = visible;
        self.mark_all_dirty();
        true
    }

    pub fn set_layer_blend_mode(&mut self, id: u64, mode: BlendMode) -> bool {
        let Some(layer) = self.layers.iter_mut().find(|layer| layer.id == id) else {
            return false;
        };
        if layer.blend_mode == mode {
            return false;
        }
        layer.blend_mode = mode;
        self.mark_all_dirty();
        true
    }

    pub fn set_layer_opacity(&mut self, id: u64, opacity: u8) -> bool {
        let Some(layer) = self.layers.iter_mut().find(|layer| layer.id == id) else {
            return false;
        };
        if layer.opacity == opacity {
            return false;
        }
        layer.opacity = opacity;
        self.mark_all_dirty();
        true
    }

    /// Insert the dragged layer above/below a target in the visible top-to-bottom
    /// stack. IDs keep the active dense material attached to its original layer.
    pub fn move_layer_relative(&mut self, dragged_id: u64, target_id: u64, above: bool) -> bool {
        if dragged_id == target_id {
            return false;
        }
        let Some(from) = self.layers.iter().position(|l| l.id == dragged_id) else {
            return false;
        };
        if !self.layers.iter().any(|l| l.id == target_id) {
            return false;
        }
        let active_id = self.active_layer_id();
        let moved = self.layers.remove(from);
        let target = self
            .layers
            .iter()
            .position(|l| l.id == target_id)
            .expect("target retained");
        let to = target + usize::from(above);
        self.layers.insert(to, moved);
        self.active_layer = self
            .layers
            .iter()
            .position(|l| l.id == active_id)
            .expect("active retained");
        if from == to {
            return false;
        }
        self.mark_all_dirty();
        true
    }

    pub fn rename_layer(&mut self, index: usize, name: String) -> bool {
        let Some(layer) = self.layers.get_mut(index) else {
            return false;
        };
        let trimmed = name.trim();
        if trimmed.is_empty() || layer.name == trimmed {
            return false;
        }
        layer.name = trimmed.to_owned();
        true
    }

    pub fn move_active_layer_up(&mut self) -> bool {
        if self.active_layer + 1 >= self.layers.len() {
            return false;
        }
        self.layers.swap(self.active_layer, self.active_layer + 1);
        self.active_layer += 1;
        self.mark_all_dirty();
        true
    }

    pub fn move_active_layer_down(&mut self) -> bool {
        if self.active_layer == 0 {
            return false;
        }
        self.layers.swap(self.active_layer, self.active_layer - 1);
        self.active_layer -= 1;
        self.mark_all_dirty();
        true
    }

    /// Return one pixel of a layer without requiring the renderer to know where the active layer
    /// is physically stored.
    pub fn layer_deposit_pixel(&self, layer_index: usize, index: usize) -> PixelDepositState {
        if layer_index == self.active_layer {
            self.surface.deposit_pixel(index)
        } else {
            self.layers[layer_index].deposit.get(index)
        }
    }

    pub fn layer_total_deposit(&self, layer_index: usize, index: usize) -> f32 {
        let p = self.layer_deposit_pixel(layer_index, index);
        p.graphite_mass + p.clay_mass + p.wax_mass
    }

    pub fn mark_dirty(&mut self, rect: DirtyRect) {
        if rect.is_empty() {
            return;
        }
        self.dirty = Some(match self.dirty.take() {
            Some(existing) => existing.union(rect),
            None => rect,
        });
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn take_dirty(&mut self) -> Option<DirtyRect> {
        self.dirty.take()
    }

    pub fn mark_all_dirty(&mut self) {
        // A full refresh supersedes queued regions, including bounds from the
        // previous orientation when several rotations happen before rendering.
        self.dirty = Some(DirtyRect::full(self.spec.width_px, self.spec.height_px));
        self.revision = self.revision.wrapping_add(1);
    }

    /// Remove deposited material from every drawing layer without restoring altered paper.
    pub fn clear_graphite(&mut self) {
        self.surface.clear_deposit();
        for (index, layer) in self.layers.iter_mut().enumerate() {
            if index != self.active_layer {
                layer.deposit.clear();
            }
        }
        self.mark_all_dirty();
    }

    pub fn graphite_coverage(&self) -> f32 {
        let n = self.spec.pixel_count();
        if n == 0 {
            return 0.0;
        }
        // Fast diagnostic estimate: visible layer occupancies are summed and clamped. Overlapping
        // pixels can therefore be counted twice, which is acceptable for this sidebar indicator.
        let mut occupied = 0usize;
        for (layer_index, layer) in self.layers.iter().enumerate() {
            if !layer.visible {
                continue;
            }
            if layer_index == self.active_layer {
                occupied += self
                    .surface
                    .graphite_mass
                    .iter()
                    .filter(|&&v| v > 0.002)
                    .count();
            } else {
                occupied += layer.deposit.occupied_pixel_count();
            }
        }
        (occupied as f32 / n as f32).clamp(0.0, 1.0)
    }

    pub fn mean_paper_disturbance(&self) -> f32 {
        if self.surface.abrasion.is_empty() {
            return 0.0;
        }
        self.surface.abrasion.iter().sum::<f32>() / self.surface.abrasion.len() as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paper::{generate_builtin_albedo, PaperTexturePreset};

    fn test_document() -> Document {
        let spec = CanvasSpec::from_physical("test", 10.0, 10.0, 100.0);
        let albedo = generate_builtin_albedo(
            spec.width_px,
            spec.height_px,
            spec.dpi,
            PaperTexturePreset::White,
        );
        Document::new(spec, PaperPreset::DrawingMedium, "White", albedo)
    }

    #[test]
    fn custom_a4_dpi_preserves_physical_size_and_changes_sampling() {
        let spec = CanvasSpec::a4(300.0);
        assert!((spec.width_mm - 210.0).abs() < f32::EPSILON);
        assert!((spec.height_mm - 297.0).abs() < f32::EPSILON);
        assert_eq!(spec.width_px, 2480);
        assert_eq!(spec.height_px, 3508);
        assert!((spec.dpi - 300.0).abs() < f32::EPSILON);
    }

    #[test]
    fn layer_switch_preserves_each_layers_material() {
        let mut doc = test_document();
        doc.surface.graphite_mass[0] = 0.25;
        doc.add_layer();
        assert_eq!(doc.surface.graphite_mass[0], 0.0);
        doc.surface.graphite_mass[0] = 0.75;
        doc.activate_layer(0);
        assert!((doc.surface.graphite_mass[0] - 0.25).abs() < 1.0e-6);
        doc.activate_layer(1);
        assert!((doc.surface.graphite_mass[0] - 0.75).abs() < 1.0e-6);
    }
}
