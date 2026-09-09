# Graphite Studio

**A native drawing app for textured pencils, built in Rust.**

Graphite Studio models how a pencil contacts paper, deposits pigment, wears down and blends with existing marks. Pressure and tilt shape the stroke; paper tooth and the contacting pencil surface create its texture. Choose graphite tones or your own colors, then work with layers and editable stroke paths.

**Current version: v0.24.15** · [Release history](RELEASE_HISTORY.md) · [MIT license](LICENSE)

## Features

- **Expressive pencils:** pressure-sensitive width and tone, tilt shading, adjustable sharpness, gradual tip wear, line smoothing. Holding the pencil still preserves the freehand stroke. Core diameter reaches 200 px.
- **Custom pencil gallery:** import sampled ABR tips, organize folder collections and save independent settings for each pencil. Imported shapes use the graphite material renderer and live in separate detected files in `imported_brushes`.
- **Liquify:** Push, Twirl Left/Right, Pinch, Expand, Crystals, Edge and Reconstruct. Size, pressure, distortion, momentum, Adjust/Amount and Reset controls; GPU deformation shaders and background multicore processing.
- **Drawing tools:** Pencil, Smudge, Tissue, vinyl and kneaded erasers with diameters up to 200 px, textured geometric shapes, free selection and individual stroke selection.
- **Editable artwork:** move, resize and rotate retained stroke paths; confirm or cancel transforms and undo changes. Layers support drag ordering, visibility, opacity, Merge down and blend modes, with Multiply as the default.
- **Beyond the page:** strokes and moved objects can extend past every page edge. The working area expands without clamping the pointer or shifting the page. The original page outline stays visible, layers and undo remain intact, and PNG/JPEG/BMP exports include the entire expanded artwork area. Editable Graphite/PSD saves also retain the original page bounds.
- **Flexible workspace:** multiple drawing tabs, resizable and collapsible panels, custom paper dimensions, DPI, textures and color, portrait/landscape orientation, fit-to-screen and fullscreen drawing.
- **Pen and touch:** selectable Windows Ink and Wintab input, pressure calibration, and canvas pinch, pan and rotation when the device or bridge forwards those gestures.
- **GPU acceleration:** Vulkan or DirectX 12 compute for eligible pencil and eraser strokes, with bounded buffers and CPU fallback. The display preserves texture with antialiasing and linear-light downsampling.

## Run on Windows

Extract a packaged build and run **Graphite Studio.exe** from a writable folder. It is a portable application with embedded icons and no console window.

1. Open **Paper settings…** to configure or create a drawing.
2. Choose a pencil or import sampled tips through **Pencil gallery…**.
3. Draw with a pen, or adjust **Mouse pressure fallback** when using a mouse.
4. Use **File > Save project as…** to choose a project or image format.

Under **Options**, choose an acceleration mode and input backend. **General** selects hardware automatically; **AMD Radeon 7900 XTX** prefers a discrete AMD GPU and lower display latency; **Intel HD Graphics** provides a lighter OpenGL compatibility path for older hardware. Switching the graphics backend may require restarting the application.

When updating, keep these files beside the executable if present:

| File | Contents |
| --- | --- |
| `pencil.settings` | Settings for each pencil, including width response and line smoothing |
| `Pencils.gallery` | Native pencil presets, collection metadata and gallery order; no imported masks |
| `imported_brushes/` | Individual imported `.pencil` resources with lossless sampled tips and collection labels |
| `Graphite-Studio.settings.json` | Acceleration preference, recent-file limit and recent-file paths |

File and folder pickers open as owned dialogs over the application, following its current monitor on Windows.

ABR import converts each sampled tip into a separate Graphite Studio `.pencil` file in `imported_brushes`, beside the executable. The original ABR is left untouched. The gallery scans these files at startup and whenever it is reopened; deleted or missing brush files are not reconstructed from cached data. To share imported pencils, copy their `.pencil` files into the recipient's `imported_brushes` folder. Copying the executable, `pencil.settings` or the new `Pencils.gallery` alone does not import any third-party brush. Use **Import ABR…** to convert an ABR; raw ABRs dropped in this folder are not auto-imported.

Existing legacy `Pencils.gallery` files beside the app migrate their embedded tips into this folder once, retaining pencil IDs and individual settings. Keep the gallery and settings files together when updating. Removing a collection in the gallery removes its managed `.pencil` files, while preserving source ABRs and existing artwork. Drawings still embed the resources needed to replay their strokes; opening a drawing does not add its brushes to the gallery.

Save drawings before closing the app; drawing tabs are not automatically restored on restart.

## Recent files

**File > Recent files** reopens recently opened or successfully saved drawings and images, newest first. Reopening a file moves it to the top without duplicates. Hover over an entry to see its full path. A missing or unreadable file reports the same error as Open and leaves your current drawing intact.

