use eframe::egui::ColorImage;

use crate::core::document::{DirtyRect, Document};

/// Display/export boundary for a physical document state.
///
/// The CPU rasterizer implements this today. A future GPU backend can keep the same
/// high-level contract while moving paper/graphite shading into compute/render passes.
pub trait DocumentRenderer {
    fn reset(&mut self);
    fn render_full(&mut self, document: &Document) -> ColorImage;
    fn render_region(&mut self, document: &Document, rect: DirtyRect) -> ColorImage;
    fn rgba8(&mut self, document: &Document) -> Vec<u8>;

    /// High-precision RGB rendering for 16-bit/channel export. This is derived directly from the
    /// floating-point optical model rather than expanding the cached 8-bit display texture.
    fn rgb16(&mut self, document: &Document) -> Vec<u16>;
}
