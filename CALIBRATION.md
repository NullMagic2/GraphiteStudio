# Graphite Studio v0.12.1 calibration harness

The calibration harness is deliberately **not** a 900-case Cartesian product. Empirical calibration should isolate a small number of physical relationships; exhaustive combinations belong in automated regression testing.

Run:

```bash
cargo run --bin calibrate_graphite
```

By default the harness writes `graphite_calibration_swatches.png`, `graphite_calibration_metrics.csv`, and `graphite_calibration_metrics.json` under `calibration-output/`.

## Current empirical matrix: 69 unique cases

The matrix uses a standard 2.0 mm core for most cases and combines four focused experiments:

1. **Pressure × grade** — HB/2B/4B at five pressures (`0.10, 0.25, 0.48, 0.70, 1.00`), near-upright and broad-side.
2. **Tilt mapping** — HB at five tilts (`8°, 25°, 45°, 65°, 78°`) and representative light/default/hard pressures.
3. **Buildup** — HB/2B/4B at three pressures and `1, 3, 6` passes; duplicates with the pressure family are removed.
4. **Physical core check** — HB at `1.5, 2.0, 3.8 mm`, two tilts and two pressures, again deduplicated.

The resulting set contains **69 unique cases**.

## Preflight invariants

Before rendering the matrix the harness verifies that ordinary HB contact remains visible for a 1.5–4.0 mm physical core, including both near-upright and tilted contact. This is meant to catch geometry/transfer regressions before constants are fitted.

## Metrics

Each case records mean darkness, occupied area, edge roughness, tone variance, contact width, taper length, and final tip wear. These metrics should be fitted against controlled real swatches rather than tuned to screenshots by eye.

A separate exhaustive validation sweep can be added later and may contain hundreds or thousands of synthetic combinations because those require no physical swatch-making effort.