Under **Options > Recent files**, set **Maximum files** from **0 to 10** (default **10**). Lowering the limit immediately removes the oldest excess entries. **0 disables the menu, clears the saved list and stops recording files.** Re-enabling it starts a new list. The limit and list persist in `Graphite-Studio.settings.json` beside the executable; keep that file when updating. No drawing files are deleted by changing this setting.

## Open drawings and images

Use **File > Open drawing or image…** or **Ctrl+O** for `.graphite`, `.psd`, `.png`, `.jpg`, `.jpeg` and `.bmp`. Each file opens in a new tab; external images fit the canvas automatically. PNG alpha, indexed PNG, 16-bit PNG and JPEG orientation are handled. PNG, JPEG and BMP contain a single image layer.

PSD image layers keep their names, order, visibility, opacity and supported blend modes (Normal, Multiply, Darken, Screen and Lighten). 8-, 16- and 32-bit RGB layer data supports raw, PackBits, ZIP and ZIP prediction. Grayscale, indexed, CMYK and Lab data can be converted to the drawing's RGB colors. CMYK/Lab conversion is not ICC-managed; HDR values beyond the display range are clipped. Import never substitutes a flattened preview for an unsupported layered document.

**Photoshop vectors:** solid-color shape layers retain their cubic Bézier anchors and handles, closed/open paths, holes and path operations. Solid centered strokes keep width, color, cap and join. Select the layer and use **V** to select, move, resize or rotate its shape; transforms and whole-document rotations/mirrors retain the curves. Saved Photoshop paths appear on hidden `Path / …` layers; enable one to see and select it. Native saves retain vector geometry, and PSD saves include standard Photoshop paths and hidden vector shape alternatives.

Pass-through folders at full opacity become separately editable child layers named `Folder / Layer`. Bitmap masks and clipping against opaque bases are applied to each individual image layer's alpha; folder hierarchy and separate mask controls are not retained. Off-canvas image pixels are cropped to the canvas. Photoshop adjustment layers, effects, isolated or translucent groups, unsupported blend modes, gradients/patterns, smart objects, dashed/inside/outside vector outlines, and bitmap-masked/clipped vector shapes, and clipping groups with translucent bases are not fully supported. Unsupported features produce an explanation instead of silently flattening. Text layers open as their separate cached image layers, not editable text. Native Graphite files preserve all their own supported state.

Graphite imposes no fixed source-file-size, canvas-pixel, combined-layer-pixel, open-tab-pixel, or layer-count quota. Available memory/storage and each file format's representable sizes still apply. PSD version 1 uses 30,000-pixel sides, signed 16-bit layer counts, and 32-bit section lengths; save as a Graphite project when a drawing exceeds PSD's format capacity. Undo history starts fresh after opening. Save important drawings before replacing an older executable with this version; retain your gallery and settings files.

## Save formats

| Format | What it preserves |
| --- | --- |
| `.graphite` | Material state, layers, editable paths, paper and tool settings, and embedded brush/paper resources |
| `.psd` — 8 or 16 bit | Visible artwork layers, Photoshop-native paths and vector shape alternatives, plus Graphite Studio state for reopening here |
| `.png`, `.bmp` | Flattened, lossless image copies |
| `.jpg` / `.jpeg` | Flattened, compressed image copies |

The textured artwork in a PSD remains on pixel layers. Photoshop-native vector alternatives are initially hidden and represent clean geometry; editing a Photoshop path does not repaint the graphite texture. Graphite Studio can open external PSDs using their actual layers and supported vector shapes. Unchanged Graphite-created PSDs restore their exact embedded material state; externally changed files import their current PSD content. Imported originals use Save As so they are not overwritten by a normal Save. Undo history starts fresh when a project is reopened.

## Controls

| Action | Shortcut or gesture |
| --- | --- |
| Pencil / Eraser | **P** or **B** / **E** |
| Select an individual stroke | **V** |
| Transform / rotate selection | **Ctrl+T** / **Ctrl+R** |
| Confirm / cancel transform | **Enter** / **Esc**, or the checkmark / cross |
| Undo / redo | **Ctrl+Z** / **Ctrl+Y** |
| Save / save as / open | **Ctrl+S** / **Ctrl+Shift+S** / **Ctrl+O** |
| Zoom around the cursor | Mouse wheel; two-finger pinch when forwarded |
| Zoom the drawing around the viewport center | **Ctrl++**, **Ctrl+=**, **Ctrl+-**; panels and menus retain their size |
| Quick tool size | Press **Shift**, then adjust the slider; click outside to close it |
| Pan paper | Right drag, middle drag, **Space + left drag**, or two-finger drag |
| Rotate the paper view | **R** and drag, or two-finger twist |
| Snap paper rotation to cardinal directions | Hold **Shift** while rotating |
| Snap a geometric line to 15° increments | Enable **Snap shape**, or hold **Shift**, with the Line tool |
| Exit fullscreen | **Esc** |

