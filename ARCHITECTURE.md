# Graphite Studio architecture

## Design rule

The application treats a drawing as a **physical-ish material document** and the visible bitmap as a derived view.

```text
UI / platform input
        ↓
normalized stroke samples
        ↓
contact mechanics
        ↓
material simulation
        ↓
document surface channels
        ↓
renderer
        ↓
display texture / exported image
```

No UI module writes drawing pixels directly.

## Modules

### `input.rs`

Owns input normalization. v0.3 keeps a stateful `PressureInputState` that watches egui touch/pen events for normalized force, applies a tiny dead-zone and smoothing, and falls back to the configured mouse pressure when device force is unavailable.

This boundary is deliberately separate from the stroke engine. A later Windows Ink/Wintab implementation can replace or augment the platform pressure provider while continuing to emit the same normalized `StrokePoint` pressure values.

### `core/contact.rs`

Owns tip/paper contact geometry and force/contact evaluation. A document pixel is too coarse to represent individual fibers at preview DPI, so the model uses a homogenized sub-cell asperity distribution rather than a binary height threshold. Mean surface height shifts the effective contact fraction; unresolved high fibers provide a small force-dependent micro-contact component.

The pressure/contact geometry also lives here. v0.8 uses a rounded-conical/capsule approximation: pressure changes point radius and tilt exposes side contact. Input only supplies normalized force/orientation; UI code never decides pencil width.

### `core/document.rs`

Owns finite paper dimensions and persistent material state. It is independent of egui widgets, viewport coordinates, and file dialogs.

### `core/material.rs`

Current persistent channels include:

- original/rest paper height;
- current/evolving paper height;
- fiber field;
- abrasion;
- graphite mass;
- clay mass;
- wax mass;
- loose mass;
- compacted mass;
- axial orientation accumulator.

Darkness and sheen are intentionally not stored as paint channels.

### `core/paper.rs`

Creates deterministic paper microstructure sampled in millimeters. A future scanned-paper provider can implement the same role.

### `core/pencil.rs`

Contains generic grade formulations, tool settings, and the persistent `TipProfile` height map. Grade is a material formulation, not an opacity preset. The user-facing core diameter is physical millimeters, not brush radius. Wear changes the stored local profile, and barrel orientation is stored with the profile so rotation exposes different worn facets.

### `core/stroke.rs`

Owns interpolation, assembly/rasterization of the active footprint, contact queries, graphite transfer, smudger dispatch, paper deformation and erasure. Raw pointer polylines are converted by the app into midpoint quadratic Bézier segments and resampled here, so curve smoothing remains geometric and never blurs rendered pixels. Pencil footprint rasterization uses the rounded point/side envelope from `contact.rs`, then samples the persistent tip height profile before continuous paper-support and porous-edge statistics modulate material mass without binary pore pixels. It consumes pressure- and tilt-dependent geometry from `core/contact.rs`; it does not invent pressure curves in the UI path. It knows nothing about screen widgets or PNG output. This is the primary future GPU-compute target.


### `core/smudge.rs`

Owns the blending-stump material reservoir and directional material transport. It operates on graphite/clay/wax and loose/compacted state, never on rendered RGB pixels. Existing reservoir material is laid down toward the trailing side of a moving contact while the leading side preferentially picks up accessible loose material. The reservoir persists between smudge strokes and can be explicitly cleaned; document-history operations reset it because it is external tool state rather than part of the paper document.

### `core/history.rs`

Stores sparse per-stroke material patches rather than cloning the full document after every stroke. The app routes toolbar Undo/Redo and Ctrl+Z/Ctrl+Y through the same history operations.

### `core/viewport.rs`

Maps finite document coordinates to/from the camera. Pan/zoom cannot contaminate physical brush calculations.

### `render/`

`DocumentRenderer` is the optical rendering boundary. `RasterRenderer` derives paper/body tone and graphite response from material state. `GpuDisplayPyramid` owns a separate WGPU reconstruction texture: base-level dirty patches are uploaded and lower levels are regenerated in linear light for stable zoomed-out display. Preview reconstruction never feeds back into simulation or export state.

### `ui/`

Contains controls only. The main body uses egui's full-available-height horizontal layout so the drawing viewport fills the remaining window height independently of tool-panel content. v0.12.1 exposes physical **Core diameter (mm)** rather than a brush-size slider; broad pencil width is controlled primarily by tilt.

### `app.rs`

Composition root coordinating input, strokes, history, rendering, texture upload, document commands and UI.

## Intended GPU evolution

1. Preserve `Document`, normalized stroke samples, tool settings and UI contracts.
2. Move material channels to GPU textures/buffers.
3. Upload coalesced stroke samples into compact buffers.
4. Dispatch contact/deposition compute only over touched tiles.
5. Dispatch shading only over dirty tiles.
6. Keep a CPU serialization representation for save/recovery.

## Finite document strategy

The paper remains finite by design. Very large sheets should become sparse/demand-loaded tiles, while the camera can pan freely around them.


## Scroll/pan ownership

`core/viewport.rs` decides zoom, whether each sheet axis fits or overflows, and free-pan translation for fitting axes. `egui::ScrollArea` owns overflow offsets and native scrollbars. This keeps navigation state out of the graphite/material engine and allows right-button panning without turning left-drag drawing into a generic draggable scroll area.


## Export boundary (v0.4.0)

