# Graphite Studio compatibility patch

Upstream: eframe 0.35.0, https://github.com/emilk/egui/tree/0.35.0/crates/eframe
License: MIT OR Apache-2.0, as declared in Cargo.toml. Copyright the egui contributors.

One change in src/native/glow_integration.rs: if the default desktop context fails,
try OpenGL 2.1 compatibility before the upstream GLES fallback. The default glutin
request is OpenGL 3.3, while the reference Intel HD 2000 supports desktop OpenGL
3.1 and does not provide Vulkan. egui_glow supports OpenGL 2.0 or later.

All window lifecycle, input, texture upload, shader and draw code remains upstream.