Cancel, Esc, or switching tools during a transform restores the original artwork and clears the selection. Use the checkmark or Enter to keep a transform before switching tools.

Use the lasso icon for a free selection. The green checkmark beside the selection confirms and deselects; the red X cancels and leaves the selection tool. Switching tools also cancels and clears the selection. Enable **Keep aspect ratio** in the transform controls to resize proportionally without Shift. Holding Shift also constrains supported shape proportions. Text fields retain normal typing behavior.

Tissue supports sizes up to **200 px** and a **Random graphite** control for varying the density of its stain while retaining the chosen pencil color. At 0%, it keeps uniform loading. Eraser diameter also reaches **200 px** at any document DPI.

**Merge down** combines the active layer with the visible layer below. To merge several layers at once, **Ctrl-click** individual rows or **Shift-click** a range, then choose **Merge selected**. For pen/touch, enable **Select multiple** and tap the rows. One Undo restores all original layers and editable paths. Non-adjacent selections merge at the highest selected position, leaving intervening layers separate; this can change their blending order. Matching Multiply, Normal and Screen groups keep their blend behavior. Other combinations bake their current combined appearance into Normal material. Paper stays separate, and merged graphite remains erasable. Graphite buildup has 15% more capacity for deeper blacks after repeated passes with a dark pencil color.

Pencil **Opacity** (0–100%) replaces Material flow. It controls pigment transfer while retaining pressure, tip shape and paper grain; 0% makes no mark or paper deformation. New default pencils transfer 15% more pigment than before, producing approximately 15% darker light strokes. Existing saved pencil settings and stroke replay values are retained. Opacity is saved per pencil in `pencil.settings` using its compatible internal `flow` value. Pressure feel calibrates the pen's force curve, while Width response controls thin-to-thick variation; these controls serve different purposes.

Drag the dotted handle to reposition the fullscreen toolbar. Its position stays in place when leaving and reentering fullscreen during the session, and is kept within the visible drawing area.

The fullscreen toolbar gives quick access to Pencil, Eraser, Transform, Tissue and Smudge with 48 px touch targets. It is visible only in fullscreen and remains accessible when zoom leaves no free margin. Use its small arrow to hide or reopen it, and the separate **×** button at the top right to leave fullscreen. Its **Size** button opens the same slider without a keyboard. Press Shift for size in either display mode, then release it and adjust the slider. The compact popup contains only the slider and size value. Click outside or press Escape to close it; a dismissal click on the paper does not draw. It adjusts pencil, eraser, smudge and tissue sizes in pixels, or scales an idle transform proportionally in percent. Text entry, active drawing/transform drags, shape snapping and paper rotation take priority over opening the size popup.

**Options → Keyboard shortcuts…** lets you record new key combinations, clear assignments and restore defaults. Duplicate assignments are rejected with the conflicting command's name. Click **Save shortcuts** to apply and persist changes in `shortcuts.settings` beside the executable; Cancel leaves current shortcuts intact. The dialog also lets you choose Shift, Ctrl, Alt or Disabled for the held size control. Keyboard commands include tools, selection, saving, Undo/Redo, drawing zoom, panning and window recovery; Fit to screen and Toggle fullscreen can also be assigned. Mouse/touch gestures and Shift snapping are unchanged. The table above lists the defaults.

**Mirror horizontal** and **Mirror vertical** in the transform section preview a flip of the selected path or fragment. Confirm with the checkmark, or cancel with X. Without a selection, they mirror the whole drawing, including all layers and paper, without resampling or changing zoom. **Copy layer**, beside Merge, duplicates the active layer above the original, preserving its editable paths, appearance settings and material; edits to the copy are independent. Both actions support Undo/Redo.

Paper settings includes **Texture opacity** (0–100%). At 0% the visible paper is the selected flat color; at 100% it retains the complete texture and surface shading. Pigment deposits, paper tooth and drawing behavior are unaffected. The value stays with each drawing, is preserved in native and PSD projects, appears in exports, and carries into drawings created from Paper settings. Portrait and landscape choices use icons with tooltips.

## Build from source

Windows is the primary tested platform. Install **Rust 1.92 or newer** with the MSVC toolchain and the Microsoft C++ build tools, including a Windows SDK.

From the repository root:

```powershell
cargo build --release --locked --bin graphite-studio
.\target\release\graphite-studio.exe
```

Use the release profile for drawing performance. It enables link-time optimization. `build.bat` also checks and builds the project; `build.sh` is supplied for Bash environments. Linux and macOS builds and tablet behavior have not been validated for this release.

The repository includes a patched `eframe` dependency for legacy OpenGL support. Keep `vendor/eframe` when copying the source; see its [patch notes](vendor/eframe/GRAPHITE-PATCH.md).

