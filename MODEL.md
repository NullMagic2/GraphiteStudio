# Graphite Studio v0.12.1 material model



## v0.12.1 pressure-grown tapered apex

The upright contact is no longer treated as an almost fixed nib. Raw stylus pressure is normalized linearly, then one shared graphite-force response advances the paper/contact plane down the sharpened cone. The apex therefore gains contact width substantially under load while remaining bounded by the physical core. Tilt makes the cone intersection mildly elliptical before true side-facet contact begins.

Mesoscopic texture no longer gets a new random flake-density realization per stroke. The dominant signals are the stable paper-support field, fixed physical-scale relief, and the persistent worn tip profile. This is intended to remove the stamped digital-brush appearance while keeping graphite tooth crisp.

## v0.12 physical-core / pointed-to-facet contact

The pencil no longer has an arbitrary document-pixel brush diameter. Each pencil owns a **physical graphite-core diameter in millimeters** plus a persistent 32×32 tip-wear profile in pencil-local coordinates. A near-upright sharpened pencil contacts the paper through only a small apex. Increasing tilt progressively exposes a **one-sided tapered side facet behind the stylus tip**; the facet width remains bounded by the physical core instead of growing into a circular brush stamp.

Successful moving contact writes abrasion back into the exact profile cells that touched. Wear is accumulated during a stroke and committed on release so one continuous stroke keeps stable geometry, while later strokes see the evolved face. Barrel rotation rotates the stored profile relative to paper and can expose a sharper unworn facet. Cumulative `wear_work` is retained only as a diagnostic/calibration scalar; it never globally enlarges the point.

Pressure primarily changes total normal force and graphite transfer. It changes the upright apex width only moderately, while **tilt is the primary control for broad marks**. Material release therefore uses only a mild additional pressure-efficiency term; this avoids applying the same steep pressure curve twice and prevents the old nearly-invisible-normal-pressure / suddenly-charcoal-hard-pressure response.

## v0.10 contact-area / fixed-scale texture correction (historical)

The visible graphite texture is no longer parameterized by pencil diameter. `PencilTipContact` determines only the projected contact geometry; the paper-tooth and graphite-flake fields are sampled at fixed DPI-derived physical frequencies. A larger point therefore intersects more of the same-scale texture rather than scaling or blurring it.

In v0.10, stylus pressure was treated as a total-force signal and a rounded/capsule envelope estimated contact area before paper contact. That envelope has been superseded by the v0.12 pointed-to-facet geometry, but the total-force principle remains: genuinely broad side contact is lighter per unit area at the same stylus force.

`evaluate_mesoscopic_contact` now implements continuous texture depth: low force preferentially reaches raised fiber/support regions, higher force progressively reaches lower tooth, and a small minimum-depth term prevents binary white pore pixels. Directional streaking is controlled by side-contact/tilt, not by brush size. Repeated stroke seeds shed different fixed-scale graphite flake patterns, allowing multiple light passes to build tone progressively.

## v0.9 mesoscopic-contact correction (historical)

v0.8 proved too expensive and visually over-averaged: it sampled a ~12 µm sparse fiber grid repeatedly during every dab and immediately collapsed those samples into one document-resolution material value. v0.9 removes that micro-grid from the interactive pencil hot path. Individual cellulose fibers remain the physical motivation, but at ordinary 120–300 DPI one document pixel contains many fibers, so the real-time solver stores a **continuous statistical fiber-top coverage** per document pixel rather than literal binary pores.

The rounded/tilted tip now supplies a center-loaded contact-pressure field. Paper support, edge grain, persistent side-contact ridges, graphite-particle density, pressure and material formulation continuously modulate deposited mass. Whole raster pixels are no longer switched between graphite and blank paper to represent microscopic pores.

The abandoned sparse micro-surface prototype has been removed from the active source tree; the real-time pencil uses the continuous paper-support/contact model described here.

## v0.8 tip geometry and stroke termini (historical)

The old pressure-scaled ellipse has been replaced in the pencil path by a rounded capsule approximation to a **rounded conical point plus exposed side contact**. At low force the point can approach a very small contact radius; pressure expands that point contact, and tilt exposes an increasingly long side patch. The user-facing pixel size remains the uniform scale parameter.

Many tablet stacks end contact before reporting a perfect zero-force sample. v0.8 reconstructs a short physically mandatory release interval: tangentially moving lifts taper over a distance related to recent motion, while nearly stationary lifts remain rounded. Pen-down similarly begins with a small reconstructed contact so moving strokes can grow naturally from a fine entry.

## Particle tone

`Particle variation` controls restrained per-microcontact graphite value variation. Samples are clamped to a dark-gray/value range and are mass-weighted into the drawing layer color. The control cannot generate white particles or write into the paper texture.

This document separates **mechanism** from **calibration**. The mechanisms are chosen to resemble observed pencil/paper behavior; many constants are still provisional and require controlled real-world calibration.

## 1. Paper and simulation scale