`src/export.rs` consumes renderer output and owns file-format encoding. 8-bit formats use the RGBA preview path; 16-bit PSD uses a direct high-precision RGB renderer path. PNG/BMP/JPEG/PSD support therefore remains downstream of the physical document and optical renderer. PSD stores the merged visible RGB image plus document DPI metadata and can be written at 8 or true 16 bits/channel. The 16-bit path renders directly from the floating-point optical model rather than upscaling the display texture; Graphite Studio material channels remain simulation state and are not misrepresented as conventional Photoshop paint layers.


## Layer architecture

The document has one shared physical paper support and an ordered stack of drawing layers. The active drawing layer lives in the dense `SurfaceState` deposit arrays so the pencil/smudge/eraser hot path does not pay indirection per cell. Inactive layers are serialized into sparse 32×32 `SparseDepositState` tiles. Switching layers captures the active deposit into sparse tiles, clears the dense deposit channels, then expands the selected layer into the active buffers.

This keeps paper deformation global and physically meaningful while making drawing-layer visibility/order Photoshop-like. Layer metadata is kept in `core/document.rs`; layer-panel widgets stay in `ui/layer_panel.rs`.

Pencil color is not stored as a layer tint. Each deposited cell carries mass-weighted RGB accumulators, so multiple pencil colors can coexist on one layer and the smudger transports those colors with the graphite/clay/wax material.


## v0.8 physical micro-contact boundary

```text
rounded-conical point / exposed side contact
                    ↓
coarse potential load on evolving paper height
                    ↓
~12 µm procedural cellulose-fiber / pore samples
                    ↓
sparse per-layer micro-deposit + aggregate material transfer
                    ↓
transparent drawing-layer optical state
```

The pencil hot path uses continuous precomputed paper-support/edge fields plus the evolving coarse paper height. The abandoned sparse micro-surface prototype was removed; paper albedo is never a destination for graphite.

## v0.6 contact/deposit boundary (legacy background)

The pencil engine now distinguishes the shared physical support from visible drawing material:

```text
paper albedo (immutable during drawing)
       +
shared height/fiber/abrasion state ──> coarse contact
                                      ↓
                           continuous paper-support/tooth response
                                      ↓
active transparent drawing layer (graphite/clay/wax/color)
                                      ↓
layer compositor over paper
```

`contact.rs` owns both the coarse contact calculation and the stroke-seeded pencil-face/micro-asperity selection. `stroke.rs` transfers material only after a micro-contact is accepted. No pencil path writes RGB into `paper_albedo`. The shared paper can still deform mechanically because subsequent layers should interact with the same physical sheet.
## GPU display pyramid

`render/display_pyramid.rs` is a display-only WGPU subsystem. It receives RGBA8 regions from the normal document renderer at mip level 0 and regenerates all lower resolution levels on GPU. Its shader performs sRGB -> linear-light -> box average -> sRGB reconstruction. The resulting native WGPU texture is registered with egui using a mip-aware sampler. No pyramid value is written back into paper state, drawing layers, history, smudger reservoirs, or exported pixels.



## v0.12 physical-core / persistent-facet boundary

`core/pencil.rs` owns the physical graphite-core diameter, the 32×32 pencil-local profile and the pending abrasion buffer. `core/contact.rs` owns paper-space projection: it derives a small sharpened apex near upright and transitions with tilt to a one-sided tapered side facet whose width is bounded by the physical core. Each candidate paper cell is then mapped into the rotated tip profile so existing local wear can perturb the contacting facet. `core/stroke.rs` is the only layer that writes wear, and it does so only after contact actually transfers graphite.

```text
physical core + stylus pressure + tilt + azimuth
                     ↓
small apex or one-sided tapered side facet
                     ↓
rotated TipProfile wear-recession sample
                     ↓
moderate side-area dilution + paper tooth
                     ↓
graphite transfer
                     ↓
exact contacted profile cells accumulate abrasion
```

Profile changes are deferred until stroke end. This prevents geometry from drifting within a single drag but allows gradual session-scale evolution. Cumulative wear is diagnostic state only; contact does not fall back to a global `flatness` value.

`src/bin/calibrate_graphite.rs` uses the library-facing core/renderer modules to generate controlled HB/2B/4B swatches and machine-readable metrics without UI input.

## v0.10 fixed-scale material / area-force boundary

`core/contact.rs` now has a strict separation between geometry and material texture. `PencilTipContact` owns projected rounded/side geometry and an estimated contact area. It converts total stylus force into a local force-density scale before `evaluate_contact` sees the paper. `evaluate_mesoscopic_contact` does not receive pencil diameter: it samples stable paper tooth and stroke-seeded graphite flakes at DPI-derived fixed frequencies, adds continuous pressure-dependent texture depth, and only uses tilt for side-contact orientation effects.

This removes the old `size_regime -> texture character` coupling. A larger pencil contact simply covers more cells of the same material fields.

## v0.9 real-time contact boundary

The live pencil path intentionally stops at document-resolution **coverage statistics**. `SurfaceState::contact_support` and `edge_grain` are compact 8-bit fields precomputed from the existing paper relief/fiber maps. `PencilTipContact::sample` produces geometric coverage plus a center-loaded rounded contact-pressure field. `evaluate_mesoscopic_contact` combines them with pressure, tilt, persistent tip-face contact and graphite-particle variation in O(1) work per affected document pixel.

The older sparse 12 µm micro-surface module is retained as experimental/reference code but is no longer touched by normal pencil strokes. This avoids HashMap/tile lookups and nine micro-samples for every raster pixel and dab.