### Tests and performance checks

```powershell
cargo test --locked --lib --bin graphite-studio --tests
cargo run --release --locked --bin gpu_stroke_check -- --one --integrated --display
```

The GPU check requires a compatible hardware adapter. It exercises material rendering, compares CPU/GPU output and checks undo/redo. The `--display` timings include shading, upload and GPU mip completion, but exclude window presentation and pen/bridge latency. It writes diagnostic images two directories above the repository root.

The v0.24.5 regression suite passed **202 tests**. Hardware rendering checks covered Vulkan and DirectX 12 on Radeon hardware in v0.24.2; Intel hardware was not available for direct testing. See the [release measurements](RELEASE_HISTORY.md#v0236) for workloads and timing limits.

## Liquify

Choose the turquoise swirl directly below Transform, in the regular palette or fullscreen toolbar. The material sidebar also has a Liquify button below Transform. The controls window can be moved by its title bar.

The eight choices are **Push**, **Twirl Left**, **Twirl Right**, **Pinch**, **Expand**, **Crystals**, **Edge**, and **Reconstruct**. Push follows the stroke; the Twirl modes rotate material; Pinch and Expand contract or inflate it; Crystals form uneven shards; Edge folds toward the stroke's line. Reconstruct locally restores the state from when Liquify was opened.

**Size** sets the affected diameter. **Pressure** sets maximum strength and responds to real pen pressure; mouse/finger input uses the slider's full strength. **Distortion** adds irregular motion. **Momentum** continues the effect after lifting the pen, then decays. Twirl also responds to barrel rotation when reported by the device. **Adjust > Amount** reduces the whole session's deformation, and **Reset** restores the session's starting artwork without leaving Liquify. **Apply/Enter** keeps it; **Cancel/Escape** restores it.

Liquify changes active-layer pixel material, respects a free selection and works beyond the page. Other layers stay separate. Applying bakes that layer's retained paths into pixel material; Undo restores its exact original pixels and paths. Editable Graphite and PSD saves retain the resulting material and layers.

All mode mappings run through a reusable WGSL compute pipeline on supported Vulkan/DirectX 12 devices. Queued deformation steps share one antialiased image resampling pass. Large brushes publish smaller batches for earlier feedback; redundant straight Push samples are combined without merging corners or pressure changes. Stationary Push does no unnecessary work. Momentum uses continuous decay and loses its throw when the pen pauses before lifting. Jobs and GPU readback run off the interface thread; deformation-field composition, original-material sampling and Amount changes use multiple CPU workers. Small jobs, disabled acceleration and unsupported/failed GPU work use the CPU fallback. Adaptive footprint filtering smooths compressed detail along its principal directions without applying a global blur. Sampling the original material through a deformation field avoids repeated image-blurring passes. Explicit save or layer/tool changes finish pending jobs before switching their document.

Behavior reference: [Procreate's official Liquify handbook](https://help.procreate.com/procreate/handbook/adjustments/adjustments-liquify). This is Graphite's own implementation of the documented effects, not Procreate's proprietary rendering engine.

## Compatibility and scope

- Wacom, XP-Pen and Apple Pencil through EasyCanvas depend on the installed driver or bridge. Tilt and barrel rotation are used only when the hardware and input backend expose them.
- ABR import reads supported sampled-tip data, not Photoshop's complete brush dynamics or procedural brush engine. Third-party brush packs are not bundled.
- Editable paths regenerate textured material on an expandable pixel canvas. Imported tips retain their sampled resolution; this is not a pure vector/SVG editor.
- The material model is an artistic, physically inspired approximation. Grade recipes and wear rates are not measured specifications for a particular pencil manufacturer.
- Large documents and brushes require more memory and processing. There is no fixed document-pixel quota; GPU acceleration falls back to CPU when unavailable or outside its working-memory budget.

## Documentation and contributing

- [Release history](RELEASE_HISTORY.md): every recorded version, including detailed notes and measurements.
- [Architecture](ARCHITECTURE.md): application modules and data flow.
- [Material model](MODEL.md): paper, contact and pigment simulation.
- [Calibration](CALIBRATION.md): controlled swatches and metrics.
- [Research notes](RESEARCH_NOTES.md): sources and modeling background.

Bug reports should include the application version, GPU and acceleration mode, pen/tablet and input backend, and steps to reproduce. For rendering or performance issues, include brush size, paper dimensions/DPI and an example image or project when possible.

Contributions are welcome. Keep simulation changes separate from display-only effects, preserve saved-project compatibility, and run the relevant tests before submitting changes.

## License

Graphite Studio is released under the [MIT license](LICENSE). Dependencies and imported assets retain their respective licenses; the bundled `eframe` license is [included here](vendor/eframe/LICENSE-MIT).
