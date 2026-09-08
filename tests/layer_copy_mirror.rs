use graphite_studio::{core::{document::{CanvasSpec,Document},history::{History,EditTransaction},paper::PaperPreset},render::{DocumentRenderer,RasterRenderer}};

#[test]
fn copy_mirror_and_quarter_turn_history_restore_all_layers_losslessly() {
    let spec=CanvasSpec::from_physical("Mirror",20.,30.,120.);
    let mut doc=Document::new(spec.clone(),PaperPreset::DrawingMedium,"White",vec![[1.;3];spec.pixel_count()]);
    let mut history=History::default();let mut renderer=RasterRenderer::default();
    let snapshot=|d:&Document,r:&mut RasterRenderer|(d.spec.width_px,d.spec.height_px,d.layer_count(),r.rgba8(d));
    let mut states=vec![snapshot(&doc,&mut renderer)];
    let i=doc.index(11,23);let mut tx=EditTransaction::default();tx.remember(i,&doc);
    doc.surface.graphite_mass[i]=0.6;doc.surface.loose_mass[i]=0.6;
    doc.surface.color_r_mass[i]=0.07;doc.surface.color_b_mass[i]=0.22;
    history.push(tx,&doc);doc.mark_all_dirty();states.push(snapshot(&doc,&mut renderer));
    assert!(history.copy_layer(&mut doc));states.push(snapshot(&doc,&mut renderer));
    let original=doc.surface.graphite_mass.clone();
    history.mirror_document(&mut doc,true);states.push(snapshot(&doc,&mut renderer));
    assert_eq!(doc.surface.graphite_mass[doc.index(doc.spec.width_px-1-11,23)],original[i]);
    assert_eq!((doc.spec.width_px,doc.spec.height_px),(spec.width_px,spec.height_px));
    history.rotate_document(&mut doc,true);states.push(snapshot(&doc,&mut renderer));
    history.mirror_document(&mut doc,false);states.push(snapshot(&doc,&mut renderer));
    for state in states[..states.len()-1].iter().rev() {
        assert!(history.undo(&mut doc));assert_eq!(&snapshot(&doc,&mut renderer),state);
    }
    for state in &states[1..] {assert!(history.redo(&mut doc));assert_eq!(&snapshot(&doc,&mut renderer),state);}
}
