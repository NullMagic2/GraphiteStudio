use graphite_studio::{core::{brush::BrushTip,document::{CanvasSpec,Document},history::EditTransaction,
    paper::PaperPreset,pencil::{PencilTipState,ToolSettings},stroke::{StrokeEngine,StrokePoint}},render::{DocumentRenderer,RasterRenderer},project::{self,ProjectRef}};
use std::sync::Arc;

fn paper()->Document {
    let spec=CanvasSpec::from_physical("Shape check",67.73333,67.73333,120.);
    let n=spec.pixel_count();Document::new(spec,PaperPreset::DrawingMedium,"White",vec![[1.;3];n])
}
fn ring()->Arc<BrushTip> {
    Arc::new(BrushTip{name:"Hollow square".into(),width:64,height:64,
        mask:(0..4096).map(|i|if (22..42).contains(&(i%64)) && (22..42).contains(&(i/64)){0}else{255}).collect()})
}
#[test]
fn imported_shape_keeps_corners_holes_color_and_native_grain() {
    let mut doc=paper();
    let settings=ToolSettings {pencil_core_diameter_mm:200.*25.4/120.,pencil_texture:Some(ring()),pencil_color_rgb:[150,45,25],particle_variation:0.24,..Default::default()};
    let mut tip=PencilTipState::fresh(settings.pencil_core_diameter_mm);
    let mut engine=StrokeEngine::default();let mut tx=EditTransaction::default();
    let p=StrokePoint{x:160.,y:160.,pressure:1.,tilt_deg:0.,azimuth_deg:0.,rotation_deg:None};
    let start=std::time::Instant::now();
    for _ in 0..5{engine.apply_dab(&mut doc,&settings,&mut tip,p,&mut tx).unwrap();}
    eprintln!("200 px sampled shape: {:.2} ms/dab",start.elapsed().as_secs_f64()*1000./5.);
    let mass=|x,y|doc.surface.graphite_mass[doc.index(x,y)];
    assert!((82..92).any(|x|(82..92).any(|y|mass(x,y)>1e-6)),"Square corners must extend beyond a round pencil contact");
    for x in 140..180{for y in 140..180{assert_eq!(mass(x,y),0.,"Transparent hole must stay empty");}}
    for x in 0..55{for y in 0..320{assert_eq!(mass(x,y),0.);}}
    let values:std::collections::HashSet<_>=(75..110).flat_map(|y|(75..110).map(move|x|(x,y))).map(|(x,y)|mass(x,y).to_bits()).collect();
    assert!(values.len()>100,"The shape must retain paper-driven graphite variation");
    let mut renderer=RasterRenderer::default();
    image::save_buffer("../../shape-v234.png",&renderer.rgba8(&doc),320,320,image::ColorType::Rgba8).unwrap();
    let path=std::env::temp_dir().join(format!("graphite-shape-{}.graphite",std::process::id()));
    project::save(&path,&ProjectRef{document:&doc,settings:&settings,tip:&tip,engine:&engine,brushes:&[],custom_paper:&None,zoom:1.,pan:[0.,0.]}).unwrap();
    let restored=project::load(&path).unwrap();std::fs::remove_file(path).unwrap();
    assert!(restored.settings.pencil_tip_shape);
    assert!((restored.tip.effective_core_diameter_px(120.)-200.).abs()<0.001);
    assert_eq!(restored.document.surface.graphite_mass,doc.surface.graphite_mass);
}

#[test]
fn core_reaches_200_pixels_at_supported_dpi_and_old_paths_keep_old_contact() {
    for dpi in [36.,72.,120.,300.,600.] {
        let mm=200.*25.4/dpi;let mut tip=PencilTipState::fresh(mm);
        assert!((tip.effective_core_diameter_px(dpi)-200.).abs()<0.001);
        tip.sync_core_diameter(mm);tip.sharpen(mm,ToolSettings::default().grade.formulation());
        assert!((tip.effective_core_diameter_px(dpi)-200.).abs()<0.001);
    }
    let settings=ToolSettings::default();let mut old=serde_json::to_value(&settings).unwrap();
    old.as_object_mut().unwrap().remove("pencil_tip_shape");
    assert!(!serde_json::from_value::<ToolSettings>(old).unwrap().pencil_tip_shape);
}
