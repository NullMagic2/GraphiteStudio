use graphite_studio::{core::{brush::BrushTip, document::{CanvasSpec,Document}, history::EditTransaction,
    paper::PaperPreset,pencil::{PencilTipState,ToolSettings},stroke::{StrokeEngine,StrokePoint}},
    render::{DocumentRenderer,RasterRenderer}};

fn draw(flow:f32, imported:bool)->(f64,f64,usize) {
    let spec=CanvasSpec::from_physical("Opacity",50.8,25.4,120.);
    let mut doc=Document::new(spec,PaperPreset::DrawingMedium,"White",vec![[1.;3];240*120]);
    let before=RasterRenderer::default().rgba8(&doc);
    let settings=ToolSettings {flow,pencil_texture:imported.then(||std::sync::Arc::new(BrushTip {
        name:"Texture".into(),width:3,height:3,mask:vec![60,160,20,220,255,110,10,140,80]
    })),..Default::default()};
    let mut engine=StrokeEngine::default();let mut tip=PencilTipState::fresh_for_formulation(2.,settings.grade.formulation());
    let start=StrokePoint{x:25.,y:60.,pressure:0.24,tilt_deg:8.,azimuth_deg:20.,rotation_deg:None};
    let mut tx=EditTransaction::default();
    engine.begin_pencil_stroke(start);
    engine.apply_segment(&mut doc,&settings,&mut tip,start,StrokePoint{x:215.,..start},&mut tx);
    if flow==0. {assert!(tx.is_empty(),"Zero opacity must leave no marks or paper deformation");}
    let after=RasterRenderer::default().rgba8(&doc);
    let darkening=before.chunks_exact(4).zip(after.chunks_exact(4))
        .map(|(a,b)|(a[0] as f64-b[0] as f64)/255.).sum();
    let mass=doc.surface.graphite_mass.iter().map(|&v|v as f64).sum();
    let pixels=doc.surface.graphite_mass.iter().filter(|&&v|v>0.).count();
    (mass,darkening,pixels)
}

#[test]
fn opacity_is_monotonic_and_default_pencils_are_darker_without_a_fatter_contact() {
    for imported in [false,true] {
        let zero=draw(0.,imported);let low=draw(0.5,imported);let old=draw(1.,imported);
        let new=draw(ToolSettings::default().flow,imported);let high=draw(2.,imported);
        assert_eq!(zero,(0.,0.,0));
        assert!(low.0<old.0 && old.0<new.0 && new.0<high.0);
        assert!(low.1<old.1 && old.1<new.1 && new.1<high.1);
        let ratio=new.1/old.1;
        eprintln!("Imported={imported}: default darkness ratio {ratio:.4}, mass ratio {:.4}",new.0/old.0);
        assert!((1.10..1.20).contains(&ratio),"Default marks should be approximately 15% darker: {ratio}");
        assert_eq!(old.2,new.2,"Opacity must not widen the physical contact");
    }
}
