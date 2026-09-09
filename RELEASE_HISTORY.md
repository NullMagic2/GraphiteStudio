# Release history

All Graphite Studio release notes and changelog entries, newest first. Historical descriptions and measurements reflect their respective versions; see [README.md](README.md) for current features and build instructions.

<details>
<summary>Browse releases</summary>

- [v0.24.18](#v02418)
- [v0.24.17](#v02417)
- [v0.24.16](#v02416)
- [v0.24.15](#v02415)
- [v0.24.14](#v02414)
- [v0.24.13](#v02413)
- [v0.24.12](#v02412)
- [v0.24.11](#v02411)
- [v0.24.10](#v02410)
- [v0.24.9](#v0249)
- [v0.24.8](#v0248)
- [v0.24.7](#v0247)
- [v0.24.6](#v0246)
- [v0.24.5](#v0245)
- [v0.24.4](#v0244)
- [v0.24.3](#v0243)
- [v0.24.2](#v0242)
- [v0.24.1](#v0241)
- [v0.24.0](#v0240)
- [v0.23.9](#v0239)
- [v0.23.8](#v0238)
- [v0.23.7](#v0237)
- [v0.23.6](#v0236)
- [v0.23.5](#v0235)
- [v0.23.4](#v0234)
- [v0.23.3](#v0233)
- [v0.23.2](#v0232)
- [v0.23.1](#v0231)
- [v0.23.0](#v0230)
- [v0.22.9](#v0229)
- [v0.22.8](#v0228)
- [v0.22.7](#v0227)
- [v0.22.6](#v0226)
- [v0.22.5](#v0225)
- [v0.22.4](#v0224)
- [v0.22.3](#v0223)
- [v0.22.2](#v0222)
- [v0.22.1](#v0221)
- [v0.22.0](#v0220)
- [v0.21.1](#v0211)
- [v0.21.0](#v0210)
- [v0.20.0](#v0200)
- [v0.19.2](#v0192)
- [v0.19.1](#v0191)
- [v0.19.0](#v0190)
- [v0.18.0](#v0180)
- [v0.17.0](#v0170)
- [v0.16.0](#v0160)
- [v0.15.0](#v0150)
- [v0.14.0](#v0140)
- [v0.13.0](#v0130)
- [v0.12.1](#v0121)
- [v0.12.0](#v0120)
- [v0.11.2](#v0112)
- [v0.11.1](#v0111)
- [v0.11.0](#v0110)
- [v0.10.0](#v0100)
- [v0.9.3](#v093)
- [v0.9.2](#v092)
- [v0.9.1](#v091)
- [v0.9.0](#v090)
- [v0.8.0](#v080)
- [v0.7.0](#v070)
- [v0.6.3](#v063)
- [v0.6.2](#v062)
- [v0.6.1](#v061)
- [v0.6.0](#v060)
- [v0.5.0](#v050)
- [v0.4.2](#v042)
- [v0.4.1](#v041)
- [v0.4.0](#v040)
- [v0.3.4](#v034)
- [v0.3.3](#v033)
- [v0.3.2](#v032)
- [v0.3.1](#v031)
- [v0.3.0](#v030)
- [v0.2.1](#v021)
- [v0.1.0](#v010)

</details>

## v0.24.18

- Minimal preview fix: growing the artwork backing store no longer extends the visible paper texture. Outside the original page, only artwork is shown over the workspace. Drawing, movement, page coordinates and export behavior are unchanged.

## v0.24.17

- The first canvas contact after changing brush size now closes the popup and draws immediately, instead of consuming an extra click. Slider drags stay blocked until lift.
- Recover stale control ownership on fresh pen contact, and recover missing Up packets when the driver reports Hover or a new Down. Unowned Move packets cannot start phantom strokes.
- Added Gaomon guidance for the existing Windows Ink/Wintab backends and driver-reported 8192/16384-level pressure range coverage. No driver installation or system settings are changed.
- Added native input, first-stroke-after-size, and missed-release regression tests. Physical Gaomon hardware was not available for verification.

## v0.24.16

- Added Saturation to the layer blend menu and the shared preview/export compositor, using nonseparable hue/luminosity-preserving saturation blending with opacity and gamut handling.
- Recognize and write Photoshop's `sat ` blend key instead of rejecting the layer. Preserve individual layers and their modes in 8-bit/16-bit PSD and Graphite round trips.
- Added independent saturation PSD fixtures, reference-color/gamut/opacity checks, and round trips through both native editable saves and standard PSD records without Graphite metadata.

## v0.24.15

- Improved Liquify responsiveness by sampling original material once per presented batch, with duplicate output pixels removed. Batched results match sequential material exactly.
- Reduced field lookup and CPU fallback overhead; retained GPU shaders, background processing and adaptive antialiasing.
- Large brushes publish smaller batches. Push skips stationary work and combines redundant straight samples without merging corners, pressure changes or distortion.
- Push uses a centered swept footprint; momentum decays continuously and stops throwing after a pause before pen lift, making stroke following more natural.
- A CPU benchmark of eight queued 600-pixel brush steps on a 768-pixel canvas improved from 1017/1046/862 ms to 528/567/459 ms for Push/Twirl Right/Crystals (median of three runs). These are worker timings, not display frame rates.

## v0.24.14

- Added a drag handle to the fullscreen toolbar. Moving it retains button functionality, prevents accidental drawing, and preserves its position when toggling fullscreen.
- Added Liquify directly below Transform with an original turquoise droplet/spiral icon in both palettes and a material-sidebar entry.
- Implemented Push, Twirl Left, Twirl Right, Pinch, Expand, Crystals, Edge and Reconstruct, plus Size, Pressure, Distortion, Momentum, Adjust/Amount, Reset, Apply and Cancel. Supported pen barrel rotation controls Twirl.
- Added reusable GPU deformation buffers and WGSL shaders for every mode, a background job pipeline and multicore CPU field composition/material resampling with automatic CPU fallback.
- Added adaptive anisotropic antialiasing for compressed material, preserving perpendicular detail and exact unchanged pixels; corrected linear sampling in headless previews.
- Kept changes local to the active layer and selection; page expansion, exact cancellation, undo/redo, and layered Graphite/PSD round trips remain supported.
- Verified all shader modes on AMD Vulkan and DirectX 12 adapters, with maximum CPU/GPU coordinate disagreement below 0.001 pixel. Added mode, reconstruction, pressure, distortion, worker, momentum, Amount/Reset, toolbar-drag and full-interface regression coverage.

## v0.24.13

- Removed pencil hold-to-straighten entirely. Pausing preserves the freehand curve, including when loading older pencil settings that enabled the former behavior.
- Drawing and transforms now extend past all four page edges. The working canvas grows losslessly around the original page, retaining material, individual layers, selections, editable paths and undo/redo.
- Removed edge input rejection and coordinate clamping. Workspace growth compensates the view position, so the original page and the document point beneath the pointer stay stationary, including rotated and zoomed views.
- PNG, JPEG and BMP exports include the entire expanded working area and outside artwork. Editable Graphite and PSD files also preserve the original page bounds.
- Removed remaining 256-layer copy and four-million-pixel selection-transform restrictions.
- Added headless pointer, interface, layer-history, outside-object transform, native/PSD round-trip and image-export regressions.

## v0.24.12

- Remove the 16-million-pixel canvas quota, 512 MB source/native file quota, and 256-layer quota across opening, creation, save, and native reopening. Remove the associated 32-million-pixel open-tab quota, 768 PSD-record quota, 256 MB decoder quota, and 2 GB native decompression/serialization quota.
- Keep nonzero/overflow validation and file-format field validation. Large Graphite projects are no longer restricted to PSD's representable sizes; PSD export reports its actual format constraints before narrowing integers.
- Verify files larger than 512 MB, 800-layer Graphite/native PSD/external PSD roundtrips, and a 4001 x 4000 canvas through PNG opening and native save/reopen. Preserve background opening, progress display, and Recent files.

## v0.24.11

- Remove the combined-layer pixel cap from PSD records, vector/path import, and drawing conversion. Layer bounds retain arithmetic/overflow validation without a pixel quota.
- Store exactly representable 8-bit import samples in compact temporary buffers, reducing their retained memory by 75%; higher-depth samples and fractional masks retain full precision. No resizing, flattening, hidden-layer removal, or sample quantization is used.
- Verify the reported 3015 x 2743, eight-layer PSD through background opening against an independent decoder: all 66,161,160 layer pixels, names, order, opacity, visibility, and blend modes. Private artwork is not included in the package.

## v0.24.10

- Add File > Recent files for successful opens and saves, with newest files first, duplicate removal and full-path tooltips.
- Add Options > Recent files > Maximum files, from 0 to 10 (default 10). Lowering it trims the list immediately; 0 disables and clears history and stops recording paths.
- Persist the limit and list alongside display preferences, preserving existing acceleration settings and older preference files.
- Verify menu opening, the Options slider, disk persistence, case-insensitive Windows paths, save/export recording, failed operations and disabling/re-enabling.

## v0.24.9

- Fix opening ordinary 4032 × 3024 camera-sized PSDs (12,192,768 pixels), previously rejected by the decimal 12-million-pixel limit.
- Share a 16-million-pixel canvas limit across image import, document creation and native project reopening. Open tabs and imported layer data allow 32 million pixels in total.
- Keep the original dimensions and layer structure; no resizing or flattening is used to fit the new limit.
- Add boundary checks and an opt-in full-size PSD regression that compares every rendered pixel against an independent RGB reference and verifies native/PSD save-reopen behavior. Private artwork is not bundled.

## v0.24.8

- Open now accepts Graphite, external PSD, PNG, JPEG and BMP files. External images open in new tabs with fit-to-screen and use Save As to protect the original.
- PSD import preserves image layers, supported blending, visibility, opacity, names and offsets; raw/RLE/ZIP/prediction work at 8/16/32-bit depth. Pass-through folder children stay separate, with bitmap masks/clipping applied per layer.
- Photoshop solid-color Bézier shape layers and centered solid outlines remain editable vectors, including handles, holes and path operations. Saved paths open as hidden vector layers. Transform, mirror, rotate, undo and save/reopen retain their geometry.
- Unsupported Photoshop-only features report an explanation; opening a layered PSD never falls back to its flattened preview.
- Added independent PSD-library fixtures covering the depth/compression matrix, grouped masks, curves, fills, holes, outlines, saved paths and unsupported styles, plus PNG/JPEG/BMP variants and application import/undo tests.
- Retains the persistent Shift size slider from v0.24.7.

## v0.24.7

- Simplified the size popup to one row with the slider and numeric size. Removed its tool title and close button.
- Pressing Shift (or the configured Ctrl/Alt modifier) opens and focuses the slider. Releasing the key keeps the popup open. Clicking outside dismisses it, and the dismissal contact is consumed through release so it cannot leave an accidental mark. Escape also dismisses the popup. The fullscreen Size button uses the same behavior.
- All 80 application tests pass. Regression coverage includes physical left/right modifier events, a press and release delivered in one frame, adjusting without a held modifier, outside mouse/pen dismissal, ordinary shortcut chords, fullscreen toolbar interactions and widget focus. The compact popup was rendered and visually checked.

## v0.24.6

- Fixed the held size slider failing to appear on physical keyboards. egui 0.35 emits left/right modifier presses as key events; the chord guard incorrectly treated the activating Shift (or configured Ctrl/Alt) as an extra shortcut key and blocked the entire hold. Both sides of the configured modifier now activate the slider, while other shortcut keys still suppress it until release.
- Added a regression test using the physical modifier events emitted by the native input adapter. It failed on ShiftLeft before the fix and passes for left/right Shift, Ctrl and Alt after the fix. All four quick-controls tests pass, covering held/released keys, real chords, slider interaction, widget focus and prevention of accidental drawing. The earlier modifier-only tests did not cover native key events.

## v0.24.5

- Added Options → Keyboard shortcuts… with key recording, conflict detection, Clear, Restore defaults, Save and Cancel. Assignments persist in portable `shortcuts.settings`, separately from drawings. Tools, editing, files, history, drawing zoom, pan and window recovery use the selected bindings; Fit and Fullscreen can also be assigned. File-menu labels and principal tool/transform hints follow custom assignments. The tool guide explicitly lists default keys.
- The default held size modifier is now **Shift**. The shortcut editor can change it to Ctrl, Alt or Disabled. Shift sizing works after actual tool clicks, opens without a second key and dismisses on release. Shapes, view rotation, active strokes and active transform drags retain their existing Shift behavior instead of opening the popup.
- Shortcut recording blocks drawing and command execution, including while fullscreen. Conflicts and file errors keep the editor open without silently applying changes. The package excludes personal shortcut settings.
- Validation: all 202 regression tests pass, including preference round trips, duplicate detection, modifier matching, actual key recording and dispatch after reassignment, and held-Shift sizing after selecting Pencil/Eraser/Smudge/Tissue with pointer clicks. Tests check release cleanup and no accidental marks. Existing shape snapping and transform tests also pass. Shortcut and size popups were rendered for visual inspection.

## v0.24.4

- Mirror horizontal/vertical appear only alongside transform controls: an active transform, Vector selection or Free selection. Ordinary drawing tools no longer show these buttons.
- Holding Ctrl opens the size slider even after a button or slider has retained keyboard focus. Actual text editing and modal dialogs still suppress the shortcut; Ctrl keyboard combinations remain available, and releasing Ctrl dismisses the popup.
- Fullscreen now has a collapse/reopen arrow on its floating toolbar and a separate 48 px X button at the top right to leave fullscreen by mouse or touch. The exit button remains available with the toolbar collapsed; collapsing closes its Size popup.
- Validation: 57 application regression tests passed for mirror visibility and Ctrl focus handling, followed by three quick-controls checks covering collapse/reopen, fullscreen exit while collapsed, slider changes and prevention of accidental marks. The fullscreen layout was also rendered and visually inspected.

## v0.24.3

- The floating Pencil/Eraser/Transform/Tissue/Smudge/Size toolbar is now visible only in fullscreen mode. Leaving fullscreen also closes its Size popup and removes its input targets. The Ctrl size shortcut remains available in either mode.
- Validation: both quick-controls regression tests pass, covering normal/fullscreen visibility, exit cleanup, size changes, tool selection and input ownership without drawing accidental marks.

## v0.24.2

- Imported sampled tips now live as separate, lossless `.pencil` files in `imported_brushes`, beside the executable. `Pencils.gallery` keeps native presets, collection metadata and order without embedded imported masks. Startup and reopening the gallery detect only existing valid pencil files in this folder. Missing files are not reconstructed from in-memory brush caches. Individual malformed files are skipped with an explanation.
- Existing local galleries migrate their embedded tips once, retaining pencil IDs and independent `pencil.settings` controls. Removing a gallery pencil or collection deletes only its managed copies, preserving source ABRs and artwork resources. Raw ABRs are converted using Import ABR; they are not automatically imported just by dropping them into the folder. Packages exclude personal gallery/settings resources and include an empty `imported_brushes` folder.
- Replaced Pencil Material flow with a single **Opacity** percentage control. New default transfer is 15% stronger; light strokes are approximately 15% darker without widening their contact. Zero opacity performs no paper or pigment edit on CPU or GPU. Existing saved flow values remain compatible, and the control persists independently for each pencil.
- Ctrl+Plus, Ctrl+Equals and Ctrl+Minus now zoom only the drawing around the viewport center. The interface no longer changes scale from these shortcuts.
- Holding Ctrl opens a draggable tool-size slider. It adjusts Pencil, Eraser, Smudge, Tissue and other supported drawing sizes in document pixels, or uniformly scales an active transform around its center. Ctrl keyboard chords remain available. A 48 px tool bar in the canvas margins provides Pencil, Eraser, Transform, Tissue, Smudge and a touch-accessible Size button, including fullscreen. UI gestures retain pointer ownership through lift to prevent marks beneath controls.
- Added **Copy layer** beside Merge. The active layer is duplicated immediately above the original, retaining opacity, visibility, blend mode, material and editable paths. Copying is one undoable action; editing the copy does not change the original.
- Added **Mirror horizontal** and **Mirror vertical** to the transform controls. Selections preview a flip before Confirm/Cancel; vector selections retain paths, while pixel-fragment transforms use the existing raster-bake behavior. With no selection, the complete layered drawing and paper are mirrored losslessly. Undo/Redo supports mixed copy, mirror and portrait/landscape operations.
- Paper orientation choices now show only their visual icons, with accessible names and tooltips. Added **Texture opacity** (0–100%) below paper texture selection: it fades visible texture and relief toward the selected paper color without altering the material simulation. It is saved per drawing, preserved in native/PSD projects, used in previews and exports, and inherited when creating a drawing from Paper settings. Older files default to full texture opacity.
- Improved numerical precision for tiny pencil deposits on CPU and GPU, reducing differences in pressure packing near saturation.

- Validation: 198 regression tests pass, plus focused gallery checks after the missing-file selection fix. Coverage includes legacy brush migration, missing/corrupt resources, independent pencil controls, zoom/UI scale, Ctrl-size and fullscreen input, copying/editing layers, vector/pixel mirrors, mixed structural Undo/Redo, paper texture fading and native/PSD round trips. All 26 supplied Chromagraph sampled tips (24,037,129 decoded bytes) round-trip through separate pencil files without shape or collection changes. Measured light-stroke darkening is 13.45–13.75%, with unchanged contact coverage.

- Release GPU validation on Radeon RX 7900 XTX: Vulkan and DirectX 12 passed 313 compute batches combined, including selection clipping and Undo/Redo checks. Rendered CPU/GPU channel differences were at most 1/255. All 198 regression tests also passed after the precision change.

## v0.24.1

- Added multiple layer selection: Ctrl-click toggles individual rows; Shift-click selects a range from the anchor; Ctrl+Shift adds a range. Plain clicks return to a single selected layer.
- Added **Select multiple** for pen/touch use without a keyboard. Layer rows expand to 44 px touch targets, and tapping toggles membership. All selected layers highlight; the active drawing layer is labeled separately.
- With two or more layers selected, **Merge selected (N)** replaces Merge down and merges the entire selection in one operation. One Undo restores every original layer, position, opacity, blend mode, material deposit and editable path; Redo merges again.
- Supports non-adjacent selections. Their merged result occupies the highest selected position; intervening unselected layers remain separate and keep their data/order. Moving selected content above intervening layers can change blending; the panel explains this when gaps exist. Contiguous selections preserve the current appearance.
- Hidden selected layers must be shown before merging. Selecting layers does not mark the drawing modified. Selection follows stable IDs through reordering, remains separate for each drawing during the session, and is not embedded in the saved artwork.
- Generalized merge compositing to combine all selected material in one pass. Shared paper stays separate, matching Multiply/Normal/Screen groups retain their blend behavior, and mixed groups bake their current combined appearance into Normal material. The result remains erasable and supports new editable strokes, native projects and layered PSD.
- All 186 tests pass. UI-event checks cover Ctrl/Shift selection, touch toggling, highlight membership, active-layer identity and merging four editable layers with one Undo. Engine checks cover non-adjacent layer order, untouched intervening material, mixed blend modes, hidden-layer protection, selection IDs through reordering, rotation, and native/PSD 8-bit/16-bit round trips of a multi-layer merge. The touch-selection panel was rendered and inspected without taking over the desktop.

## v0.24.0

- Added **Merge down** to Layers. It combines the active layer with the visible layer immediately below, bakes their opacity and rendered strokes into one erasable/smudgeable graphite layer, and leaves the shared paper and other layers intact. The bottom layer and hidden pairs are protected.
- Merging is undoable without clearing stroke history. Undo restores both layers, their original deposits, opacity, blend modes and editable paths; Redo reapplies the merge. Merge history also survives portrait/landscape rotation.
- Multiply, Normal and Screen pairs retain their respective blend mode and transparency independently of the paper/backdrop. Mixed modes and Darken/Lighten pairs bake their current appearance into a Normal layer, so their original separate blending behavior is no longer retained. The merged layer's opacity becomes 100%, with the former opacity included in its material coverage.
- Increased local paper capacity by **15%** in CPU and GPU deposition. Repeated passes with dark graphite can reach deeper blacks; the chosen pencil color and existing deposited marks are unchanged. Normalized tissue/custom powder transfer to keep light initial passes nearly unchanged.
- Merged material uses the existing native and layered PSD project formats, without a format change. New strokes remain editable above the merged material base; Undo restores the original paths from before merging.
- All 182 regression tests pass. Merge checks cover every pair of blend modes at multiple opacities (visible difference at most 1/255), original layer/path restoration, paper tint, rotation, unrelated layers, native/PSD 8-bit/16-bit reopening and erasing merged material. The first light tissue pass differs by less than 0.1% while saturation reaches the 15% higher capacity.
- Radeon RX 7900 XTX checks passed 220 compute batches each on Vulkan and DirectX 12, including selection clipping on DirectX 12, native/imported pencils, tissue and erasers. CPU/GPU visible differences remain within 1/255 per color channel; undo/redo is exact.

## v0.23.9

- Confirm now clears the pixel selection mask and selected vector highlight, including when no move, resize or rotation was made. Tool switching also clears standalone selection masks without an active transform.
- Cancel restores the original artwork, clears the selection and leaves Free selection / Vector selection for Pencil. Confirm also leaves those selection tools. Enter, Esc and Ctrl+D use the same completion/cancellation behavior.
- Replaced the top transform action row with 48 px green-checkmark and red-X buttons beside the selection. They follow its transformed screen bounds, appear below it where space permits and stay inside the visible canvas when zoomed, panned or rotated. They remain available with the panels hidden or in fullscreen, and for standalone selections.
- Button pointer input is kept out of the drawing engine, including native pen presses that move away before release.
- Moved **Keep aspect ratio** into the left tool options panel, alongside the transform controls. It is also available before selecting with the free or vector selection tools.
- All 176 regression tests pass. Real UI-event tests cover checkmark/X clicks with moved and untouched vector/pixel selections, standalone masks, tool switching, fullscreen, hidden panels and the relocated aspect checkbox. Position tests cover rotated and off-screen selections; cancellation and undo restore material and editable paths exactly. UI previews were rendered and inspected without taking over the desktop.

## v0.23.8

- Added a persistent **Snap shape** checkbox to the Line tool. Preview and placed lines snap to 15° increments without Shift or a gesture; Shift remains available for temporary snapping.
- Added **Keep aspect ratio** to active transform controls for proportional resizing of vector strokes and pixel selections without Shift. It also works on rotated selections and paper views; the preference is saved in projects.
- Tissue now has a **Random graphite** density control (0–100%) for soft patches of lighter and deeper graphite in the chosen color. The pattern is deterministic for editable-stroke replay, with matching CPU/GPU calculations. Its default of 0% preserves existing strokes.
- Set the Tissue size control maximum to 200 px. Older saved sizes remain intact until explicitly edited.
- Increased the Eraser diameter control to 200 px for both Vinyl and Kneaded erasers. The maximum is based on document pixels at any supported DPI, replacing the old 12 mm limit (about 57 px at 120 dpi).
- Retained the existing physical-size storage, pressure response, lift strength, CPU/GPU rendering and saved strokes. Drawing quality and document resolution are unchanged.
- Checked the size-control behavior at 36, 120, 300 and 600 dpi. The rendering paths already support erasers of 200 px and larger.
- All 174 regression tests pass, including modifier-free snapping, proportional vector/pixel resizing on rotated paper, native/PSD setting round trips, tissue density/color checks and exact undo/redo. Older settings default to the previous behavior.
- The release-engine tissue benchmark at 200 px averaged 0.59 ms per segment with uniform loading and 0.85 ms with Random graphite at 100%, over 60 segments. These are material-engine times, excluding display rendering and input latency.
- Radeon RX 7900 XTX Vulkan checks confirm randomized and uniform tissue GPU results stay within 1/255 per visible color channel of CPU rendering, with exact undo/redo and selection clipping. Normal tissue rendering retains the faster CPU path.

## v0.23.7

- Selecting a tool from the toolbar or a keyboard shortcut cancels the pending transform, restores the artwork before that transform, and clears its vector or pixel selection.
- Applies to move, resize and rotate previews. Cancellation preserves editable paths and creates no extra undo entry.
- Opening Pencil gallery no longer confirms a transform; choosing a pencil cancels it through the shared tool-selection behavior.
- Cancel (the cross) and Esc also clear the vector or pixel selection when restoring the artwork. Esc does this while exiting fullscreen too. The checkmark and Enter still explicitly confirm transforms.
- Windows file and folder pickers are now owned by the live Graphite Studio window, so Windows positions and stacks them with the application on its current monitor. This covers project Open/Save As, ABR import, paper-image import and folder collections. The owner refreshes if the native window changes; no monitor coordinates are cached.
- Dialog ownership is connected to the native Windows dialog API; physical placement on the EasyCanvas display still needs a user check. No desktop input was taken over during verification.
- Updated transform guidance and the GitHub README. Release notes remain consolidated in this file.
- All 171 regression tests pass. Real UI-event tests cover every toolbar tool with vector and pixel transforms, tool shortcuts, gallery activation, exact restoration and undo; explicit confirmation and cancellation checks also pass.

## v0.23.6

### Release summary

- Packed conservative contact tiles and selected pixels for GPU strokes, reducing broad-tip transfers without reducing simulation quality.
- Reused GPU scratch buffers and warmed typical large-brush storage at startup; cached the ordinary-pencil mask.
- Retuned the compute crossover for upright and imported tips after culling.
- Uploaded the canvas after processing pen packets, removing one frame of avoidable delay, with immediate final-stroke flushing.
- Packaged the production release profile with link-time optimization and measured stroke plus canvas-update completion.
- Passed 169 regression tests and hardware material/image/undo checks on Vulkan and DirectX 12.

### Detailed release notes

Large pencils respond substantially faster. The drawing now processes queued pen input before updating the canvas texture, removing an unnecessary frame of display delay. Pen lift still flushes the final stroke immediately.

#### What changed

- Transfer and compute only a conservative union of the brush's contacting tiles, including its irregular edges and antialiasing fringe. The broad sideways-pencil workload transfers roughly 70,000–131,000 pixels per segment instead of 521,000–681,000. Selected-out pixels are excluded before transfer.
- Reuse GPU material, coordinate, dab, wear and readback buffers across strokes. Initialize a typical broad-tip working set at startup to reduce first-contact allocation stalls. Cache the neutral mask instead of recreating it for each ordinary-pencil segment.
- Correct the CPU/GPU crossover after contact culling: large upright standard and imported pencils now use compute at workloads where the old threshold kept them on the slower CPU path.
- Ship a production release build with link-time optimization. Earlier packages used the optimized development profile.

No brush spacing, paper resolution, texture samples, imported silhouettes, color settings or antialiasing were reduced. Material is still synchronized to the document for exact undo/redo and saving. CPU fallbacks remain for unsupported devices, failed compute and work exceeding the bounded scratch budget. Tissue, smudge and complete erasing retain their faster CPU paths.

#### Measurements

Radeon RX 7900 XTX / Vulkan, production release executable, 20 curved 8 px segments with changing pressure and azimuth. These are measured processing times, not a claim of zero input-to-display latency. Machine load affects results.

| Workload | Stroke processing, mean | Stroke + shading + upload + GPU mip completion, median / p95 |
| --- | ---: | ---: |
| Standard pencil, 200 px, 8° tilt | 1.53 ms | 2.43 / 3.08 ms |
| User-supplied Chromagraph tip, 200 px, 65° tilt | 3.92 ms | 5.82 / 7.40 ms |
| Standard pencil, 200 px, 65° tilt, A4 at 120 dpi | 9.64 ms | 13.57 / 17.03 ms |

The last workload previously took about 39 ms for stroke processing alone on the v0.23.5 implementation in this session: approximately a fourfold improvement. The full-canvas-update timing uses the application's raster renderer and GPU display pyramid and waits for GPU completion. It excludes window presentation, screen refresh, pen-driver latency and EasyCanvas transport. It is a headless measurement; no desktop input was taken over. Literal instant rendering and matching Photoshop in every workload are not guaranteed.

#### Validation

- All 169 regression tests passed, including a new real UI-frame test for same-frame mark upload and immediate pen-lift flush.
- Real GPU checks covered standard/imported pencils, partial/full/kneaded erasing, tissue, selection clipping, and exact undo/redo on Vulkan and DirectX 12.
- In tested workloads, GPU output differs from the CPU reference by at most one 8-bit channel level. Physical floating-point material/wear values are not bit-identical across backends, as in the previous compute engine.
- Native material, paper, pressure, tilt, barrel rotation, sampled brush shape, editable paths and save formats remain in use. Proprietary brush files are not bundled.
- Modern Intel devices use the same supported compute path, but Intel hardware was not available for testing. Legacy Intel HD compatibility mode remains CPU/OpenGL.

Save your work, extract the package and run **Graphite Studio.exe**. Use the **AMD Radeon 7900 XTX** acceleration mode in Options for your Radeon. Keep **pencil.settings**, **Pencils.gallery** and **Graphite-Studio.settings.json** beside the executable when updating, if present. Source and checksums are included; the executable retains its embedded icons and has no console window.

## v0.23.5

### Release summary

- Added bounded, batched GPU compute for large standard/imported pencils and partial erasing, with CPU fallback; retained faster CPU paths for tissue, smudge, small strokes and full lift.
- Cached CPU contact, material and eraser calculations while retaining exact reference material results; reduced redundant smudge work.
- Changed line angle snapping to 15-degree increments.
- Removed the toolbar brand label, PSD bit-depth selector and Export button.
- Unified editable project and flattened image output in Save project as, including PSD bit-depth controls.

### Detailed release notes

#### Drawing performance

Large standard and imported pencils, including their textured geometric outlines, and partial-strength erasing can now use GPU compute. Work is batched across stroke samples; pixels are processed in parallel while each pixel retains stroke order. The existing paper, pigment color, pressure, tilt, sampled shape and wear models remain in use. Undo still captures the material and paper changes.

The app uses the graphics device selected by General or AMD Radeon 7900 XTX mode. The portable compute shader uses baseline wgpu/Vulkan/DirectX 12 features, compatible with supported AMD and Intel devices. Older Intel HD 2000/3000 compatibility mode continues to use the CPU. Intel hardware was not available for direct testing. See [wgpu backend support](https://github.com/gfx-rs/wgpu) and [Vulkan compute](https://docs.vulkan.org/tutorial/latest/11_Compute_Shader.html).

Small workloads, tissue, smudge and complete erasing stay on the CPU. Tissue and full erasing were faster there than with GPU transfers. Work beyond the bounded GPU buffer budget also falls back to the CPU. Failed compute readbacks are discarded before changing the drawing, and disable stroke compute for that session. This is parallel processing within the selected GPU, not combined processing across multiple GPUs.

CPU drawing also caches pressure, transfer and orientation calculations across each contact. Standard/imported pencil material, wear and final pixels match the previous CPU implementation exactly in the reference workloads. Eraser and smudge calculations have additional shared-value caching; empty smudgers skip clean paper.

##### Measured checks

Optimized development build, engine time including GPU transfers (not full frame time); timings vary with machine load:

- 200 px imported synthetic pencil: original CPU about 26–37 ms per segment, optimized CPU about 13–19 ms; GPU application path about 5 ms in the tilted workload.
- User-supplied Chromagraph tip, 200 px: optimized CPU about 29 ms versus GPU about 7 ms per segment.
- Standard pencil at 100 px, tilted: about 56–59 ms CPU versus 17–18 ms GPU.
- Partial eraser at 200 px: about 10–13 ms optimized CPU versus 4–5 ms GPU.
- CPU-only 400 px partial-erasing workload improved from 25.7 to 21.7 ms per segment with identical material fingerprints. Smudge gains were modest, about 1–5% in these workloads.

Hardware checks ran on the Radeon RX 7900 XTX and integrated Radeon adapters, through Vulkan and DirectX 12. GPU reference images differed by at most one 8-bit channel level in the checked workloads; GPU floating-point material/wear results are not bit-identical to CPU results. Tests also cover selection clipping and exact undo/redo. Proprietary brush files are not included.

The regression suite passes all 168 tests, including save format round-trips, PSD bit depth, modal interaction, shape snapping, material behavior and existing drawing workflows.

#### Controls

- Hold Shift with the Line tool to choose the nearest 15-degree angle, including 0, 15, 30, 45, 60 and 90 degrees. Preview and placed geometry use the same snapping. Triangle, circle and square proportion constraints are unchanged.
- Removed the redundant Graphite Studio toolbar label.
- Combined image export with File > Save project as… (Ctrl+Shift+S). Choose PSD, Graphite, PNG, JPEG or BMP, then a location. PSD bit depth is available in this dialog only; the toolbar no longer contains PSD bit depth or Export.
- PSD and Graphite keep editable project state. PNG/JPEG/BMP save flattened copies and do not mark an unsaved editable project as saved.

Save open work, extract the package and run **Graphite Studio.exe**. Keep **pencil.settings** and **Pencils.gallery** beside the executable when updating. Rust source is included. The application has a Windows GUI executable and embedded icons, with no console window.

## v0.23.4

### Release summary

- Raised the core diameter control to 200 px and removed the smaller internal size ceiling in drawing, sharpening and project validation.
- Used imported sample outlines, proportions and empty areas as graphite contact shapes, with subpixel edges and native material rendering.
- Preserved the previous contact model for old editable strokes; gallery pencils opt into shaped contact when selected.

### Detailed release notes

**Core diameter now reaches 200 px.** The pixel control converts to the drawing's physical size without the old 5 mm rendering limit. Sharpening, saved pencil preferences and project reopening retain the larger diameter. Pressure and tip sharpness still determine how much of that core touches the paper.

**Imported pencils now use their sampled tip shapes.** Their outline, aspect ratio and empty areas define the contacting face instead of being clipped inside the standard pencil outline. Pressure scales contact, tilt stretches the face, and orientation turns it. Subpixel sampling smooths the edges. Continuous stroke sampling and Graphite Studio's paper contact, material transfer, color, grain and wear produce the mark; Photoshop's proprietary dynamics are not imported.

Selecting an existing imported pencil in the gallery uses the new shaped contact automatically. Previously saved editable strokes retain their original contact model for faithful replay. No reimport is required.

Validation covers square corners, transparent holes, rectangular proportions, rotation, edge antialiasing, conservative rendering bounds, native grain, 200 px at 36/72/120/300/600 DPI, sharpening and project round-trip. A rendered hollow-square sample was visually checked.

Save open work, extract the package and run **Graphite Studio.exe**. Keep **pencil.settings** and **Pencils.gallery** beside the executable when updating. Rust source is included.

## v0.23.3

### Release summary

- Autosaved independent pencil controls in pencil.settings, including Width response and Line smoothing.
- Restored individual controls on pencil selection and the last available pencil on startup, including Standard graphite.
- Added persistent pencil identities so duplicate names, renaming, moving and deleting other presets do not mix preferences.
- Kept preferences separate from texture data and verified control edits through real pointer events and file round-trips.

### Detailed release notes

Pencil controls are now remembered independently in **pencil.settings**, beside the executable. Changes save automatically after a short idle interval and when the application closes. Switching pencils restores their individual controls; reopening the app restores the last selected available pencil.

Saved controls include **Width response**, **Line smoothing**, grade, core diameter, geometry scale, sharpness, color, shade variation, material flow, fallback pressure, pressure feel, hold-to-straighten, manual tilt/azimuth, auto-azimuth and tip rotation. Standard graphite has its own entry too. Tip wear remains drawing state.

Each gallery pencil has a stable identity. Pencils with identical names remain independent, and renaming or moving a pencil between folders preserves its preferences. Existing galleries gain these identities automatically. New saved pencils become the active pencil. Existing drawings retain their stored stroke settings.

The small settings file stores controls only. Brush textures and collections remain in **Pencils.gallery**, so changing sliders does not rewrite the texture library. Keep **both files** beside the executable when updating or moving the portable application. Imported texture files are not modified.

Validation covers real pointer edits to Width response and Line smoothing, switching between two identically named pencils and Standard graphite, rename/move, settings-file save/reopen, legacy gallery identities, and all other stored controls. File writes use a temporary file and replacement after successful serialization.

Save open work, extract the package and run **Graphite Studio.exe**. Rust source is included.

## v0.23.2

### Release summary

- Made gallery tiles distribute across the available viewport width with proportionate previews.
- Kept the gallery scrollbar at the right edge and filled the scrolling area above the editing controls.
- Preserved current zoom when resetting portrait/landscape orientation.
- Replaced Undo/Redo labels with touch-sized arrow icons and shortcut tooltips.

### Detailed release notes

The Pencil gallery now fills its available width. Pencil tiles share the space evenly and reflow when the window is resized; their previews keep their original proportions. The scrollbar sits at the gallery's right edge, and the scrolling area fills the space above the editing controls, including when only a few pencils are shown.

Portrait and Landscape reset orientation while preserving the current zoom, both on the main toolbar and in Paper settings. Undo and Redo now use icon-only arrow buttons with shortcut tooltips and 44-point touch targets.

Verified with the application interaction tests, including gallery selection, orientation/zoom preservation and toolbar layout, plus rendered UI previews.

Save open work, extract the package and run **Graphite Studio.exe**. Keep **Pencils.gallery** beside the executable when updating to preserve your collections. Rust source is included.

## v0.23.1

### Release summary

- Added labeled vector icons and touch targets for Paper settings, Pencil gallery, Fit to screen and Fullscreen.
- Restored portrait/landscape shortcuts to the main toolbar, retaining orientation reset and artwork rotation.
- Added multiple persistent directory collections with Add folder and Delete folder; source files are untouched.
- Fixed modern ABR resource padding, verified with the actual 26-tip Chromagraph file and gallery round-trip.
- Imports now reveal their collection, clear search filters, display errors in the gallery and commit all tips together.

### Detailed release notes

#### Toolbar

Paper settings, Pencil gallery, Fit to screen and Fullscreen now have scalable vector icons alongside their labels, with 44-point touch targets. The toolbar wraps on narrower windows.

Portrait and Landscape are back on the main toolbar and remain in Paper settings. Clicking either resets view rotation and fits the sheet; changing its orientation also rotates the artwork, layers and editable paths with Undo support.

#### Folder collections and ABR import

- **Add folder…** opens a folder picker and imports the ABR files directly inside that directory into a named collection. Add multiple folders, including empty folders for organizing saved pencils. Identical folder names are distinguished automatically.
- **Delete folder** removes the selected collection and its gallery presets. Source files on disk and existing drawing strokes remain intact.
- Collections and embedded textures persist in **Pencils.gallery** beside the executable. Keep that file when updating or moving the portable app.
- Fixed padding handling between modern ABR sections. The supplied TGTS Chromagraph file now imports its **26 sampled textures** successfully. These use Graphite's pencil engine; Photoshop computed brushes and proprietary dynamics are not reproduced. Imported textures currently receive numbered names under the ABR filename.
- Successful imports select their collection and clear the search. Import errors appear inside the gallery. A failed multi-tip import leaves the existing gallery intact.

The gallery supports 256 pencils, 256 folder collections and 64 MB of decoded tip masks. Folder import reads immediate ABR files, with a 128 MB combined source-file limit; it does not watch or alter the directory. No third-party brush files are bundled.

#### Validation

159 automated tests passed, including toolbar interaction at widths from 900 to 1440 points, orientation and undo, multiple collection persistence/deletion, ABR resource padding, and atomic import failure. The actual Chromagraph file additionally passed a complete 26-texture gallery save/reopen comparison. Toolbar previews were rendered and visually checked without taking over the desktop.

Save open work, extract the package and run **Graphite Studio.exe**. Rust source is included.

## v0.23.0

### Release summary

- Merged imported tips into Pencil as contact relief for the physical graphite solver; kept legacy stroke rendering intact.
- Added a persistent pencil gallery with large engine-rendered preview tiles, search, folders, ABR import and named user presets.
- Moved paper configuration and information into Paper settings, with custom new-document dimensions in mm or px.
- Portrait/landscape buttons always reset view rotation and fit the paper, including repeated selection of the current orientation.
- Verified custom-tip continuity and dynamics, gallery persistence, custom paper creation and existing project compatibility.

### Detailed release notes

#### One Pencil tool

Imported ABR samples are now available inside Pencil. They describe the contacting graphite surface instead of being pasted repeatedly as grayscale stamps. They share the pencil's continuous stroke sampling, pressure/width response, tilt, barrel orientation, paper grain, shade variation, material transfer and wear. Core diameter, sharpness and flow use the usual pencil controls. Low-relief areas leave a lighter graphite film.

Standard graphite remains available. Existing saved strokes retain their previous rendering, including old custom-brush paths and pencil paths with dormant brush masks. New texture choices are stored in editable paths and projects. Imported samples are interpreted as contact relief; Photoshop stamp silhouettes, proprietary dynamics and computed brushes are not reproduced.

#### Pencil gallery

Open **Pencil gallery…** from the top bar or Pencil controls. Each 200 × 112 point tile shows a stroke rendered by Graphite's own engine. Previews are cached and generated in small batches. Primary controls use large touch targets.

- Import ABR tips into a folder named after the file.
- Search by pencil name or select a folder.
- Tap a tile to select its pencil; tap Done to draw.
- Enter a name and folder, then Save current pencil to keep your own settings.
- Select a saved pencil to rename it, move it to another folder or remove it from the gallery. Removing a preset does not remove strokes from drawings.

Pencils and embedded tips are saved in **Pencils.gallery** beside the executable. Keep this file when moving or updating the portable application. The gallery starts with Standard graphite; no third-party brushes are bundled. It supports up to 256 saved pencils and 64 MB of decoded tip masks. Device/backend preferences are kept when changing pencils.

#### Paper settings

**Paper settings…** now contains current paper information, orientation, paper color, built-in/custom textures and surface information. A5, A4, Letter and Custom sizes appear under New drawing. Custom width and height can be entered in millimeters or pixels, with a DPI control and a live pixel-size readout. Create drawing opens a new tab using those dimensions and the current paper appearance; it does not resize existing artwork. The document pixel limit remains enforced.

Portrait and Landscape reset view rotation and fit the paper even if the sheet is already in that orientation. Changing sheet orientation also rotates the artwork, layers and paths as before, with Undo support.

#### Validation

All 155 automated tests passed, including continuous custom-tip strokes, pressure/tilt/color response, gallery persistence, embedded project tips, large gallery tile selection, custom paper creation and orientation buttons. The standard-pencil benchmark retained identical material, wear and rendered-pixel fingerprints in all six reference cases. Gallery/settings gestures do not draw or navigate the paper behind the windows.

Save open work, extract the package and run **Graphite Studio.exe**. Rust source is included.

## v0.22.9

### Release summary

- Tool icons and shortcuts commit pending transforms instead of reverting their placement; committed changes remain undoable.
- Added large Confirm/checkmark and Cancel/cross controls above the canvas, available with panels hidden and in fullscreen.
- Verified tool switching, transformed vector geometry and pixel-selection confirm/cancel through real egui pointer/key events.

### Detailed release notes

Switching tools now keeps a pending transform's position, size and rotation instead of canceling it. This works with every tool icon and with P/B, E and V shortcuts; entering paper rotation with R also keeps the transform. The result remains undoable with Ctrl+Z.

A green checkmark **Confirm** and red **Cancel** cross appear above the canvas during a transform. Both have large pen/touch targets and vector-drawn icons. They remain available with Material/Layers hidden and in fullscreen.

- Confirm keeps the current placement; Enter also confirms.
- Cancel restores the original artwork; Esc also cancels (in fullscreen, the first Esc exits fullscreen).
- Switching tools confirms automatically.

Both editable vector paths and free pixel selections use these controls. Existing behavior for pixel transforms is retained: confirming a changed pixel selection flattens editable paths on that layer, and Undo restores them. Vector transforms remain editable.

All 151 automated tests passed. Validation exercises actual toolbar clicks, keyboard shortcuts, move/resize/rotation persistence, vector and pixel confirm/cancel, hidden panels, fullscreen and Undo. Save open drawings, extract the package and run **Graphite Studio.exe**. The Rust source is included.

## v0.22.8

### Release summary

- Prepared shared pencil contact geometry once per dab and conservatively rejected empty blocks before detailed graphite sampling, accelerating broad tilted strokes without changing their material or pixels.
- Restricted pencil/eraser redraw bounds to pixels actually changed by the dab.
- Added a repeatable tilted-pencil benchmark with before/after material, wear and pixel fingerprints plus undo/redo checks; tested worn rotated tips, AA fringes and dirty-region correctness.

### Detailed release notes

Sideways pencil shading now spends less time checking paper that cannot touch the graphite. The solver prepares shared tip geometry once per dab and skips empty 8x8 blocks using conservative bounds that include irregular edges and anti-aliasing. Pixel processing order, texture, pressure response, tilt, color and tip wear are preserved. Drawing refreshes use the actual changed area instead of the larger search square.

#### Measured pencil performance

Production Rust stroke engine on this computer, optimized development build. Each case draws 24 approximately eight-pixel segments with changing pressure and azimuth; numbers are median milliseconds per segment from three runs. Document creation, export and screen presentation are outside the timed section.

| Core / resolution | Tilt | v0.22.7 | v0.22.8 |
| --- | ---: | ---: | ---: |
| 1.5 mm / 120 DPI | 8° | 0.524 ms | 0.272 ms |
| 1.5 mm / 120 DPI | 45° | 1.401 ms | 0.594 ms |
| 1.5 mm / 120 DPI | 78° | 7.252 ms | 1.598 ms |
| 5 mm / 240 DPI | 8° | 2.131 ms | 2.133 ms |
| 5 mm / 240 DPI | 45° | 12.021 ms | 5.957 ms |
| 5 mm / 240 DPI | 78° | 106.623 ms | 20.105 ms |

Sideways contact processing was about 4.5–5.3 times faster in these two cases. These are engine timings, not tablet-to-screen latency measurements; very large high-resolution marks still cost more than fine lines.

All 148 automated tests passed. Before/after fingerprints match for all rendered pixels, twelve material/surface arrays and serialized tip state in every benchmark case. Undo and redo restore identical material and pixels. Additional tests cover rotated worn tips, subpixel fringes and complete redraw coverage. No approximation, lower resolution or reduced stroke sampling was introduced.

Save open drawings, extract the package and run **Graphite Studio.exe**. The Rust source and repeatable pencil benchmark are included.

## v0.22.7

### Release summary

- Confined the horizontal resize cursor to the visible panel edge bounds and active panel drags; hovering tool icons and surrounding space no longer shows it.
- Buffered JSON writes before compression and reads after decompression, avoiding per-token codec calls for large material arrays.
- Applied the same bounded-memory optimization to native projects and editable PSD payloads while retaining file format, validation, size limits and atomic save replacement.
- Added bidirectional compatibility checks against the previous codec and a repeatable save/open benchmark with exact round-trip comparisons.

### Detailed release notes

The double-arrow resize cursor now appears only inside a visible panel edge or while dragging that edge. Tool icons and nearby empty space keep their normal cursor. Both panel handles still support hiding, showing and resizing.

Project saving and opening now batch the small in-memory JSON operations around compression/decompression using a fixed 256 KB buffer. Previously each small number or punctuation token reached the compression layer separately. The file format, compression level, floating-point data and existing file compatibility are unchanged.

The change applies both to native .graphite projects and to Graphite's editable state in PSD files. Photoshop layers and vector information are preserved. Saves still complete a temporary file before replacing the destination; size limits and validation remain enabled.

#### Measured results

Test drawing: A4 at 120 DPI, two layers and 20 editable pencil strokes, using the same optimized development build profile on this computer.

| Operation | Before | With buffering |
| --- | ---: | ---: |
| Save .graphite | 18.57 s | 0.87 s |
| Open .graphite | 5.28 s | 1.68 s |
| Save editable 16-bit PSD | 19.23 s | 2.81 s |
| Open editable PSD | 5.55 s | 1.77 s |

These are individual benchmark runs rather than end-to-end UI latency guarantees. File size, drawing complexity, storage cache and other computer activity affect timings. The native file was about 30 MB and PSD about 52 MB. Rendered pixels, material arrays and retained paths matched after reopening.

All 146 automated tests passed. Validation covers tool-icon hover and both panel handles, the previous unbuffered reader/writer, custom paper, embedded brushes, selection data, both PSD bit depths, corrupted/truncated input and preservation of an existing file when saving fails. The final benchmark also reopened the baseline files and compared their rendered pixels and material arrays exactly. Large projects still perform save/open work synchronously; this update reduces processing time rather than adding background jobs.

Extract and run **Graphite Studio.exe** after saving open work. The Rust source is included.

## v0.22.6

### Release summary

- Limited provisional rollback redraws to the affected region; replaced hash-based pixel snapshot storage with a contiguous list and bitset duplicate checks.
- Cached sparse layer tile spans within each render and skipped empty/invisible layers without changing pixel results.
- Stabilized panel handle identities and prevented stale mouse hover from overriding the cursor or claiming panel resize interaction during canvas contact.
- Verified exact before/after render pixels and measured lower tissue and sparse-layer rendering costs.

### Detailed release notes

#### Cursor fix

Panel handles no longer claim the resize cursor or resize interaction while a canvas stroke is active. This covers the case where native pen coordinates move while Windows/egui retains an older mouse hover over a panel edge. Both handles now have stable interaction identities across layout changes. Normal panel resizing and hiding remain available when drawing is finished.

#### Performance

- Provisional-stroke rollback now refreshes only the affected pixels instead of the entire page. This reduces unnecessary redraw work while adjusting hold-to-straighten lines and cancelling provisional strokes.
- Undo capture stores original pixel states consecutively with a duplicate-check bitset. Large tools avoid per-pixel hash-table insertion overhead.
- Layer rendering skips invisible/empty layers and resolves sparse tile lookups once per horizontal span instead of once per pixel. Layer order, blending and opacity use the same calculations as before. These span references exist only during each render, so edits cannot leave a stale cache.

##### Measured on this computer

| Workload | Before | After |
| --- | ---: | ---: |
| 400 px tissue, mean per segment | 3.57 ms | 2.58 ms |
| 800 px tissue, mean per segment | 13.74 ms | 10.13 ms |
| Six-layer full redraw, median | 104.99 ms | 52.08 ms |
| Single-layer full redraw, median | 53.39 ms | 50.95 ms |
| 256 × 128 drawing patch, median | 1.32 ms | 1.14 ms |

Tissue timings cover 60 segments on A4 at 120 DPI. The rendering benchmark uses A5 at 300 DPI (1748 × 2480); the six-layer case has one half-covered layer and five empty layers, measuring sparse-layer overhead. Full-page timings are medians of five runs; patch timings use 31 runs. Builds use the same optimized development profile. These are isolated CPU benchmarks, not end-to-end Apple Pencil latency or a guarantee for other hardware/documents.

Rendered tissue and six-layer benchmark pixels were identical before and after. Large tissue undo/redo was exact. Automated checks also cover all blend modes, opacity and visibility changes, active-layer switches, partial updates across tile boundaries, limited rollback bounds, and stale panel-hover cursor behavior.

Extract and run **Graphite Studio.exe** after saving open work. The Rust source is included. Physical EasyCanvas/Apple Pencil cursor behavior has not been verified on the user's device during this update.

## v0.22.5

### Release summary

- Added a persistent Layers edge handle for click-to-hide/show and drag-to-resize, retaining the chosen width within the session.
- Canvas allocation accounts for the handle and panel width, with bounds to keep layer controls usable.

### Detailed release notes

The Layers panel now has a clickable, draggable handle on its left edge.

- Click the arrow to hide Layers and free more drawing space.
- Click the remaining edge to show Layers again.
- Drag left to widen the panel, or right to narrow it.
- The chosen width is retained while hiding/showing the panel during the session. The toolbar Layers toggle still works.

The panel width is bounded to keep its controls usable and leave room for the drawing. These layout changes do not alter artwork, layer visibility or document resolution.

Extract and run **Graphite Studio.exe** after saving open work. The Rust source is included.

Validation uses pointer press/drag/release events to widen, narrow, hide and restore Layers, verifies the handle remains reachable and confirms that artwork and layer count are unchanged.

## v0.22.4

### Release summary

- Added a general Width response control for finer light-pressure strokes with stronger thin-to-thick variation, paired with the existing Tip sharpness control.
- Updated cursor, vector picking, sampling and geometric outline sizing to account for width dynamics.
- New sessions start with a finer pencil; older serialized paths retain their previous geometry.

### Detailed release notes

Added one general **Width response** slider to Pencil controls. There are no new presets or modes.

- Raise Width response for thinner light strokes and a wider thin-to-thick range as pressure increases.
- Set it to 0% for the previous width response.
- Use the existing **Tip sharpness** control to adjust the tip independently.
- New application sessions start at 60% Width response and 65% Tip sharpness for a finer default feel.

Saved drawings and vector paths from older versions retain their previous geometry. When reopening an older drawing, increase Width response to apply the new behavior to subsequent strokes. New paths save their width response for consistent reopening and editing.

The cursor and vector picking follow the updated contact width. Sampling becomes denser for fine strokes to keep their edges smooth. Shape outlines still use their requested width and the pencil texture. With a mouse, width follows the fixed fallback pressure; live thin-to-thick variation requires pressure input.

Extract and run **Graphite Studio.exe** after saving any open work. The Rust source is included.

Validation covers rendered light/heavy strokes, increased width range, legacy settings compatibility, saved settings, existing shape/vector tests and the application test suite. Physical Apple Pencil feel still needs to be judged on the user's device.

## v0.22.3

### Release summary

- Added Shift snapping to geometric shape preview and placement: eight line directions and equilateral triangles, with existing circle/square constraints retained.
- Snapping uses document coordinates and responds to Shift changes without pen movement; placed outlines remain editable and undoable.

### Detailed release notes

Hold **Shift** while dragging with the geometric Shapes tool:

- **Line:** snaps to horizontal, vertical or 45-degree diagonals.
- **Triangle:** constrains the outline to an equilateral triangle.
- **Circle / Square:** continue to keep equal width and height automatically.

Press or release Shift during the drag to change the preview. Keep Shift held when lifting the pen or releasing the mouse to place the snapped shape. Snapping follows the paper's axes even when the view is rotated or zoomed.

Preview and placement share the same geometry calculation. Pencil texture, color, shape opacity, editable paths and undo/redo are preserved.

Extract and run **Graphite Studio.exe**. Save open drawings before replacing an earlier executable. The Rust source is included.

Validation covers all eight line directions, triangle side lengths, all four shape tools, changing Shift without pointer movement, placement on rotated/zoomed paper and pixel-exact undo/redo.

## v0.22.2

### Release summary

- Pencil hold-to-straighten: pause for 0.65 seconds, move to adjust the endpoint, then lift to commit one editable graphite path. Includes a Pencil setting to disable the gesture.
- Fixed disappearing geometric shapes with zero-pressure pen-down packets and requested a fresh display after placement.
- B now selects Pencil alongside P, for EasyCanvas brush-key compatibility.

### Detailed release notes

#### Pencil: hold to straighten

Draw with Pencil, then keep the tip still for about 0.65 seconds without lifting. The stroke becomes a straight graphite line. Keep the pencil down and move to adjust its endpoint; lift to place it. No keyboard is needed. Turn off **Hold to straighten** in the Pencil controls to keep all strokes freehand.

The straight line retains the stroke's graphite color, texture, pressure and tilt variation. It remains one editable vector path and one undo step. Short taps do not trigger straightening.

#### Shape placement

Lines, circles, triangles and squares now retain contact pressure during the drag instead of using only the first pen-down reading. An initial zero-pressure packet no longer makes the finished shape invisible. Release requests a display refresh even if the pointer stops moving. Shape color, texture and opacity remain controlled by the current pencil and Shapes settings.

#### EasyCanvas shortcut

**B** selects Pencil, as does **P**. This supports an EasyCanvas brush shortcut configured to send B. Modified shortcuts and text-field typing are not reassigned. EasyCanvas supports customizable keyboard shortcuts ([manufacturer](https://www.easynlight.com/en/easycanvas)); the user's iPad shortcut configuration has not been changed or tested on hardware.

Extract and run **Graphite Studio.exe**. Save open drawings before replacing an earlier version. The package includes the Rust source.

Validation covers all four geometric shapes with zero-pressure pen-down/up, display refresh, undo/redo, timed holds with no new pen packets, endpoint adjustment, exact retained-path replay, the hold toggle, and keyboard shortcuts. Apple Pencil/EasyCanvas hardware testing remains for the user.

## v0.22.1

### Release summary

- Fixed the portrait/landscape toolbar buttons to rotate existing paper and artwork, with current-orientation highlighting, while retaining the selected orientation for new drawings.
- Fixed stale redraw bounds when undo/redo changes orientation repeatedly before the next render.
- Added a regression test that clicks both actual toolbar buttons with existing vector artwork and verifies exact artwork restoration and undo/redo.

### Detailed release notes

The portrait and landscape picture buttons now rotate the current paper and its artwork together. Previously these toolbar buttons only changed the next drawing's orientation, while current drawing rotation was confined to the Canvas menu.

- Click either picture button to set the current drawing's orientation. Its highlight follows the current paper, including after undo or switching tabs.
- Layers and editable vector strokes are preserved without resampling. Ctrl+Z undoes the rotation; Ctrl+Y restores it.
- The chosen orientation also becomes the preference for new drawings. Clicking the orientation already in use leaves the drawing unchanged.
- The Canvas menu retains the same orientation commands.
- Fixed a redraw error when orientation undo/redo happened repeatedly before the next screen refresh.

Validation includes simulated clicks on both toolbar buttons with existing vector artwork, repeated same-orientation clicks, pixel-exact return to portrait, undo/redo, and project/PSD orientation round trips.

Extract and run **Graphite Studio.exe**. Save any open drawings before replacing an older executable. This package retains the embedded Windows icon and console-free startup from v0.22.0.

## v0.22.0

### Release summary

- Added Vector selection (V): click an individual retained path on the active layer and resize, move or rotate it using automatic transform handles. Completing a free selection also starts a transform preview.
- Added illustrated portrait/landscape controls for new drawings and Canvas → Current drawing orientation for lossless 90° turns of paper, layers and editable paths. Orientation edits and earlier strokes remain undoable/redoable; PSD/project exports retain the result.
- Shift snaps paper-view and selection rotation to 0/90/180/270°. View gestures retain the unsnapped angle so small pointer/touch movements can cross snap boundaries naturally.
- Embedded seven sizes of the pencil application icon plus Windows version information, set a stable taskbar identity and packaged a stable Graphite Studio.exe filename.
- Main app uses the Windows GUI subsystem, so direct/pinned launches do not allocate a console. Startup failures appear in an error dialog.
- Passed 132 automated tests and native Windows resource/icon extraction checks. Did not confirm a pin-related rendering slowdown or change material/render quality settings.

### Detailed release notes

#### Select and transform individual strokes

Choose the arrow icon, **Vector selection**, or press **V**. Click a stroke on the active layer. Its transform box appears on release: drag inside to move it, drag a corner to resize, or drag the round handle above it to rotate. **Enter** applies; **Esc** cancels. Other strokes keep their geometry. Selecting a different stroke applies pending changes to the previous one. Clicking empty paper deselects. For strokes on another layer, select that layer first.

The hit test follows the retained curve and its approximate pencil width, so you do not have to hit a particular grain particle. Overlapping paths select the topmost path. The Editable paths list is still available for choosing an obscured stroke precisely, and selecting an entry now shows handles automatically.

Finishing a **Free selection** also shows transform handles immediately. This remains a pixel selection: applying a pixel transform flattens editable paths on that layer, as before. Cancelling preserves them, and Undo restores them after an applied edit. Vector selection retains the selected stroke's editable geometry and realistic material texture.

#### Portrait, landscape and rotation snapping

- The two illustrated page buttons beside the paper-size selector set **Portrait** or **Landscape** for the next drawing. Both **New** and the tab **+** use that choice.
- Open **Canvas → Current drawing orientation** for matching picture buttons that change the current drawing. Landscape turns a portrait drawing clockwise; Portrait turns a landscape drawing counterclockwise. An already matching orientation is unchanged.
- Current-drawing orientation rotates paper, pigment, layers, selections and retained paths together. Width/height swap while DPI and total pixel count remain unchanged. Samples are relocated exactly; there is no interpolation, cropping or layer flattening. Hidden layers, blending modes and opacity are preserved. **Ctrl+Z / Ctrl+Y** undo/redo the orientation change and continue to work for earlier drawing edits.
- **Shift** snaps rotation to **0°, 90°, 180° and 270°**. This applies to R mouse view rotation, two-finger twist while Shift is held, and selection rotation. Shift while resizing continues to preserve proportions.

Changing drawing orientation resets the view upright and fits it. R/touch view rotation remains a separate display adjustment that does not change exported artwork.

#### Windows icon, pinning and console

The executable now embeds 16, 24, 32, 48, 64, 128 and 256 px pencil icons plus application/version information. The window uses the same icon and a version-independent application identity. The main application uses the Windows GUI subsystem, preventing a console allocation. Startup errors are displayed in a dialog rather than disappearing into a detached console.

The package now uses the stable filename **Graphite Studio.exe**. Close the old application when convenient, extract this package into your permanent Graphite Studio folder, and run that executable. Unpin the old versioned/blank shortcut and pin the new running application once. Keep future updates at that same path. Do not pin a ZIP preview or a temporary extracted copy. Existing versioned executables are not removed automatically.

Your installed settings file was inspected and already selects **AMD Radeon 7900 XTX**. No matching Graphite shortcut was present in the accessible taskbar shortcut folder during inspection, so the reported pin-related slowdown could not be reproduced. GPU/material settings and image quality are unchanged in this revision. Preferences remain beside the executable and do not depend on a shortcut's working directory; keep your existing Graphite-Studio.settings.json when updating. No default settings file is included to overwrite it.

Implementation follows the [Rust Windows subsystem documentation](https://doc.rust-lang.org/reference/runtime.html#the-windows_subsystem-attribute) and [Microsoft taskbar application identity guidance](https://learn.microsoft.com/en-us/windows/win32/shell/appids). Icon decoding was checked with the native [Windows Shell extraction API](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-extracticonexw).

#### Validation and package

All application and validation code is Rust. **132 automated tests passed**, including individual vector selection with real resize/rotation input, free-selection completion, untouched-path retention, reversible multi-layer orientation, selection masks, earlier history, and portrait PSD/project roundtrips. Existing material, input, layer, persistence and export tests also passed.

Native Windows verification confirms GUI subsystem 2, seven embedded icon sizes, and successful extraction of both large and small application icons. Headless UI previews were inspected for orientation controls and individual transform handles. The existing installed application was left running; no visible test window or mouse/keyboard control was used. Live taskbar pinning and physical EasyCanvas gestures still need a device check.

Included: portable executable, complete source with vendored eframe, release notes, previews, license and SHA-256 checksums. The prior acceleration profiles, texture and tissue optimizations, layer/shape opacity and fullscreen features are included.

## v0.21.1

### Release summary

- Moved Windows Ink / Wintab selection into Options → Input backend, with the selected backend highlighted.
- Changes apply immediately through the existing input bridge. Switching finishes an active stroke and clears cached pressure/orientation input without cancelling a pending transform.
- Pen & touch now points to Options. Backend choices continue to be retained in saved drawing settings.
- Verified both menu choices through headless UI clicks and built the Windows executable.

### Detailed release notes

Open **Options → Input backend** and select **Windows Ink** or **Wintab**. The current choice is highlighted and changes apply immediately, without restarting.

Use Windows Ink for EasyCanvas. Wintab uses compatible Wacom / XP-Pen drivers; the existing bridge falls back to Windows Ink if it cannot open a Wintab context. Backend selection remains part of each drawing's saved settings.

The previous selector in Pen & touch has been replaced by a pointer to Options. Pressure feel and other pencil dynamics remain in Pen & touch. Acceleration profiles remain in Options below Input backend.

Verified opening Options and selecting both backends through headless egui input; built the Windows executable. No visible app or desktop input was used. Physical tablet-driver switching was not tested in this revision.

Includes all v0.21.0 improvements: shape and layer opacity, pencil-textured shapes, P/E/R shortcuts, two-finger view rotation, and fullscreen drawing view with Esc to exit. See [v0.21.0](#v0210) in the source archive for details.

## v0.21.0

### Release summary

- Added per-layer opacity with live compositing, persistence and standard PSD layer-opacity records. Pixel alpha remains separate to prevent double fading in Photoshop.
- Geometric outlines now use the current physical pencil engine, pigment, grade, shade variation, sharpness and tip profile. Added independent 0–100% Shape opacity, applied to the complete outline and retained through vector transforms and project/PSD saves.
- P selects Pencil; E selects Eraser; R selects drag-to-rotate paper navigation. Ctrl+R still rotates selected marks.
- Two-finger twist rotates the view together with pinch and pan. Drawing, pen azimuth, selection and transform overlays use inverse/forward view mappings.
- Fullscreen fits the whole rotated drawing and hides panels; Esc exits. Rotation uses the existing canvas texture without resampling document material.
- 127 automated tests passed across the full suite and the additional dense-opacity test. Headless UI previews inspected; EasyCanvas hardware twist and live fullscreen transitions remain for device testing.

### Detailed release notes

#### Layer and shape opacity

Select a layer, then use **Opacity** below its blend mode. Each layer retains its own 0–100% setting. It changes the displayed result without removing graphite or editable paths. Layer ordering, visibility and blending continue to work independently. PSD writes this value as standard layer opacity, with the original pigment alpha kept separate; Graphite projects also preserve it. Older projects open with full layer opacity.

Choose **Shapes** for Line, Circle, Triangle or Square. New outlines use the Pencil tool's current color, grade, shade variation, tip sharpness and worn tip texture. Stroke width scales that physical contact to the chosen px size. The former round powder brush and fixed flow override are removed. Set the pencil's appearance in Pencil controls, then switch to Shapes.

**Shape opacity** controls new outlines from 0–100%, independently of layer opacity. It fades the complete outline rather than reducing each segment's flow, preserving the grain and avoiding extra opacity buildup at joins. Color is retained in pigment space; lowering opacity does not replace pigment with gray or white. Saved paths retain this setting through resize, rotation and reopening. Existing shapes keep their saved rendering settings. PSD's native vector alternatives include the shape opacity combined with their parent layer opacity; visible graphite layers retain the textured result.

#### Shortcuts and paper navigation

- **P** selects Pencil.
- **E** selects Eraser.
- **R** enables paper rotation: drag around the paper's center. Press R or Esc to leave rotation, or P/E to change tools. Reset rotation appears while this mode is active.
- **Ctrl+R** continues to rotate selected strokes, separately from the paper view.
- **Two fingers:** drag to pan, pinch to zoom, twist to rotate. These gestures can work together when EasyCanvas forwards multi-touch events. The remaining finger is prevented from drawing while a navigation gesture ends.
- **Fullscreen** fits the entire drawing, including a rotated sheet, and hides the panels. It refits after the fullscreen window resizes. **Esc** leaves fullscreen and fits the drawing into the regular workspace. Tool panel visibility is preserved.

View rotation does not rotate or degrade document pixels, editable paths, or exported artwork. Pen positions, live tilt direction, selections and transform handles map through the rotated view. The existing canvas texture is drawn as a rotated mesh, with no extra document-sized raster or material replay. Each open tab retains its view during the session; reopening a saved project starts the view upright.

#### Validation

All application changes are Rust. The full automated suite passed 126 tests; an additional targeted dense-opacity regression test passed (127 total). Coverage includes exact pencil-engine equivalence for all four shapes, selected pigment RGB, 0/50/100% shape coverage, existing-color overlap, native PSD opacity at 8/16 bits, layer and shape persistence, undo/redo, inverse rotation mapping, combined gesture anchors, multi-touch twist packets, shortcut separation, and fullscreen entry/exit commands.

The actual egui interface was rendered headlessly and inspected: shape controls, layer opacity and a fullscreen rotated drawing fit without clipping. No visible application or desktop input was used while you were working. The physical Apple Pencil/EasyCanvas twist gesture and the operating system's live fullscreen transition still need a device check. Photoshop itself was not opened during this revision.

The portable package includes the Windows executable, complete source, license, previews and SHA-256 checksums. Existing acceleration profiles and prior tissue optimizations are included.

## v0.20.0

### Release summary

- Added persistent 0–100% Pencil tip sharpness, affecting drawing contact, cursor geometry and PSD vector outlines. Older settings default to the previous standard point.
- Halved the base tip abrasion rate, replaced fixed per-stroke height smoothing with proportional conservative abrasion, and normalized sampled wear area across DPI.
- Clarified the Fit to screen button and preserved pending transforms during this view-only action.
- Optimized large tissue strokes in Rust using fast remembered-pixel membership and simplified radial falloff; measured about 5.7× faster at 800 px with the same rendered pixels and exact undo/redo.
- 117 automated checks passed, plus production tissue benchmarks. See [v0.20.0](#v0200).

### Detailed release notes

#### Pencil tip and view

- **Tip sharpness** appears directly below Core diameter in the Pencil controls. 0% is blunt, 50% is the previous standard point, and 100% is fine. This adjusts the physical contact point rather than changing the canvas resolution or graphite color. Pressure and tilt still control the contact. It applies to new strokes; saved strokes retain their own setting for later vector transforms and native PSD outlines.
- **Sharpen pencil** restores a fresh worn surface at the chosen sharpness and keeps tip rotation. Changing the sharpness slider alone does not erase existing wear. Older projects without this setting open at 50%; PSD/.graphite saves retain the new value.
- **Fit to screen** is the clearly named top-bar button beside Undo/Redo. It centers the entire drawing in the available canvas area and resets pan/zoom without resizing the document or canceling a pending transform.

Tip wear has a 50% lower base abrasion coefficient. More significantly, the former fixed smoothing after each pen lift has been removed: only accumulated abrasion is distributed across neighboring tip cells. Small strokes no longer reshape the tip disproportionately, and recessed graphite is not grown back by smoothing. Wear remains dependent on travel, pressure and grade. Sampled pixel area is now included in DPI normalization so higher-resolution documents do not artificially accelerate wear.

This is a calibrated approximation, not a measured wear rate for a specific manufacturer's pencil. Grade behavior remains informed by the relationship between graphite/clay content and hardness described in [Faber-Castell's pencil FAQ](https://www.faber-castell.com/service/frequently-asked-questions/faq-pens-and-pencils).

#### Faster large tissue strokes

All application changes are native Rust. Broad strokes now use a compact remembered-pixel bitset to avoid repeated hash lookups when capturing undo state. It is allocated only after a transaction grows beyond 1,024 pixels. Tissue's rotationally symmetric falloff is evaluated directly, removing unnecessary rotation, divisions and square roots. The soft stain, pressure/load response, selection clipping and finite graphite capacity remain intact. No reduction in document resolution or tissue sampling density was introduced.

Production-engine benchmark on this computer, same 60 short segments on an A4 document at 120 dpi:

| Tissue size | Before | Verified optimized run | Approximate speedup |
| --- | --- | --- | --- |
| 200 px | 268.96 ms | 55.04 ms | 4.9× |
| 400 px | 1047.28 ms | 234.18 ms | 4.5× |
| 800 px | 4329.37 ms | 760.99 ms | 5.7× |

These timings cover stroke deposition and undo capture, not display rendering or the final history commit. They are observed workload timings, not guaranteed frame rates on other computers. Compared rendered images differ by at most 1/255 in a color channel; the 200 px and 800 px comparisons were pixel-identical. Large-stroke undo/redo is exact.

#### Verification and compatibility

117 automated checks passed across core/project, application/interface/input and export tests, including the focused Fit to screen check. New checks cover sharpness geometry and backward-compatible settings, work-proportional wear across repeated pen lifts, and no tip-material regrowth. The actual interface was rendered headlessly and inspected. The Rust benchmark separately validates large-tissue image comparisons and undo/redo.

Existing drawing files remain readable. Old drawing pixels are not automatically repainted; transforming a retained path regenerates it using the current solver and its saved settings. Custom pencil, PSD vectors, antialiased display filtering, acceleration profiles and tablet controls remain available. Physical Intel HD performance has not been measured here. No visible application window was opened and no desktop input was taken during this work.

Extract the package and run Graphite-Studio-v0.20.0.exe. The versioned Intel startup launcher remains available for older graphics hardware. Source and benchmark reports are included.

## v0.19.2

### Release summary

- Replaced nearest-neighbor canvas magnification with bilinear reconstruction in the Vulkan/DirectX display pyramid and Intel/OpenGL fallback. Enlarged graphite edges interpolate smoothly rather than showing square texel steps; existing drawings benefit immediately.
- Preserved native document detail, material noise, editable paths and exports. Uses ordinary hardware texture filtering without an additional full-screen pass or higher-resolution document allocation.

### Detailed release notes

Graphite now uses smooth bilinear interpolation when the canvas is enlarged. The previous nearest-neighbor magnification exposed square pixel steps along curved and diagonal strokes.

This is enabled in General, AMD Radeon 7900 XTX and Intel HD Graphics modes, and applies immediately to existing drawings. General/XTX retain their multi-level filtering when zooming out. Intel retains its lighter display path.

The change interpolates neighboring pixels during display; it does not blur or resample the saved drawing. Paper grain, graphite material, editable paths, colors and PNG/PSD exports remain unchanged. At exact pixel-for-pixel alignment the original pixels are preserved. Enlargement becomes smoother but cannot invent detail beyond the drawing's native resolution.

This uses hardware texture filtering, without a new full-screen effects pass, supersampled document or extra material simulation. The Custom pencil name and all v0.19 acceleration profiles remain included.

Validation: checked the actual OpenGL painter's interpolated edge pixels and the Vulkan display pipeline; inspected a generated curved-stroke comparison at enlarged view. The application build passed. Intel hardware-specific performance remains unmeasured on the physical HD 2000.

## v0.19.1

### Release summary

- Renamed the Brush tool to Custom pencil in tool headings, tooltips, new stroke labels, import error messages and the tool guide. ABR import and saved project compatibility remain unchanged.

### Detailed release notes

The Brush tool is now called **Custom pencil** in tool headings, tooltips, new stroke labels, import error messages and the tool guide. It still supports imported ABR tips.

Existing drawings and editable PSD projects remain compatible. Previously saved stroke names are preserved. All v0.19 acceleration modes remain available; their notes are included in this package.

## v0.19.0

### Release summary

- Added Options → Acceleration mode with persistent General, AMD Radeon 7900 XTX and Intel HD Graphics preferences, actual renderer identification and restart guidance.
- Added automatic legacy Intel detection and an OpenGL compatibility path, including a desktop 2.1 context fallback for drivers without OpenGL 3.3/GLES.
- Intel mode removes expensive display filtering passes, interface shadows/animations and startup dithering. Canvas updates are paced while preserving every processed input sample; pen-up flushes immediately.
- XTX mode prefers AMD discrete hardware and a low-latency presentation queue. Drawing physics and export quality remain unchanged.
- 113 automated tests passed. Verified production Vulkan and OpenGL display paths on the installed RX 7900 XTX; physical Intel performance remains to be tested.

### Detailed release notes

Choose **Options → Acceleration mode**:

| Mode | Display behavior |
| --- | --- |
| General | Balanced hardware selection, preferring Vulkan with other supported backends available; full zoom-filtering pyramid; up to 60 canvas texture updates per second while drawing. |
| AMD Radeon 7900 XTX | Prefer a discrete AMD adapter and Vulkan; one queued display frame for lower latency; full zoom-filtering pyramid; up to 120 canvas texture updates per second while drawing. Other compatible hardware remains a fallback. |
| Intel HD Graphics | OpenGL compatibility renderer for the reference HD Graphics 2000; direct texture display without the GPU filtering pyramid; interface shadows, animations and display dithering disabled; up to 30 canvas texture updates per second while drawing. |

These are upper bounds for canvas updates, not guaranteed frame rates. The screen refresh rate, drawing complexity and CPU still matter. Physical material simulation and vector replay run on the CPU in every mode. This update accelerates and tunes display work; it does not move graphite simulation to the GPU.

The menu shows the actual running graphics adapter/backend. Display pacing and lighter interface effects change immediately. **Save your drawings and reopen the app to finish changing the renderer, adapter selection and startup display settings.** Preferences are saved in `Graphite-Studio.settings.json` beside the executable. If that folder is read-only, the app reports that the preference could not be saved; the current run still uses the selected display settings.

#### Intel HD reference and startup

The screenshot's `VEN_8086&DEV_0102` identifies Intel HD Graphics 2000, Sandy Bridge. Intel lists DirectX 10.1 and OpenGL 3.1, with no Vulkan support. The compatibility mode therefore uses desktop OpenGL rather than trying to run Vulkan or DirectX 12 on that chip.

Windows startup checks attached physical display adapters. If only recognized Sandy Bridge Intel adapters are attached, the app automatically starts in Intel compatibility mode, even if a preference was copied from a modern PC. Virtual display adapters such as EasyCanvas do not disqualify this check. If detection is unavailable, use **Start Intel HD Graphics.cmd** in the extracted package; it starts the same application with `--intel-hd`. `--general` and `--xtx` are also available. Intel compatibility still requires a working driver with desktop OpenGL 2.1 or newer; Microsoft's basic OpenGL 1.1 software display driver is insufficient.

The bundled windowing library includes a documented fallback to a desktop OpenGL 2.1 compatibility context before trying GLES. This matters because its usual OpenGL 3.3 request is newer than the Intel reference supports. The vendored patch, original source and MIT notice are included in the source archive.

#### Quality and responsiveness

- Document resolution, physical paper texture, pigment colors, material deposition, layers, vector data and export bit depth remain unchanged.
- Dirty drawing regions accumulate between display refreshes. All incoming stroke samples still reach the drawing engine; tablet polling remains responsive. Lifting the pen immediately flushes the final display update.
- Intel mode avoids GPU mip generation and refreshes full-sheet diagnostic statistics less often. Smaller dirty regions still use partial texture uploads rather than uploading the entire drawing.
- Intel mode uses simpler minification filtering, so a zoomed-out preview can look less smoothly filtered than the General/XTX preview. Zooming to actual pixels and exporting preserve the original detail.
- There are no automatic reductions of drawing DPI, brush quality or layer count. Large documents, large tissue/smudge brushes and long vector replays can still be expensive on old CPUs.

#### Validation

**113 automated tests passed:** 70 core/project, 36 application/input/interface and 7 export regressions. Profile checks compare saved stroke commands and exported pixels exactly across all three modes, verify that a pending display update is flushed on pen-up, and retain undo. Preference replacement/corruption and reference hardware detection are covered. The Options menu was opened with simulated pointer events and its actual egui output rendered and inspected; a preview is included.

The production Vulkan pipeline passed full/partial upload and nine-level downsampling checks on the installed **AMD Radeon RX 7900 XTX**, using the XTX profile and its one-frame queue. The production OpenGL painter passed full/partial texture uploads and pixel readback using GLSL 1.20 and 1.40 shaders with dithering disabled. The OpenGL check used an invisible helper surface on the installed AMD driver; its tiny-probe timing is not a drawing benchmark or an Intel performance estimate.

**Physical Intel HD 2000 smoothness has not been measured here.** Please test a short pencil stroke, a broad tissue stroke, and pan/zoom on that computer. No visible application window was opened and no desktop input was taken during validation. Existing builds and drawings were preserved.

All v0.18 drawing and editable PSD features remain included; see the bundled v0.18 notes for their controls and compatibility limits.

#### References

[Intel's legacy GPU table](https://dgpu-docs.intel.com/overview/supported-hardware/legacy-gpus.html) maps device 0102 to HD Graphics 2000. [Intel's supported API table](https://www.intel.com/content/www/us/en/support/articles/000005524/graphics.html) lists the model's API support. Renderer behavior was checked against the bundled eframe/egui_glow 0.35.0 and glutin 0.32.3 source.

## v0.18.0

### Release summary

- Added a compact illustrated tool strip and collapsible Pencil / Paper details.
- Added bounded 8-bit sampled ABR import, pressure-sensitive custom brush strokes and a soft graphite-loaded Tissue tool.
- Added line, circle, triangle and square outlines with drag previews.
- Added free selection, selection-clipped drawing and Ctrl+T move/resize with apply, cancel and undo.
- Retained per-stroke vector geometry, pressure, tilt, rotation, tool settings and tip state. Individual path transforms regenerate texture; pixel region edits flatten that layer's paths, with undo restoration. Added native .graphite save/reopen and editable PSD save with embedded material state. PSD now also contains native Photoshop Bezier paths and native clean outline shape layers; original texture layers remain visible.
- Prefer hardware Vulkan for GPU display; validated full/partial uploads and mip generation on Radeon RX 7900 XTX. CPU material simulation remains unchanged.
- Clarified generic XP-Pen Windows Ink / Wintab input and tested the Star 03 pressure range.
- Added Ctrl+R rotation for vector paths and pixel selections, a rotation handle, angle field and 15-degree Shift snapping.
- 109 automated tests pass; see [v0.18.0](#v0180).

### Detailed release notes

#### Editable graphite paths

Draw with Pencil, Brush, Tissue, Smudge or Eraser, or place a geometric outline. The active layer retains each stroke's path, pressure, tilt, rotation, brush settings and initial tip state.

1. Expand **Editable paths** above the tool options.
2. Choose a stroke, or choose **All paths**.
3. Press **Ctrl+T** (or click Transform).
4. Drag inside the box to move, or drag a corner to resize. Hold Shift to preserve proportions.
5. Press Enter / Apply to regenerate the material texture. Esc / Cancel restores the original. Ctrl+Z and Ctrl+Y restore both the material and path state.

Resizing a native path reruns the graphite drawing engine at its new geometry. The interactive preview is temporarily an image for responsiveness; Apply produces the new material rendering. The paper remains a finite pixel canvas, and imported ABR tips retain their original bitmap resolution. This is a hybrid path-and-material brush system, not pure SVG artwork. Pencil widths scale independently of the physical core-size limit.

#### Save, reopen and Photoshop-native vectors

**Ctrl+S** saves an editable PSD by default. Choose `.graphite` in the save dialog for the dedicated native format. **Ctrl+Shift+S** saves a copy; **Ctrl+O** reopens a project in a new drawing tab. The File menu exposes the same actions. The former Save As image button is now **Export**; PNG, JPEG and BMP remain image exports.

Both editable formats retain physical material, drawing layers, visibility/blending/order, vector stroke commands, pressure/tilt/rotation, pencil wear, tool settings, selection, view and embedded ABR/custom-paper images. You can reopen a drawing and continue transforming its paths without the original brush or paper files. Undo history starts fresh on reopening. Saves complete a temporary file before replacing the destination; invalid/truncated projects are rejected without replacing the current drawing.

The PSD export contains real Photoshop-native objects:

- **Paths panel:** named Bezier centerlines for every retained stroke. These are standard PSD path resources, directly editable with Photoshop's path tools. Midpoint quadratic drawing curves are converted to cubic Beziers. When a drawing exceeds the PSD range of 998 named path resources, strokes are collected into paths by drawing layer.
- **Layers panel:** native shape layers named **Vector - ...** for pencil, brush, tissue and geometric strokes. These use solid color fills and vector masks, with no stored pixel channels. Pencil/brush/tissue alternatives approximate the clean footprint using stroke geometry and pressure; they do not vectorize ABR grain or tissue feathering. Smudge and eraser retain centerlines but do not become independent filled shapes.
- **Original artwork layers:** the exact graphite texture and layer stack remain visible. The clean native vector alternatives are initially hidden, so the PSD opens with the same appearance. Enable a Vector layer in Photoshop to work with that alternative; hide the corresponding original artwork when comparing it. Changing a Photoshop path does not automatically repaint the original graphite pixels.

PSD can represent native paths and shape layers; the graphite material model and sampled texture are a separate representation. Graphite's exact physical state is additionally stored in the PSD for reopening here, **alongside the actual native Photoshop geometry**. This extra data is not what makes the Photoshop paths and shapes editable.

**External PSD changes are not imported into the material model.** If another program changes/re-saves the PSD, Graphite detects that its saved material state may no longer match and refuses to silently restore a stale drawing. Continue external edits in Photoshop, or reopen a `.graphite` copy for material editing here. Ordinary PSDs without Graphite state cannot currently be imported as physical drawings. Native .graphite files allow up to 512 MB compressed / 2 GB decoded data, 256 layers and the existing 12-megapixel document limit; custom paper source bitmaps are limited to 32 million pixels.

#### Rotate a selection

After selecting a path or tracing a free selection, press **Ctrl+R**. Drag around the center to rotate, or enter a precise angle in the options panel. Shift snaps to 15-degree increments. The round handle above a Ctrl+T box also rotates it. Enter applies; Esc cancels; Ctrl+Z restores the previous result. Vector rotation updates geometry and regenerates texture, including the orientation of sampled brush tips. Pixel selections rotate their selected material and selection outline. Rotations persist when saved and reopened.


#### Free selection and transform

Choose the lasso icon and trace around an area. Pencil, Brush, Tissue, Smudge and Eraser affect only the selected region. Ctrl+D deselects, Ctrl+A selects the whole paper, and Delete clears selected material.

Ctrl+T with a free selection moves/resizes the selected pixels; Ctrl+R rotates them. **Applying a pixel-region transform or deleting a pixel region flattens that active layer's existing editable paths into its material base.** The controls say this before applying. Undo restores the paths as well as the original material. Choose a path from Editable paths to keep vector editing instead. The paper and other layers stay intact.

A transform preview supports up to 4 million selected pixels; a target is limited to 16 million pixels of area. Edits outside the finite paper are clipped. Transform application replays material on the CPU and may take longer on large drawings with many paths. Marks outside the edited path's old/new areas remain unchanged.

#### Tools and controls

The compact illustrated strip keeps Pencil, Brush, Tissue, Smudge, Eraser, Shapes and Free selection available even when the options panel is hidden. Pencil and paper diagnostics are collapsed. Existing light document tabs, resizable options panel and layer controls remain. The **?** button opens a tool guide.

- **Brush / ABR:** choose Brush, click Import ABR brushes, and select a sampled tip from the dropdown. Adjust Size and Flow. The package includes an original `Graphite-grain-sample.abr` for trying the import. ABR versions 1, 2, 6 and 10 are supported for 8-bit sampled tips, raw or PackBits compressed. Rectangular tip masks retain their shape rather than being clipped to a circle. Photoshop computed brushes, 16-bit tips and Photoshop dynamics/descriptors are not imported. A new import replaces the current tip list; previously drawn strokes retain their tip. Saving a project embeds the current tip library and each stroke’s retained tip, so reopening does not require the ABR file. Input limits: 128 MB file, 64 MB decoded masks, 512 tips, 8192 pixels per side and 16 million pixels per tip.
- **Tissue:** a graphite-loaded tissue deposits a soft stain in the chosen pencil color on the active layer. Graphite load adjusts strength; repeated passes deepen tone. Load 0% changes nothing. Smudge remains the tool for moving existing graphite.
- **Shapes:** choose Line, Circle, Triangle or Square; drag to preview and release to commit a colored outline. Esc cancels. Each outline is an editable path. The initial circles and squares preserve their proportions; later transforms can stretch them.
- Existing optional pencil smoothing, pen dynamics, two-finger paper pan/pinch and 0–100% eraser lift remain available.

#### Vulkan and tablets

Hardware Vulkan is preferred for GPU canvas display, zoom and downsampling when available. Other supported hardware adapters remain fallback choices; `--dx12` explicitly selects DirectX 12. **Material deposition, smudging and vector replay still run on the CPU.** The Pen & touch area accepts Windows Ink and generic Wintab input from compatible Wacom / XP-Pen drivers.

For XP-Pen Star 03, start with Windows Ink enabled in the tablet driver. If pressure is absent, choose Wintab in the app and configure the driver consistently. The advertised 8192 pressure levels correspond to raw values 0–8191, which the normalization test covers. Actual tablet input has not been tested with physical XP-Pen hardware here. Tilt and barrel rotation work only if the tablet, pen and driver report them; this update cannot add sensors to unsupported pens. Existing EasyCanvas forwarding requirements still apply.

#### Validation and package

- **109 automated tests passed:** 68 core/project tests, 34 application/input/interface tests and 7 export regressions. New checks include supported ABR versions and both compression types, malformed/truncated inputs, rectangular masks, tissue color/load/falloff, lasso clipping including eraser/smudge, shapes, retained pressure/rotation, vector scaling, selected eraser path movement, undo/redo, cancellation and layer isolation.
- Actual egui controls were rendered headlessly and inspected. Pointer-driven tests used a transform corner handle to resize a shape and Ctrl+R to rotate a stroke. Tests also cover vector/pixel rotation, saving then reopening in a new application instance, both PSD bit depths, embedded brush/custom paper recovery, rejected external PSD edits, corrupt files and safe replacement saves. Pencil, Brush, Tissue, Shapes, Lasso, Eraser, collapsed-panel and guide previews are included. The final small layout changes were followed by focused interface/transform checks.
- A headless production GPU check passed on **AMD Radeon RX 7900 XTX / Vulkan**: full upload, partial upload and GPU mip downsampling through a nine-level display pyramid.
- The tool study's layered **16-bit PSD** was independently opened and recomposed with psd-tools. It retained Paper, a Multiply artwork layer, 12 native Photoshop Bezier paths and 12 native shape alternatives. The independent reader recognized the alternatives as shape layers without pixels and rendered them from their vector masks. Maximum RGB difference in the original textured composite was 1 out of 255. Actual Photoshop GUI editing was not tested on your occupied desktop.
- No application window was launched, and no mouse/keyboard control was taken during these checks. Existing packages and the running application were preserved.

The complete package contains a portable Windows executable, source, original sample ABR, editable tool-study PSD/.graphite plus PNG, previews, validation reports and SHA-256 checksums. It uses the project's optimized development build profile. Build: `cargo build --offline --bin graphite-studio`; tests: `cargo test --offline`. GPU probe: `cargo run --offline --bin gpu_check -- report.txt`.

#### Compatibility references

[Adobe describes ABR brush-pack import](https://helpx.adobe.com/photoshop/desktop/apply-painting-techniques/brushes-presets/import-brushes-brush-packs.html). Sampled-tip record layouts and PackBits organization were checked against the [GIMP ABR loader](https://raw.githubusercontent.com/GNOME/gimp/master/app/core/gimpbrush-load.c); Graphite Studio's bounded parser is independently implemented.

[XP-Pen's Star 03 specifications](https://www.xp-pen.com/ie-store/buy/star-03.html) describe its pressure range. XP-Pen's [Star 03 V2 manual](https://download01.xp-pen.com/file/2020/06/Star%2003%20V2%20User%20Manual%28Spanish%29.pdf) documents the Windows Ink driver setting. No tablet driver was installed or changed.


Adobe documents the distinction between [native shape layers, paths and raster painting](https://helpx.adobe.com/photoshop/using/drawing.html). The native path records, vector masks, solid-color fill descriptors and layer flags follow [Adobe’s PSD file-format specification](https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/).

## v0.17.0

### Release summary

- Added two-finger pan combined with anchored pinch zoom, for fitted and enlarged paper.
- Added optional pencil line smoothing from 0–100%, preserving sensor dynamics.
- Changed eraser lift to 0–100% with true no-op and complete active-layer pigment removal at the endpoints.
- 91 automated tests pass; see [v0.17.0](#v0170).

### Detailed release notes

- **Two-finger paper movement:** drag two fingers over the drawing to pan the sheet. Pinch and pan work together, keeping the drawing point beneath the moving finger center. This works with fitted paper and enlarged drawings, within the existing scroll boundaries. The tool panels do not zoom. Navigation remains active until all fingers lift, preventing a remaining finger from drawing. Pen contact takes precedence.
- **Pencil → Line smoothing, 0–100%:** optional path stabilization reduces wobble. It starts at 0% (off); higher amounts smooth more and introduce more pen lag. Try 30–50% and adjust to taste. Pressure, tilt and barrel rotation remain independent. The filter resets with each stroke and does not add a catch-up tail at release. It works with mouse, Windows Ink and Wintab input and is stored with each drawing's tool settings.
- **Eraser → Lift strength, 0–100%:** 0% changes nothing and creates no undo entry. Intermediate values progressively lift material. At 100%, both vinyl and kneaded erasers completely remove all pigment in the contacted footprint, including compacted material. The erased portion of the active layer becomes transparent; underlying layers and the existing paper surface remain. Full lift does not add abrasion marks or paint white, and undo restores the removed material.

#### Validation

All **91 automated tests passed**. Checks cover two-finger drag through the actual egui drawing interface with fitted/enlarged paper, simultaneous pan/pinch anchoring, single-finger release protection, unchanged UI scale, smoothing across sample densities and zoom levels, sensor preservation, smoothing in the pencil stroke path, eraser endpoints and undo/redo. Existing layer and export checks also passed.

Inspected software-rendered previews of the actual Pencil and Eraser controls. Your running application was not controlled or replaced. Actual EasyCanvas/iPad forwarding still needs a device check: place two fingers over the paper and move them together, then pinch while moving. EasyCanvas must forward multi-touch positions for panning; a bridge that sends only zoom events cannot provide two-finger translation.

This package includes the portable Windows executable, source and interface previews. Earlier versions are preserved. The executable uses the project's optimized development profile. Build with `cargo build --offline --bin graphite-studio`; test with `cargo test --offline`.

The adjustable smoothing control is inspired by the purpose of [Procreate's stabilization settings](https://help.procreate.com/procreate/handbook/5.4/brushes/brush-studio-settings); this is Graphite Studio's own implementation, not Procreate's algorithm.

## v0.16.0

### Release summary

- Added illustrated layer actions and contiguous light document tabs with status, close controls and drag reordering. Removed the Navigation help block.
- Removed forced gray pigment normalization and neutral highlights; selected RGB survives material transfer and rendering.
- Added optional legacy Wintab input, driver-normalized pressure, available tilt and barrel rotation. Windows Ink now also passes valid barrel rotation.
- Reopen Wintab mapping after display topology changes; preserve manual controls when sensors are unavailable.
- See [v0.16.0](#v0160) for validation and hardware limits.

### Detailed release notes

#### Workspace

- New vector icons for Add layer, Delete layer and the Layers panel toggle/header, with tooltips and accessible names. Deleting the final layer remains disabled.
- Contiguous light tabs show the drawing name, zoom, active layer and an asterisk for changes since export. Click to switch, use the close button or middle-click to close, and drag to reorder. Long names are clipped with their full title available on hover; the strip scrolls horizontally.
- Closing a drawing still asks before discarding its session state. Closing the final drawing creates a fresh blank sheet. Tabs preserve independent layers, undo history and view settings, but are not restored after exiting. Export work before closing the application.
- Removed the Material panel's Navigation help section. Existing drawing zoom/pan shortcuts and gestures continue to work.

#### Selected color

- Removed the fixed graphite darkness, gray mixing and neutral reflected highlight that altered selected pigment colors. The material now carries the selected sRGB pigment through drawing, smudging, preview and layered export.
- Shade variation remains available for related lighter/deeper pigment tones. Set it to **0** for unmodified pigment RGB. Pressure and paper grain control coverage; transparency, paper color and layer blending still affect the visible pixel. Multiply remains the default; Normal avoids multiplying pigment by underlying colors.
- The included color study places the exact selected swatch at left, production-engine strokes with variation 0 in the middle, and variation 0.24 at right. Rows use RGB (235,35,55), (20,160,65), (35,75,240) and (104,104,104). These are rendered marks on white paper, not retouched reference images.

#### Bamboo, Intuos, Cintiq and EasyCanvas

- Windows Ink remains the default. For an older Bamboo or a Wintab configuration, open **Material → Pen & touch → Tablet input → Wacom Wintab**. Use Pen mode in the installed Wacom driver. No driver is installed or bundled by this application.
- Wintab reads the driver's pressure range and supported orientation axes, drains ordered samples and avoids duplicate Windows Ink drawing. Unsupported tilt or rotation retains the manual fallback. Barrel rotation is independent of the direction the pen leans; it rotates the persistent tip contact profile. Windows Ink also passes barrel rotation when its validity flag is present.
- Wintab rotation becomes active after twist changes are observed, avoiding a false zero-angle sensor on ordinary pens. A rotation-capable pen is required; this does not add rotation hardware to a Bamboo or a standard pen.
- Losing focus cancels active Wintab contact. Display topology changes reopen its mapping, supporting EasyCanvas disconnect/reconnect alongside the existing window recovery. An unavailable Wintab connection leaves Windows Ink/mouse fallback available.
- EasyCanvas continues using Windows Ink for forwarded Apple Pencil pressure and tilt, plus drawing-only pinch zoom.

#### Validation and device check

- **85 automated tests passed**, including real egui pointer events for layer buttons/tab activation/close/drag, tab state isolation, pigment fidelity, pen rotation interpolation, optional Wintab packet layouts, pressure normalization, existing eraser/layer/gesture behavior and exports.
- Inspected software-rendered previews of the actual interface with expanded, widened and collapsed panels, plus production-engine color swatches. Your running apps were not controlled or replaced.
- Independent PSD validation preserved seven layers, names, visibility, transparency and all five blending modes in both 8-bit and 16-bit exports. Recomposition differed from the preview by at most **1/255** per RGB channel.
- Actual Bamboo/Intuos/Cintiq and EasyCanvas hardware forwarding remains unverified. On your Bamboo, select Wintab if needed and confirm that the pressure readout and stroke respond to light versus firm contact. On a compatible Intuos/Cintiq pen, also check the tilt readout; test barrel rotation only with a pen that supports it.

The portable executable uses the optimized development profile. Previous packages are preserved. Source and fixtures are included.

#### Primary references

- [Wacom Wintab reference](https://developer-docs.wacom.com/docs/icbt/windows/wintab/wintab-reference/) and [official Windows samples](https://github.com/Wacom-Developer/wacom-device-kit-windows): context mapping, packet layout and sensor axes.
- [Wacom Windows Ink support](https://developer-support.wacom.com/hc/en-us/articles/12844630619415-Windows-Ink).
- [Microsoft pen properties](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-pointer_pen_info): validity flags and barrel rotation.
- [EasyCanvas capabilities](https://www.easynlight.com/en/easycanvas): forwarded pressure, tilt and finger gestures.

Build: `cargo test --offline`, then `cargo build --offline --bin graphite-studio`.

## v0.15.0

### Release summary

- Added Windows Ink pen pressure and tilt, coalesced sample history, fractional pen coordinates and adjustable pressure feel. Removed invented pen-entry force and release extensions.
- Added pinch zoom of the drawing, with finger navigation isolation and a stationary-finger release guard.
- Added session drawing tabs with isolated documents, histories and view positions. New creates a tab; closing a tab asks before discarding it.
- Added a collapsible, resizable Material panel, illustrated tool buttons, paper color and vinyl/kneaded erasers.
- Removed Clear graphite. All v0.14 layer, PSD, texture, rendering and screen-recovery features remain.
- See [v0.15.0](#v0150) for validation and hardware requirements.

### Detailed release notes

#### Drawing workspace

- Independent drawing tabs retain layers, visibility and blending modes, undo/redo, paper color and texture, pencil wear, smudger load, tool settings and zoom/pan position. New opens another drawing instead of replacing the current one. Export names become tab names. Closing a tab asks before discarding it.
- Tabs live within the current session. Export drawings before closing tabs or exiting; this version does not restore open tabs after restarting or import PSDs for continued physical simulation.
- Click the narrow handle on the Material panel's right edge to hide/show it. Drag the handle horizontally to resize it. The collapsed handle stays accessible.
- Illustrated pencil, paper stump and eraser buttons remain crisp at different display scales. Paper color sits directly below Pencil color and preserves the paper grain and drawing layers.
- Removed the Clear graphite button.

#### Pen and touch

- A native Windows Ink bridge reads pressure and both tilt axes before the window backend reduces the input to touch events. Compatible Wacom pens and Apple Pencil through EasyCanvas use this path when their Windows driver forwards those properties. Wintab-only configurations need Windows Ink enabled; there is no separate Wintab backend in this build.
- The bridge preserves coalesced pen samples in chronological order, including a complete short stroke between two UI frames, and uses fractional digitizer coordinates where available. Pressure filtering uses elapsed time, so its response is consistent across packet rates. Pen & touch offers a pressure-feel adjustment; 1.0 is neutral, lower values respond to a lighter touch.
- Real tilt changes the sharpened point into broad side contact, oriented beneath the leaning pencil. Reported force is used at contact and release; earlier invented entry forces and extended tails were removed. Missing pressure or tilt uses the existing manual fallback instead of treating an unsupported sensor as zero.
- Pinch with two fingers over the drawing viewport to zoom around the gesture center. It changes the drawing view only, not document resolution or panel size. Bridges that translate gestures into Ctrl+wheel or Zoom events are also supported. A provisional first-finger stroke is rolled back when a pinch begins; drawing remains paused until all fingers lift. Pen contact takes precedence over touch navigation.
- EasyCanvas advertises pressure, tilt and finger gestures. Actual forwarding depends on its version, settings and Pencil model. This build has been checked with simulated input, not an attached Apple Pencil or Wacom device. On your setup, confirm that the Device pressure and Device tilt readouts change during a stroke and that a two-finger pinch zooms the paper.

#### Eraser and color

- Vinyl erasing progressively lifts loose material, then more resistant compacted graphite. Kneaded erasing provides gentler lightening and much less paper abrasion. Both use pressure-sensitive contact with feathered edges and preserve remaining pigment color.
- Erasing integrates rubbing distance rather than applying a fixed removal per pointer packet. Repeated passes deepen the lift. Erasing reveals the chosen paper color; it does not deposit white paint. Undo restores material, pigment and paper state.
- Paper tint is shared by preview, image export and the separate Paper layer in 8/16-bit PSDs. Transparent drawing layers retain their pigment and blend modes.

#### Validation

- All 78 automated tests pass.
- Automated checks cover pen packet order and release, pressure availability and filtering, tilt quadrants, DPI conversion, pinch direction/anchor and UI-scale isolation, tab content/history/view isolation, panel clicks and dragging, eraser lift/integration/undo, paper tint and previous layer/export/screen-recovery behavior.
- Headless previews render the actual egui interface with expanded, widened and collapsed panels; no mouse or keyboard control of your running app was used.
- An independent PSD reader opened and recomposited colored-paper files with seven layers, all five modes, hidden and empty layers, Unicode names and transparency. Maximum RGB channel differences from the saved preview were 2/255 for 8-bit and 1/255 for 16-bit.
- Hardware pressure, tilt, pinch forwarding and EasyCanvas monitor removal still require an actual device check. This package leaves earlier builds intact.

#### Research references

- [EasyCanvas official capabilities](https://www.easynlight.com/en/easycanvas): pressure, tilt, palm rejection and finger gestures.
- [Microsoft pen properties](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-pointer_pen_info): valid pressure range and tilt axes.
- [Microsoft pen sample history](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getpointerpeninfohistory): reading coalesced samples before the next message.
- [Wacom Windows Ink support](https://developer-support.wacom.com/hc/en-us/articles/12844630619415-Windows-Ink).
- [Faber-Castell kneadable eraser](https://www.faber-castell.com/products/Kneadableerasergrey/127020): absorbent graphite lightening. Eraser coefficients are artistic approximations, not measured material specifications.

#### Build and checks

The portable Windows executable uses the project's optimized development profile (application and dependencies at optimization level 2). Source and validation fixtures are included in the complete package.

```
cargo test --offline
cargo build --offline --bin graphite-studio
cargo run --offline --bin psd_study -- psd-study
```

## v0.14.0

### Release summary

- Added automatic recovery onto an available screen after EasyCanvas/monitor loss, plus Ctrl+Shift+Home manual recovery onto the main screen.
- Reduced harsh white texture gaps and added stable lighter/deeper pigment shades.
- Converted pencil/eraser size controls to px without resampling or changing internal precision.
- Added drag-and-drop layer ordering and five blend modes, defaulting to Multiply.
- Preserved drawing layers, Unicode names, visibility, transparency and modes in 8/16-bit PSD exports, with paper separate.
- Accelerated large CPU refreshes, simplified equivalent orientation shading, and cached sidebar diagnostics after edits.
- See [v0.14.0](#v0140) for measurements, validation and format details.

### Detailed release notes

#### Changes

- Windows screen recovery checks display work areas about every 750 ms. A stranded window is moved onto an available display after two observations, including automatic minimization during a monitor disconnect. Intentional minimization and valid secondary-screen placement remain intact. Ctrl+Shift+Home manually moves the focused app to the main screen if EasyCanvas keeps a disconnected iPad registered as a display. The document remains in memory throughout recovery.
- Softer graphite texture: shallow deposits bridge unresolved paper fibers, reducing harsh white cutouts while retaining lighter valleys and irregular edges.
- Multiscale shade noise includes lighter and slightly deeper shades of the selected pencil color. It is precomputed in physical coordinates, stable between strokes, and transported with the material when smudging. Default Shade variation is 0.24; adjustable from 0 to 0.35.
- Pencil core and eraser diameter controls now show document pixels (`px`). Internal millimeter values are converted using the current DPI; merely displaying the controls never rounds those values. Canvas sampling, GPU preview reconstruction and true 16-bit export precision are retained.
- Drag layer names to reorder. The insertion line shows whether the drop will go above or below a row; long lists scroll while dragging at the edge. Existing Up/Down buttons remain available.
- Each layer has Multiply (default), Normal, Darken, Screen or Lighten blending. Multiply uses the graphite's physical optical density as transparency, preserving paper detail and deepening overlaps. Reordering preserves the active layer and its material, visibility and mode.
- PSD export now preserves every drawing layer, including hidden and empty layers, Unicode names, order, transparency and blend mode. Paper is a separate bottom layer. Both 8-bit and true 16-bit/channel files include a merged preview; 16-bit layers use the Photoshop `Lr16` structure.
- Large CPU refreshes use up to four workers, while small drawing patches avoid thread scheduling. Equivalent vector projection replaces per-pixel angle conversions. Sidebar diagnostics refresh after edits instead of scanning the full sheet every frame.

#### Validation

All 66 automated tests pass. Actual EasyCanvas/iPad disconnection has not yet been exercised in this revision; monitor loss and window placement are covered by deterministic tests. The manual recovery shortcut works on the focused app, so select Graphite Studio with Alt+Tab first if its window is off-screen.

Automated checks cover physical deposition, input packet rates, DPI, shade range and white gaps, unchanged px-control values, serial/parallel pixel equivalence, blend math, layer identity, an actual egui pointer drag sequence, and window recovery for removed displays, automatic/intentional minimization, negative monitor coordinates and small fallback screens.

The generated PSD fixture contains all five modes, a hidden layer, an empty layer and Unicode names. An independent `psd-tools` reader reopened both bit depths and recomposited their layer channels. Both differed from the app preview by at most one 8-bit channel level. Native Photoshop opening has not yet been tested in this revision.

CPU benchmark: identical 1748×2480 document at 300 DPI, half covered, optimized development builds, median of five full renders and 31 patches. Baseline v0.13: full render 181.444 ms; 256×128 patch 1.860 ms. Final v0.14: full render 66.566 ms (about 2.7× faster); patch 1.638 ms (about 12% less time). These measure CPU rendering, not end-to-end drawing FPS; device load can affect timings. No resolution or sample-count reduction was used for the performance changes.

#### Reproduce

```
cargo test --offline
cargo run --offline --bin realism_study -- study
cargo run --offline --bin render_benchmark
cargo run --offline --bin psd_study -- psd-study
```

#### Sources and limits

Graphite contact research: [Sousa and Buchanan, Observational Models of Graphite Pencil Materials](https://ires.cpsc.ucalgary.ca/publ/papers/2000/refs/Costa%20Sousa%20%26%20Buchanan%20%2700.pdf). PSD encoding: [Adobe Photoshop File Formats Specification](https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/).

The texture is an artistic approximation informed by paper contact mechanics and the supplied visual references. Numeric grain scales are not laboratory calibration. Real tablet pressure/tilt was not tested. PSD preserves editable optical image layers; it does not encode the application's physical graphite simulation state for round-trip editing.

## v0.13.0

### Release summary

- Added irregular grain contact that changes the tip boundary and local graphite transfer.
- Rebuilt paper relief with finite fiber bundles and area sampling; restored related shade variation around the pencil color.
- Made deposition depend on physical sliding distance and finite material capacity, with regression checks across input rates and DPI.
- Removed synthetic mouse entry/release tails while retaining device-pressure response.
- Fixed the sideways Layers layout, wrapped its toolbar, and gave layer rows a dedicated vertical scrolling area.
- Passed all 54 automated tests and the 69-case swatch run. Live Add was checked directly; the user confirmed show/hide and reordering.
- See [v0.13.0](#v0130) for research, reproduction commands, and limitations.

### Detailed release notes

This revision addresses the hard circular outline, coarse artificial texture, weak shade variation, input-rate-dependent darkness, and sideways Layers layout in v0.12.1.

#### What changed

- Contacting graphite grains now perturb the actual tip surface and its boundary. They also carry different loads. The grain pattern stays attached to the pencil and follows barrel rotation.
- The paper combines area-sampled irregular relief with finite fiber bundles of varied length and orientation. Pixel-grid random noise and crossed directional noise bands were removed.
- A single pressure/contact calculation reaches raised paper first and progressively engages valleys. The multiple contact floors and redundant threshold stages that suppressed texture were replaced.
- Deposition integrates sliding distance in millimeters. Midpoint sampling reduces endpoint bias, and zero-distance motion packets do not deposit material. Finite-capacity exponential accumulation replaces the old nonzero saturation floor.
- Default color variation is 0.16. Deposited shades vary proportionally around the selected pencil color and travel with the graphite when smudged. The renderer no longer nearly cancels this variation by normalizing every shade toward the same gray.
- Mouse strokes keep rounded starts and do not grow an invented tail beyond the last pointer position. Actual pressure still changes contact width and density.
- Material and Layers areas use explicit vertical layouts. Material controls scroll; Layers has readable wrapped buttons and a vertically scrolling list with full-width selection rows.
- The executable title and Material panel show v0.13.0.

#### Validation

All 54 automated tests pass. The 69-case swatch run completes. The Layers layout and Add action were checked in the native app; the user confirmed that drawing on Layer 2, toggling visibility, and moving it Down/Up work correctly. Hardware stylus behavior was not tested.

Run `cargo test` for the material, pressure, input, export and layer tests, including new checks for packet-rate independence, deposition across DPI, stationary input, paper-correlated texture, and layer storage during reordering/removal.

Run `cargo run --bin realism_study -- study-output` to reproduce HB/2B/4B comparisons at 120 and 300 DPI. Rows use 15%, 45%, and 85% pressure. Each cell has an upright stroke, a single side stroke, and three side passes. All images come from the production engine and renderer.

Run `cargo run --bin calibrate_graphite -- calibration-output` for the existing 69-case swatch and metrics export. These are synthetic regression fixtures, not empirical calibration against measured pencil force or a particular manufacturer's grades.

#### Research basis and limits

Sousa and Buchanan, *Observational Models of Graphite Pencil Materials*, Computer Graphics Forum 19(1), 2000, pp. 27–49:
https://ires.cpsc.ucalgary.ca/publ/papers/2000/refs/Costa%20Sousa%20%26%20Buchanan%20%2700.pdf

Its microscopy-informed model motivates pressure-dependent contact with paper relief, material accumulation and changing surface structure. This implementation is an interactive approximation; its numeric fiber/grain scales and transfer coefficients are not measured fits. Visual tuning also used the user's real-pencil reference and feedback about boundary breakup and related shades.

Mouse testing cannot validate real stylus pressure. This build retains the existing backend's pressure support; live tilt/barrel input remains limited by the original platform integration. At high zoom, the document's finite raster resolution remains visible.

## v0.12.1

- Fixed the v0.12 pressure regression that made the pencil behave like a near-constant-width mechanical pencil.
- Removed the heavy 42% pressure-packet low-pass response; rising device pressure now tracks at 74% per packet and release at 62%, while normalization itself is linear after the hover dead zone.
- Added one shared `pencil_effective_pressure()` response inside the graphite solver so input, contact geometry, paper penetration and transfer can be calibrated coherently instead of stacking unrelated gamma curves.
- Reworked upright contact as a pressure-grown intersection with the tapered graphite point. A 2.0 mm HB core at 120 dpi now grows from roughly 1.9 px diameter at 8% force to about 4.1 px at 45% and 6.0 px at 90%, while remaining bounded by the real core.
- Added slight pressure/tilt-dependent apex ellipticity and flattening so the point is not a perfectly circular invariant nib.
- Pressure can re-engage recessed/worn parts of the persistent tip profile by moving the contact plane deeper.
- Removed per-stroke random flake density from mesoscopic contact. Graphite texture is now driven primarily by stable paper support/relief plus persistent tip-facet state; optional value variation is paper-tied rather than stroke-random.
- Default synthetic graphite value variation is now zero; paper tooth and tip state provide the default texture.
- Deleted the unused legacy `micro_surface.rs` prototype so the source tree no longer carries a second, inactive graphite-contact model.
- Increased real HB transfer moderately at ordinary pressure without darkening the graphite body color.
- Updated pressure/contact regression tests to require substantial tapered-point width growth rather than explicitly suppressing it.

## v0.12.0

- Removed the arbitrary 0.5–64 px **Pencil size** model. The pencil now has a physical graphite-core diameter in millimeters (2.0 mm default; 1.5–4.0 mm UI range).
- Replaced the centered circular/capsule broad footprint with a **one-sided tapered side facet**. Near-upright contact remains a small sharpened point; tilt progressively exposes the facet behind the tip.
- Bounded side-contact width by the graphite core. Large circular marker-like pencil stamps can no longer be produced by increasing a size slider.
- Made pressure primarily a **material-transfer/force** control. Removed the second steep material-release pressure curve that caused normal Apple Pencil force to be nearly invisible and hard force to jump abruptly dark.
- Added a gentle stylus-pressure gamma lift (`0.82`) after the hover dead zone so ordinary hardware pressure lands in a more useful graphite range.
- Lightened the neutral graphite optical body and default graphite preset while preserving pressure/pass buildup as the source of dark marks.
- Reduced synthetic flake/tone modulation so paper tooth and physical contact geometry dominate the visible texture.
- Reduced empirical calibration from the old 900-case Cartesian product to **69 focused unique cases**; exhaustive combinations are reserved for automated regression validation.
- Updated cursor feedback to show physical side-facet extent when the pencil is tilted.

## v0.11.2

- removed the duplicated pencil-edge feathering that made large marks read as soft circular airbrush stamps.
- replaced the strong center-to-edge radial load falloff with a mostly loaded physical contact face; paper relief/tooth now provides most interior variation.
- added restrained persistent tip-facet/wear distortion to the actual contact silhouette, plus slightly stronger paper-driven edge breakup at large sizes.
- restored a lighter neutral graphite body and changed the default Graphite preset from near-black to neutral gray.
- reshaped pressure response so medium pressure remains light while hard pressure transfers much more material and expands contact more decisively.
- reduced stroke-seeded isotropic particle contrast so broad marks read as graphite on paper rather than sprayed/stippled brush texture.
- reduced profile wear rate substantially; ordinary drawing no longer drives the tip to near-total wear after only a few broad strokes.

## v0.11.1

- fixed the v0.11.0 pencil-size regression where the persistent profile could reject every sub-pixel sample below ~3 px and fixed-reference force dilution made larger sizes nearly invisible.
- profile contact now measures local recession from the untouched tip surface; the fresh cone is represented once by the analytic envelope, while faceting only modulates load/streak structure until real wear develops.
- changed force-density normalization so nominal Pencil size has only a mild bounded density effect; the stronger `area^0.82` dilution is reserved for genuine side contact from tilt and fades in above slight near-upright angles.
- increased physical graphite release at ordinary pressure and reduced low-load starvation, making default HB strokes clearly visible without converting transfer into opacity painting.
- added regression coverage for fresh sub-pixel contact, non-vanishing size sweeps, bounded size-density response, and useful default-pressure deposition.

## v0.11.0

- replaced scalar pencil `flatness` / wear-axis contact shaping with a persistent 32×32 `TipProfile` height field stored in pencil-local coordinates.
- mapped every successful paper contact back into tip-profile coordinates and accumulated abrasion only in the exact contacting regions; wear is still committed at stroke end for stable within-stroke geometry.
- added persistent barrel rotation so rotating the pencil can expose a different sharp edge/facet; added a Tip rotation slider plus ±15° controls, while Sharpen regenerates a fresh profile.
- removed cumulative wear as a global contact-size multiplier; `wear_work` is now diagnostic/calibration state only.
- strengthened default pressure behavior: more assertive point-contact scaling, a nonlinear high-pressure force drive, stronger transfer response, deeper tooth penetration and a moderated `area^0.82` force-density normalization.
- replaced the old synthetic longitudinal pencil-face ridge as the primary side-streak source with persistent profile-contact modulation; fixed-scale flake variation remains for microscopic texture.
- added `cargo run --bin calibrate_graphite` to render the full HB/2B/4B × size × pressure × tilt × pass-count matrix and export PNG, CSV and JSON calibration outputs.
- added calibration metrics for mean darkness, occupied area, edge roughness, tone variance, contact width, taper length and final tip wear.

## v0.10.0

- removed pencil-size-dependent graphite texture regimes: brush size now changes contact geometry, while paper tooth and graphite flake frequencies remain fixed to document physical scale.
- separated total stylus force from local contact load by distributing force over the rounded/tilted contact area; broad and side contacts are wider but naturally lighter per unit area.
- added a continuous texture-depth model: light force reaches mostly raised fiber tops, stronger force progressively reaches lower tooth without binary pore holes.
- made directional sub-streaking depend on side/tilt contact rather than large brush size.
- removed the unused legacy binary micro-contact solver from `core/contact.rs` and removed the old size-dependent wear normalization now that contact load is area-aware.
- added regression coverage for force distribution, pressure penetration, broad-contact buildup, and lower per-cell density at larger contact sizes.

## v0.9.3

- reworked broad-stroke graphite rendering so larger pencil sizes keep a crisp tooth/flake structure instead of blurring into a soft digital band.
- split the mesoscopic response into a diffuse graphite body plus a sharper clustered deposit field, preserving soft broad shading without losing graphite texture.
- made paper-tooth modulation truly paper-stable (no longer reseeded per stroke), and added a regression test to ensure large contacts still show visible density variation.

## v0.9.2

- fixed pencil tilt geometry so tilting broadens the contact patch instead of mainly elongating it.
- increased cross-contact width and side-contact support for laid-over strokes, making tilt behave more like a real pencil on paper.
- added regression tests to ensure tilt increases both side length and occupied graphite width.

## v0.9.1

- reworked the mesoscopic graphite-response model so larger pencil sizes retain crisp tooth/flake texture instead of blurring into a smooth digital brush.
- added stroke-space anisotropic flake-bundle response plus stable paper-space tooth modulation, keeping texture scale tied to graphite/paper structure rather than brush radius.
- preserved diffuse graphite bodies while making broad strokes sharper and more strand-like, especially under tilt and larger contact sizes.
- threaded local axial contact coordinates through the tip/contact pipeline so broad strokes can form coherent crisp sub-traces.

## v0.9.0

- Removed the expensive ~12 µm per-dab fiber/micro-deposit solver from the interactive pencil hot path.
- Replaced binary microscopic pore decisions with precomputed continuous sub-pixel paper-contact statistics derived from the existing paper height/fiber fields.
- Added a center-loaded rounded-tip pressure field so ordinary upright marks have a diffuse graphite body rather than a uniformly filled digital ribbon.
- Kept side-of-pencil streaking primarily tilt-driven; simply increasing Pencil size no longer turns a round point into a striped side-contact brush.
- Particle variation now modulates both local graphite density and restrained gray value using smooth clustered noise; it never creates white particles.
- Fixed the optical renderer so grayscale particle/value variation is no longer normalized back to one fixed body luminance.
- Strengthened pressure-to-width dynamics and entry/exit tapering.
- Added a subtle velocity-based width/deposition fallback only when the platform supplies no real stylus pressure packet.
- GPU display-pyramid updates now propagate only the dirty region through lower mip levels instead of rebuilding every mip over the whole page after every stroke update.
- Stored paper contact/edge-response fields as compact 8-bit statistics to reduce memory overhead.

## v0.8.0

- Replaced the pencil hot path's hash-gated raster pores with a sparse, physically-scaled ~12 µm cellulose-fiber/pore micro-surface.
- Added sparse per-layer micro-deposit tiles that influence subsequent contact while keeping visible marks independent from paper RGB.
- Added a rounded-conical/side-contact pencil geometry with much larger pressure-driven width dynamics and true tilt-driven side exposure.
- Added reconstructed low-force stroke entry and velocity-dependent pen-up release taper, while preserving rounded stationary taps.
- Added a Particle variation control for restrained dark-gray graphite grain variation; it cannot create white particles/holes.
- Removed redundant Layer summary information from the left tool panel; layer management remains in the dedicated Layers panel.

## v0.7.0

- Replaced backend-dependent egui mipmap behavior with a Graphite Studio-owned **GPU display pyramid**.
- Added a WGPU multiresolution document texture whose base level is the exact full-resolution composite and whose lower levels are regenerated by a custom shader after dirty-region updates.
- Downsampling explicitly decodes sRGB to linear light, performs area/box averaging, then re-encodes to sRGB, preventing high-frequency graphite/paper structure from aliasing into bright speckles at Fit and zoomed-out views.
- Registered the full mip chain as a native egui-wgpu texture with nearest magnification, linear minification, and trilinear mip-level selection.
- Kept a CPU/egui fallback path if GPU pyramid initialization/update ever fails; the simulation and export image remain untouched.
- Preserved the v0.6.3 stylus-orientation plumbing and manual/auto-azimuth fallbacks.

## v0.6.3

- Added tilt-aware input plumbing so live stylus orientation packets can feed the stroke model when the backend exposes them.
- Added automatic pencil azimuth following from stroke direction as the fallback when no live device orientation is available.
- Kept the manual Tilt/Azimuth sliders as explicit fallbacks for mouse and non-orientation stylus backends.

## v0.6.2

- Fixed the display-reconstruction stage for zoomed-out / Fit views by enabling mipmapped document minification.
- Reduced monitor-dependent bright graphite speckle aliasing without baking marks into the paper texture or weakening the underlying graphite simulation.

## v0.6.1

- Refined broad-stroke micro-contact behavior to eliminate conspicuous white speckles inside large pencil marks.
- Added a coherent low-level graphite support floor for near-miss micro-contacts in broad strokes, preserving texture without creating bright holes.
- Kept paper-colored gaps for strong misses so broad shading still retains real graphite variation instead of reverting to a solid digital brush.

## v0.6.0

- Replaced broad pencil deposition's nearly-solid contact interior with a **two-stage contact model**: coarse tip/paper contact followed by sparse unresolved micro-asperity selection.
- Broad and tilted strokes now leave real paper-colored gaps in the deposited material rather than simulating texture as opacity variation inside a digital brush mask.
- Added a persistent, stroke-seeded **pencil-face ridge field**. Virtual raised graphite ridges move with the stroke and create coherent longitudinal graphite streaks instead of re-randomized dab noise.
- Stable paper-coordinate micro-asperity statistics combine fine and clustered contact populations; new strokes vary the pencil-face realization so repeated passes can gradually fill different gaps.
- Pressure increases the population of contacting asperities instead of simply filling the whole broad footprint; heavy/burnished coverage remains a repeated-contact outcome.
- Sparse contact concentrates local normal load on the asperities that actually touch, preserving dark crisp particles/streaks inside a macroscopically diffuse mark.
- Pencil drawing continues to write visible graphite/color mass **only to the active transparent drawing layer**. Paper RGB/albedo is never modified by a stroke; only shared physical paper geometry/abrasion may evolve.
- Added regression tests for broad-contact sparsity, pressure-dependent micro-contact population, dense narrow-tip behavior, stroke-to-stroke contact variation, and the invariant that pencil strokes never bake visible marks into the paper texture.

## v0.5.0

- Added ordered multi-layer drawing support with independent deposited material per layer.
- Added a top-bar **Layers** toggle and a layer panel with add/delete/select/show-hide/reorder controls.
- Kept paper geometry and deformation shared across layers while pencil/smudge/eraser operate only on the active layer.
- Added sparse 32×32 storage for inactive layer material to avoid full-resolution channel allocation for every layer.
- Made stroke undo/redo layer-aware by storing stable layer IDs in history entries.
- Added an sRGB pencil color picker and Graphite/Warm graphite/Sepia/Sanguine/Indigo presets.
- Added mass-weighted RGB deposit channels so multiple pencil colors can coexist on one layer and mix through overlap/smudging.
- Extended the smudger reservoir and eraser bookkeeping to transport/remove pencil color with material.
- Renderer now composites visible drawing layers in stack order above the shared paper.

## v0.4.2

- Added built-in paper textures: White, Recycled, and Ivory.
- Switched the default new-document paper appearance to White.
- Added custom paper texture loading from PNG, JPEG, and BMP images.
- Paper texture changes can now be applied to the current document without recreating the page.

## v0.4.1

- Added global **Ctrl+Z** undo and **Ctrl+Y** redo shortcuts wired to the existing sparse document history.
- Shortcuts finish an active stroke first, so undo always removes the currently visible stroke as one transaction.
- Shortcuts do not steal undo/redo while a numeric/text editor has keyboard focus.
- Replaced the pencil footprint's whole-radius `(1-r²)^n` fade with a mostly loaded interior and an approximately one-document-pixel antialiased boundary.
- Large pencil sizes therefore preserve crisp graphite/paper texture instead of becoming progressively airbrush-like.
- Reduced the center-to-edge normal-load variation to a gentle bias; paper tooth and material state now provide most of the visible internal texture.
- Added midpoint quadratic Bézier stroke interpolation with continuous tangents, removing polygonal/square-ish corners from circles and curved strokes.
- Increased curved-path resampling density and capped dab spacing at 4 px so large pencils do not expose individual stamps on tight curves.
- Added regression tests for curved interpolation and large-footprint load sharpness.

## v0.4.0

- Added a **Smudge** tool implemented as simulated material transport rather than pixel blur.
- Added a persistent blending-stump reservoir carrying graphite, clay, wax and deposit orientation between smudge strokes.
- Smudging preferentially picks up loose material; compacted graphite is much less mobile.
- A moving stump redeposits carried material preferentially on its trailing contact region and picks up material preferentially on its leading region, creating directional drag/carry behavior.
- Paper valleys/fibers influence smudged-material recapture, and repeated rubbing mildly compacts material left on the sheet.
- Added **Smudge size (px)**, **Transfer**, live stump-load diagnostics and **Clean smudger** controls.
- Undo/redo, New and Clear reset the external smudger reservoir so document history cannot leave an inconsistent hidden tool load.
- Added true **16-bit/channel RGB PSD** export alongside 8-bit PSD.
- 16-bit PSD samples are rendered directly from the floating-point optical model instead of expanding the 8-bit display texture.
- Added a top-bar **PSD 8-bit / PSD 16-bit** selector; 16-bit is the default.
- Kept PSD output as a standards-compatible merged RGB image with document DPI metadata.
- Added tests for smudge material conservation, smudger pressure geometry, PSD bit depth and raw 16-bit sample byte order.

## v0.3.4

- Renamed the top-bar **Export PNG** command to **Save As...**.
- Added extension-driven export for PNG, BMP, JPEG (`.jpg`/`.jpeg`) and PSD.
- Enabled the `image` crate JPEG and BMP encoders in addition to PNG.
- JPEG export composites the opaque rendered paper to RGB and uses quality 95.
- Added a small native PSD v1 writer for an 8-bit RGB merged image, including the document DPI via Photoshop ResolutionInfo metadata.
- Kept file encoding isolated from the graphite/material simulation in `src/export.rs`.
- Added regression tests for format detection and baseline PSD structure.

## v0.3.3

- Replaced the millimeter pencil-size control with an intuitive **Pencil size (px)** control ranging from 0.5–64 document pixels.
- Pixel size is a uniform footprint scale: pressure response, tilt elongation, worn-tip anisotropy, material formulation and cursor geometry remain proportional.
- Made stroke dab spacing scale with the selected pixel size so large pencils do not become artificially darker from oversampling.
- Normalized deferred tip-wear work against pencil size so resizing the pencil does not by itself accelerate flattening.
- Changed document-texture filtering to nearest-neighbor magnification with linear minification, keeping graphite edges/native paper texels crisp when zooming in while preserving smooth Fit/zoomed-out display.
- Added a regression test for pixel-size footprint scaling.
- Kept zoom sharpness automatic; no separate sharpness control was added.

## v0.3.2

- Pressure now changes pencil contact-patch size as well as contact load/material transfer; medium mouse pressure stays near the nominal point size while light/heavy pen pressure produces narrower/wider marks.
- High-pressure contact uses a slightly flatter load profile so the wider footprint is visibly marked instead of appearing only as a faint edge halo.
- True zero device pressure now produces no mark instead of being forced to a non-zero minimum inside the stroke engine.
- Deferred pencil-tip wear until stroke end, preventing a constant-pressure mouse drag from visibly widening/softening while it is still being drawn.
- Reduced provisional tip-wear accumulation by another 20× and reduced the amount of isotropic diameter growth caused by flatness.
- Rebalanced worn-tip anisotropy so pressure and tilt dominate short-term width while long-term wear still preserves a directional flat.
- Made the on-canvas pencil cursor preview respond to the active pressure value.
- Added regression tests for pressure-dependent footprint size and stable short-stroke tip geometry.

## v0.3.1

- Added native horizontal and vertical egui scrollbars to the drawing workspace; each bar appears only when the zoomed paper is larger than the corresponding viewport axis.
- Added right-mouse-button drag to move/pan the paper.
- Preserved middle-drag and Space+left-drag panning.
- Added free sheet translation while the paper fits in the viewport, without unnecessarily showing scrollbars.
- Separated physical paper-size presets (A4/A5/Letter) from DPI and added an editable 36–600 DPI New-document field.
- Added a pre-allocation 12 MP document safety limit so extreme DPI values are rejected with a status message instead of risking runaway surface-memory allocation.
- Fit now recenters free pan and resets native scroll offsets.
- Moved scroll/zoom/free-pan layout policy into `core/viewport.rs` and added regression tests for overflow behavior.

## v0.3.0

- Added live device pressure through egui touch/pen `force` packets, with automatic mouse-pressure fallback.
- Added pressure packet detection/status in the tool panel.
- Added a tiny pressure dead-zone and mild temporal smoothing.
- Pressure is now stored per `StrokePoint` and interpolated through stroke segments.
- Fixed the drawing area height by using egui's full-available-height `horizontal_top` layout; the sheet/workspace now extends to the bottom of the window.
- Replaced binary per-pixel paper contact with a homogenized sub-cell asperity model, preserving tooth modulation without fragmenting normal pencil lines.
- Increased stroke resampling density to remove visible dab-spacing artifacts.
- Slightly increased provisional transfer scaling after the contact-model change.
- Reduced the provisional pencil-tip wear rate by 10× so a short sketch does not immediately produce an almost fully flattened point.
- Added contact/continuity/pressure normalization regression tests.

## v0.2.1

- Replaced the PowerShell Windows build helper with `build.bat`.
- Added `build.sh` for Linux, macOS, Git Bash, MSYS2, and similar Bash environments.
- Both build scripts validate the project with `cargo check` before producing a release build.
- Build scripts resolve the project directory themselves, so they can be launched from another working directory.

## v0.1.0

Initial modular prototype.

- Finite physical paper documents with pan/zoom.
- Multi-scale paper tooth/fiber generation.
- Grade-aware graphite material deposition (4H through 8B).
- Separate graphite mass, optical-darkness, sheen, and compaction channels.
- Tilt/azimuth contact footprint.
- Physical-state eraser.
- Sparse stroke history.
- Dirty-region rasterization and partial GPU texture upload.
- PNG export.
- Renderer and input boundaries prepared for GPU compute and Windows Ink work.
