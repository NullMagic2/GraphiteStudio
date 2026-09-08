# Graphite Studio

**A native drawing app for textured pencils, built in Rust.**

Graphite Studio models how a pencil contacts paper, deposits pigment, wears down and blends with existing marks. Pressure and tilt shape the stroke; paper tooth and the contacting pencil surface create its texture. Choose graphite tones or your own colors, then work with layers and editable stroke paths.

**Current version: v0.23.9** · [Release history](RELEASE_HISTORY.md) · [MIT license](LICENSE)

## Features

- **Expressive pencils:** pressure-sensitive width and tone, tilt shading, adjustable sharpness, gradual tip wear, line smoothing and hold-to-straighten. Core diameter reaches 200 px.
- **Custom pencil gallery:** import sampled ABR tips, organize folder collections and save independent settings for each pencil. Imported shapes use the graphite material renderer.
- **Drawing tools:** Pencil, Smudge, Tissue, vinyl and kneaded erasers with diameters up to 200 px, textured geometric shapes, free selection and individual stroke selection.
- **Editable artwork:** move, resize and rotate retained stroke paths; confirm or cancel transforms and undo changes. Layers support drag ordering, visibility, opacity and blend modes, with Multiply as the default.
- **Flexible workspace:** multiple drawing tabs, resizable and collapsible panels, custom paper dimensions, DPI, textures and color, portrait/landscape orientation, fit-to-screen and fullscreen drawing.
- **Pen and touch:** selectable Windows Ink and Wintab input, pressure calibration, and canvas pinch, pan and rotation when the device or bridge forwards those gestures.
- **GPU acceleration:** Vulkan or DirectX 12 compute for eligible pencil and eraser strokes, with bounded buffers and CPU fallback. The display preserves texture with antialiasing and linear-light downsampling.

- <img width="3836" height="2058" alt="image" src="https://github.com/user-attachments/assets/c46d7af8-cbf6-44e1-bd73-98867d2cb956" />


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
| `Pencils.gallery` | Pencil presets, collections and imported tips |
| `Graphite-Studio.settings.json` | Application acceleration preference |

File and folder pickers open as owned dialogs over the application, following its current monitor on Windows.

Save drawings before closing the app; drawing tabs are not automatically restored on restart.

## Save formats

| Format | What it preserves |
| --- | --- |
| `.graphite` | Material state, layers, editable paths, paper and tool settings, and embedded brush/paper resources |
| `.psd` — 8 or 16 bit | Visible artwork layers, Photoshop-native paths and vector shape alternatives, plus Graphite Studio state for reopening here |
| `.png`, `.bmp` | Flattened, lossless image copies |
| `.jpg` / `.jpeg` | Flattened, compressed image copies |

The textured artwork in a PSD remains on pixel layers. Photoshop-native vector alternatives are initially hidden and represent clean geometry; editing a Photoshop path does not repaint the graphite texture. Graphite Studio does not currently import arbitrary PSDs or reconcile external Photoshop edits with its stored material state. Undo history starts fresh when a project is reopened.

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
| Pan paper | Right drag, middle drag, **Space + left drag**, or two-finger drag |
| Rotate the paper view | **R** and drag, or two-finger twist |
| Snap paper rotation to cardinal directions | Hold **Shift** while rotating |
| Snap a geometric line to 15° increments | Enable **Snap shape**, or hold **Shift**, with the Line tool |
| Exit fullscreen | **Esc** |

Cancel, Esc, or switching tools during a transform restores the original artwork and clears the selection. Use the checkmark or Enter to keep a transform before switching tools.

Use the lasso icon for a free selection. The green checkmark beside the selection confirms and deselects; the red X cancels and leaves the selection tool. Switching tools also cancels and clears the selection. Enable **Keep aspect ratio** in the transform controls to resize proportionally without Shift. Holding Shift also constrains supported shape proportions. Text fields retain normal typing behavior.

Tissue supports sizes up to **200 px** and a **Random graphite** control for varying the density of its stain while retaining the chosen pencil color. At 0%, it keeps uniform loading. Eraser diameter also reaches **200 px** at any document DPI.

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

The v0.23.9 regression suite passed **176 tests**. Hardware checks covered Vulkan and DirectX 12 on Radeon hardware; Intel hardware was not available for direct testing. See the [release measurements](RELEASE_HISTORY.md#v0236) for workloads and timing limits.

## Compatibility and scope

- Wacom, XP-Pen and Apple Pencil through EasyCanvas depend on the installed driver or bridge. Tilt and barrel rotation are used only when the hardware and input backend expose them.
- ABR import reads supported sampled-tip data, not Photoshop's complete brush dynamics or procedural brush engine. Third-party brush packs are not bundled.
- Editable paths regenerate textured material on a finite pixel canvas. Imported tips retain their sampled resolution; this is not a pure vector/SVG editor.
- The material model is an artistic, physically inspired approximation. Grade recipes and wear rates are not measured specifications for a particular pencil manufacturer.
- Large documents and brushes require more memory and processing. Documents are limited to 12 megapixels; GPU acceleration falls back to CPU when unavailable or outside its working-memory budget.

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