Each cell has immutable `rest_height` and evolving `current_height`. Relief/fiber wavelengths are sampled in millimeters so the same paper preset keeps approximately the same physical scale at different document DPI settings.

A key v0.3 correction is that a 120–150 dpi cell is treated as a **coarse surface patch**, not as one literal paper fiber. One simulation pixel is on the order of tenths of a millimeter, while real paper microstructure contains many smaller features.

## 2. Coarse contact + mesoscopic fiber-coverage statistics

The document-resolution height field remains useful for broad relief, deformation, and capacity, but it is no longer asked to represent literal pores. `evaluate_contact` first estimates coarse potential contact from current surface height, pressure, hardness and compliance.

`SurfaceState::contact_support` then represents the continuous fraction of unresolved fiber tops available inside each document pixel, while `edge_grain` perturbs only the final sub-pixel of the geometric boundary. `evaluate_mesoscopic_contact` combines those fields with fixed-scale graphite flake fields and continuous pressure penetration. Pencil diameter is not an input to this texture stage. Tilt may reveal fixed-scale side-contact flakes, while persistent profile contact supplies the coherent facet/streak modulation; brush size never rescales grain. The output changes deposited **mass continuously** rather than deciding that an entire raster pixel is graphite or blank paper.

The legacy ~12 µm micro-surface remains experimental/reference code but is not queried by normal pencil strokes. Paper RGB remains independent from all drawing-layer deposits.

## 3. Pressure

Every `StrokePoint` carries pressure in the range 0..1. The material engine has no distinction between mouse and tablet sources.

`input.rs` currently resolves the source:

- device force from egui touch/pen packets when available;
- otherwise the mouse fallback slider.

Device force receives a very small dead-zone and mild temporal smoothing. Pressure is preserved at the endpoints of stroke segments and interpolated through generated dabs.

Pressure affects **normal force strongly and point geometry moderately**. For the sharpened apex, the current normalized contact-radius fraction is approximately:

```text
point fraction of core radius ≈ 0.070 + 0.230 × pressure^0.68

total-force drive ≈ 0.25 × pressure^0.45 + 1.15 × pressure^1.50
```

Device pressure receives a mild input remap (`pressure^0.82`) before entering the material engine so ordinary stylus force sits in a useful drawing range. Tilt separately exposes the long side facet; pressure therefore cannot turn an upright pencil into a broad marker.

Pressure still affects contact fraction, normal load, compaction and material transfer. It is **not** converted directly into brush opacity. A true zero-pressure sample now deposits no material.

## 4. Pencil formulation

A generic formulation contains:

```text
graphite fraction
clay fraction
wax fraction
core hardness
relative wear coefficient
lubricity
point retention
```

The 4H→8B family is research-informed but not manufacturer-specific.

## 5. Persistent tip state

```text
physical graphite-core diameter in millimeters
cumulative wear work (diagnostic only)
32×32 pencil-local tip height profile
persistent barrel orientation
pending per-cell wear for the active stroke
```

The tip persists between strokes and can be sharpened. The user-facing **core diameter** is expressed in physical millimeters. Near-upright contact uses only a small sharpened apex; tilt exposes a longer side facet whose width remains bounded by the core. The actual local contact shape is further refined by the stored profile height field; there is no scalar flatness or arbitrary brush-radius multiplier.

For each successful moving contact, local load × sliding distance × grade abrasion response is deposited into nearby profile cells by bilinear weights. Softer/high-wear formulations abrade faster and point retention resists wear. On stroke release those pending values lower the contacted height samples with a small local smoothing step, mimicking rounded abrasion rather than square cell carving.

`TipProfile::orientation_deg` rotates profile sampling in the pencil frame. Repeated drawing at one orientation can therefore wear a face, while rotating the barrel presents a different edge/facet. Sharpening regenerates the fresh profile while preserving the current barrel orientation.

`wear_work` and `wear_fraction()` remain useful for status displays and calibration reports only. They do **not** enlarge contact geometry or dab spacing. This is the key distinction from the old scalar wear model.

## 6. Stroke sampling

Stroke segments are resampled from the physical core diameter with a dense upper spacing bound. This prevents visible dab repetition along long tilted facets while keeping the narrow sharpened point continuous. Sampling density is no longer driven by an arbitrary brush-size control.

## 7. Display sampling

The optical document raster remains the export/base raster. The WGPU preview owns an explicit multiresolution pyramid: level 0 is the exact document composite, and lower levels are generated in linear light before sRGB re-encoding. Zoomed-out views therefore integrate microstructure without backend-dependent aliasing. This presentation pipeline never feeds back into material state.

## 8. Material transfer

For a contacted cell:

```text
release ∝
    formulation wear
  × material-flow setting
  × local normal load
  × contact fraction
  × paper/fiber capture
  × side-contact factor
  × remaining local capacity
```

Released material is partitioned into graphite, clay and wax.

## 9. Loose and compacted states

Fresh material is mostly loose. Pressure packs part of the new deposit and can compact existing loose material.

```text
loose_mass + compacted_mass ≈ total deposited core mass
```

