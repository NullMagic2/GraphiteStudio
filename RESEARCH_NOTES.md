# Graphite Studio research notes — v0.8

These notes document why the v0.8 micro-surface model changed. They are not a claim that one set of constants describes every drawing paper or pencil manufacturer.

## Paper scale

**Study of Hydraulic Properties of Uncoated Paper: Image Analysis and Pore-Scale Modeling** (Transport in Porous Media, 2017; DOI 10.1007/s11242-017-0909-x) reports, for its uncoated filler-free paper sample:

- mean cellulose-fiber diameter: about **20 µm**;
- mean pore size: about **12 µm**;
- surface roughness: about **35 µm**;
- micro-CT voxel size used in the study: 0.9 µm.

This is the main scale reference behind Graphite Studio's ~12 µm sparse micro-grid. It is a convenient modeling pitch, not a universal paper-pore constant.

Regular paper is commonly described as a network of cellulose fibers tens of micrometers across. For example, *Transparent paper: fabrications, properties, and device applications* (Energy & Environmental Science, 2014; DOI 10.1039/C3EE43024C) gives a regular-paper fiber diameter range of roughly **20–50 µm**.

## Graphite transfer to paper

**Pencil Drawn Strain Gauges and Chemiresistors on Paper** (Scientific Reports 4, 3812, 2014; DOI 10.1038/srep03812) uses SEM to show fine graphite material rubbed from the pencil lead and adhering to paper fibers, forming a percolated graphite-particle network.

**Tribological characteristics of graded pencil cores on paper** (Wear 197, 1996; DOI 10.1016/0043-1648(96)06952-9) reports significant differences in core wear and paper infilling and describes wear particles becoming embedded/agglomerated among tangled paper fibers. This supports treating pencil marking primarily as wear + transfer/capture rather than an opacity brush.

Repeated heavy drawing can eventually cover much of the visible fibrous topology. Electrode-oriented SEM studies report continuous or near-continuous graphite layers after repeated pencil tracing. This is why the v0.8 model allows previous compacted micro-deposit to bridge later contacts instead of defining permanent empty pores.

## Modeling implications

v0.8 therefore uses:

1. a coarse document-resolution surface for broad paper relief/deformation;
2. a sparse ~12 µm procedural cellulose-fiber/pore surface for contact/capture;
3. sparse per-layer micro-deposit state at the same physical scale;
4. document-resolution material channels as an aggregate optical state;
5. a rounded-conical/side-contact pencil approximation instead of a scaled brush ellipse.

The fiber lengths, surface calendering coefficients, contact thresholds, and graphite transfer coefficients remain provisional. Controlled swatches on known drawing papers under measured normal forces would be required for empirical calibration.

## Stroke taper

The reconstructed pen-down/pen-up intervals in v0.8 are a kinematic correction, not a measured pencil-material law. Digital pen stacks frequently terminate contact without a perfect final zero-force sample. Physically, normal force must still pass through zero during lift-off, so Graphite Studio reconstructs a short release interval whose distance depends on recent tangential motion. A near-stationary lift remains rounded rather than being forced into a needle taper.

## v0.9 implementation correction

The physical scale discussion above remains useful, but v0.8's literal sparse ~12 µm sampling was the wrong *interactive representation* for a 120–300 DPI painting application. At those output resolutions, many physical fibers/pore regions contribute to one document pixel. Sampling nine micro-points per raster cell during every dab incurred substantial CPU/cache overhead and then immediately averaged the result back into one coarse deposit value.

v0.9 therefore treats fiber/pore structure statistically in the real-time solver. The document stores continuous contact-support and edge-grain fields derived from the paper's existing height/fiber geometry. These represent unresolved sub-pixel coverage, while the graphite layer stores material mass. The result preserves the physical interpretation—partial fiber contact, tooth-dependent capture and porous boundaries—without making microscopic pores literal white raster holes.
