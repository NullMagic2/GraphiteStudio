# Opening regression fixtures

Generated independently of Graphite with psd-tools 1.18.0 and Pillow 12.3.0. Install these in a disposable Python environment, then run `generate_images.py` and `generate_vectors.py` to reproduce the data. The manifests record each layer pixel or cubic handle as decoded by psd-tools.

The layered PSD previews are deliberate placeholders: importing only the composite cannot pass the tests. Files cover all 8/16/32-bit and raw/PackBits/ZIP/prediction combinations, Unicode/legacy names, layer offsets, alpha, opacity, hidden layers, Multiply, pass-through groups and user bitmap masks. Vector fixtures include cubic handles, union/subtraction, a hole, an open centered stroke and a saved path. The unsupported-style fixture must return an error without flattening.

Raster variants include indexed and 16-bit PNG, PNG transparency, BMP, baseline/progressive JPEG, both JPEG suffixes and EXIF rotation. Run `cargo test --test open` plus the application tests.