Composition and mechanical state describe the same deposited matter from different perspectives.

## 10. Paper evolution

Loaded contact gradually flattens raised asperities. Erasing can abrade the support. Removing graphite does not restore the original paper geometry.

## 11. Orientation and optics

Deposited material stores an axial orientation vector using `cos(2θ)` and `sin(2θ)`. Parallel strokes reinforce coherence; crossing strokes reduce it.

The renderer derives darkness and graphite glare from composition, amount, packing, coherence and a virtual light direction. It does not read stored grade-specific darkness or sheen channels.

## 12. Erasing

The current eraser is a generic hard/vinyl-like lifting model. Loose material is easier to remove than compacted deposit; deep valleys are less accessible; aggressive erasing can alter paper state.

## 13. Still uncalibrated

- absolute pressure/force mapping;
- pressure curve for specific tablets;
- contact distribution parameters;
- physical wear coefficients;
- deposition quantities;
- paper compliance/abrasion constants;
- optical extinction/specular constants;
- exact manufacturer formulations;
- tip wear constants.

The next scientific step remains controlled swatch calibration with known pencil, paper, force, tilt, pass count and illumination.

## 14. Invariants to preserve

1. Paper geometry is mutable.
2. Grade is formulation, not opacity.
3. Material transfer follows contact and wear.
4. Coarse raster resolution must not be mistaken for literal fiber resolution.
5. Composition and mechanical state stay distinct.
6. Darkness and sheen are render-time consequences.
7. Tip state persists.
8. Smudging/erasing move or remove material rather than blur/paint pixels.
9. The finite sheet remains independent of camera pan/zoom.
10. Paper RGB/albedo is never painted by pencil strokes; visible material belongs only to transparent drawing layers. Shared paper height/abrasion may still evolve mechanically.
## 15. Smudging / blending stump

Smudging is modeled as **material transport**, not an image-space blur. The tool has an external reservoir containing graphite, clay, wax and a mass-weighted orientation vector. During motion:

```text
loaded stump -> trailing-side redeposition
leading-side contact -> pickup of accessible paper material -> stump reservoir
remaining loose deposit -> slight pressure-dependent compaction
```

Loose graphite is substantially more mobile than compacted material. Raised/accessible deposits are easier to pick up, while local paper valley/fiber structure affects redeposition. Motion also partially reorients redeposited graphite through shear. The model conserves composition mass between the paper and stump reservoir (apart from floating-point error).

The reservoir is deliberately external to `Document`: it represents contamination of the physical blending stump, not material still on the paper. Because current undo/redo stores document patches only, history navigation resets the stump reservoir to avoid hidden state inconsistent with the restored document. A future generalized tool-state history could preserve reservoir snapshots exactly.

## 16. 16-bit optical export

The interactive display remains 8-bit because egui's preview texture is RGBA8, but 16-bit PSD export evaluates the optical model directly to floating-point RGB and quantizes once to 0..65535. It therefore preserves tonal precision that would be lost if 8-bit preview samples were merely multiplied by 257.
## 17. Footprint sharpness, micro-contact and curve interpolation

The rounded point/side capsule describes the **potential geometric contact region**, not a visible brush mask. Its boundary is narrowly antialiased, while v0.8 resolves the interior through physically scaled fiber/pore samples. A broad mark can therefore be macroscopically diffuse without using white-hole masks or an airbrush opacity ramp.

Pointer sampling is a separate problem from material physics. Raw positions are converted to midpoint quadratic Bézier segments with continuous tangents and then resampled into ordinary physical dabs. No post-stroke image blur or vector overlay is used; the document still contains only simulated deposited material.



## 18. Drawing layers

The finite paper support remains a single physical surface. Drawing layers are optical/material overlays above that support: each layer stores independent deposited graphite/clay/wax, loose/compacted state, orientation, and mass-weighted pencil color. Paper `rest_height`, `current_height`, fibers, abrasion and albedo are shared. This means drawing on any layer can still mechanically affect the same sheet while layer visibility controls only whether that layer's deposited material participates in rendering.

The active layer uses dense simulation arrays. Inactive layers are packed into sparse 32×32 material tiles to avoid multiplying full-document memory for mostly empty layers.

## 19. Pencil color

Pencil color is deliberately separate from grade formulation. Grade continues to control graphite/clay/wax composition, hardness, wear, lubricity and point retention. Each material transfer also accumulates the selected sRGB body color weighted by deposited mass. Overlapping colors on one layer therefore mix by deposited material quantity, erasing removes the corresponding color mass, and smudging carries/redeposits it with the same material reservoir. The renderer derives final chroma from the mass-weighted color while preserving grade-derived optical density and graphite-like directional glare.
## Display reconstruction is not simulation

The multiresolution GPU pyramid is strictly a viewing transform. Pencil/paper contact, deposited material, layer transparency, smudging, erasing and paper deformation continue to be evaluated at document resolution. The pyramid only reconstructs those full-resolution optical results for a smaller on-screen footprint; therefore it must never be used as simulation input.

