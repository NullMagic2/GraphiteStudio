use graphite_studio::{core::{document::{BlendMode, CanvasSpec, Document}, history::{EditTransaction, History}, material::PixelDepositState, paper::PaperPreset, pencil::{ToolKind, ToolSettings}, stroke::StrokePoint}, render::{DocumentRenderer, RasterRenderer}};

fn sheet() -> Document {
    let spec = CanvasSpec::from_physical("Merge", 12., 9., 90.);
    let n = spec.pixel_count();
    Document::new(spec, PaperPreset::DrawingMedium, "White", vec![[0.9, 0.82, 0.74]; n])
}

#[test]
fn merge_many_is_one_undo_and_preserves_unselected_layers_and_order() {
    for nonadjacent in [false,true] {
        let mut doc=sheet();
        for l in 0..6 { if l>0 {doc.add_layer();} seed(&mut doc,l%4); }
        let chosen: Vec<_>=if nonadjacent {vec![0,2,4]}else{vec![1,2,3,4]};
        doc.layer_selection.ids=chosen.iter().map(|&i|doc.layers[i].id).collect();
        doc.activate_layer(*chosen.last().unwrap());
        let original=doc.clone(); let mut h=History::default(); let mut r=RasterRenderer::default();
        let before=r.rgba8(&doc);
        assert!(h.merge_selected(&mut doc));
        assert_eq!(doc.layer_count(),7-chosen.len());
        assert_eq!(doc.selected_layer_indices(),vec![doc.active_layer_index()]);
        let expected_order:Vec<_>=(0..6).filter(|i|!chosen.contains(i) || *i==*chosen.last().unwrap()).map(|i|original.layers[if i==*chosen.last().unwrap(){chosen[0]}else{i}].id).collect();
        assert_eq!(doc.layers.iter().map(|l|l.id).collect::<Vec<_>>(),expected_order);
        // All Multiply here: selected layers also commute across intervening Multiply layers.
        close(&before,&r.rgba8(&doc));
        for i in (0..6).filter(|i|!chosen.contains(i)) {
            let now=doc.layers.iter().position(|l|l.id==original.layers[i].id).unwrap();
            for p in 0..doc.spec.pixel_count() {assert_eq!(doc.layer_deposit_pixel(now,p),original.layer_deposit_pixel(i,p));}
        }
        let merged=r.rgba8(&doc);
        h.rotate_document(&mut doc,true); assert!(h.undo(&mut doc));
        assert!(h.undo(&mut doc)); assert_eq!(doc.layer_count(),6); assert_eq!(doc.selected_layer_indices(),chosen);
        assert_eq!(r.rgba8(&doc),before); assert!(!h.can_undo());
        assert!(h.redo(&mut doc)); assert_eq!(r.rgba8(&doc),merged);
    }
}

#[test]
fn many_layer_mixed_blends_preserve_contiguous_appearance_and_hidden_selection_is_atomic() {
    for first in BlendMode::ALL {
        let mut doc=sheet();
        for l in 0..5 {if l>0 {doc.add_layer();} seed(&mut doc,l%4); doc.layers[l].blend_mode=if l==1 {first}else{BlendMode::ALL[l]}; doc.layers[l].opacity=90+l as u8*30;}
        doc.layer_selection.ids=doc.layers[1..4].iter().map(|l|l.id).collect();
        doc.activate_layer(2); let mut h=History::default();
        doc.layers[3].visible=false;
        assert!(!h.merge_selected(&mut doc)); assert_eq!(doc.layer_count(),5); assert!(!h.can_undo());
        doc.layers[3].visible=true;
        let mut r=RasterRenderer::default(); let before=r.rgba8(&doc);
        assert!(h.merge_selected(&mut doc));close(&before,&r.rgba8(&doc));
        assert!(h.undo(&mut doc)); assert_eq!(doc.active_layer_index(),2); assert_eq!(r.rgba8(&doc),before);
    }
}

#[test]
fn ctrl_and_shift_selection_follow_layer_ids_and_never_drop_the_last_selection() {
    let mut doc=sheet(); for _ in 0..5 {doc.add_layer();}
    doc.select_layer(1,false,false);
    doc.select_layer(4,false,true); assert_eq!(doc.selected_layer_indices(),vec![1,2,3,4]);
    doc.select_layer(2,false,true); assert_eq!(doc.selected_layer_indices(),vec![1,2]);
    doc.select_layer(5,true,false); assert_eq!(doc.selected_layer_indices(),vec![1,2,5]);
    doc.select_layer(2,true,false); assert_eq!(doc.selected_layer_indices(),vec![1,5]);
    doc.select_layer(5,true,false); assert_eq!(doc.selected_layer_indices(),vec![1]); assert_eq!(doc.active_layer_index(),1);
    doc.select_layer(1,true,false); assert_eq!(doc.selected_layer_indices(),vec![1]);
    let id=doc.layers[1].id; let target=doc.layers[4].id;
    doc.move_layer_relative(id,target,true);assert_eq!(doc.selected_layer_indices(),vec![doc.active_layer_index()]);assert_eq!(doc.active_layer_id(),id);
    let reopened:Document=serde_json::from_slice(&serde_json::to_vec(&doc).unwrap()).unwrap();
    assert!(reopened.layer_selection.ids.is_empty());assert_eq!(reopened.selected_layer_indices(),vec![reopened.active_layer_index()]);
}
fn seed(doc: &mut Document, layer: usize) {
    let color = [[0.15, 0.7, 0.3], [0.7, 0.12, 0.3], [0.08, 0.2, 0.8], [0.3, 0.2, 0.4]][layer];
    for i in 0..doc.spec.pixel_count() {
        if i % (layer+2) == 0 { continue; }
        let m = 0.05 + (i%17) as f32 * 0.055;
        doc.surface.set_deposit_pixel(i, PixelDepositState { graphite_mass: m*0.7, clay_mass:m*0.25, wax_mass:m*0.05, loose_mass:m*0.7, compacted_mass:m*0.3, color_r_mass:m*color[0], color_g_mass:m*color[1], color_b_mass:m*color[2], ..Default::default() });
    }
}
fn close(a: &[u8], b: &[u8]) {
    assert_eq!(a.len(), b.len());
    assert!(a.iter().zip(b).all(|(&a,&b)| a.abs_diff(b)<=1));
}

#[test]
fn merge_preserves_appearance_for_all_modes_opacities_and_roundtrips() {
    for lower in BlendMode::ALL { for upper in BlendMode::ALL { for opacity in [0, 113, 255] {
        let mut doc = sheet(); seed(&mut doc, 0);
        doc.add_layer(); seed(&mut doc, 1); doc.layers[1].blend_mode=lower; doc.layers[1].opacity=187;
        doc.add_layer(); seed(&mut doc, 2); doc.layers[2].blend_mode=upper; doc.layers[2].opacity=opacity;
        let before=doc.clone(); let mut renderer=RasterRenderer::default(); let pixels=renderer.rgba8(&doc);
        let mut history=History::default(); assert!(history.merge_down(&mut doc));
        assert_eq!(doc.layer_count(),2); assert_eq!(doc.active_layer_index(),1);
        assert_eq!(doc.layers[1].opacity,255); assert!(doc.layers[1].vectors.base.is_some());
        close(&pixels,&renderer.rgba8(&doc));
        let saved=serde_json::to_vec(&doc).unwrap();
        let reopened:Document=serde_json::from_slice(&saved).unwrap();
        close(&pixels,&renderer.rgba8(&reopened));
        assert!(history.undo(&mut doc)); assert_eq!(doc.layer_count(),3);
        assert_eq!(renderer.rgba8(&doc),pixels);
        for l in 0..3 { for i in 0..doc.spec.pixel_count() { assert_eq!(doc.layer_deposit_pixel(l,i),before.layer_deposit_pixel(l,i)); } }
        assert!(history.redo(&mut doc)); close(&pixels,&renderer.rgba8(&doc));
    }}}
}

#[test]
fn multiply_merge_keeps_transparency_on_changed_paper_and_rotated_undo() {
    let mut doc=sheet(); seed(&mut doc,0); doc.add_layer(); seed(&mut doc,1);
    let mut unmerged=doc.clone(); let mut h=History::default(); assert!(h.merge_down(&mut doc));
    let mut r=RasterRenderer::default();
    for tint in [[255;3],[125,190,230],[25,40,60]] {
        doc.set_paper_color(tint); unmerged.set_paper_color(tint);
        close(&r.rgba8(&doc),&r.rgba8(&unmerged));
    }
    let merged=r.rgba8(&doc);
    h.rotate_document(&mut doc,true); assert!(h.undo(&mut doc));
    assert_eq!(r.rgba8(&doc),merged); assert!(h.undo(&mut doc));
    assert_eq!(r.rgba8(&doc),r.rgba8(&unmerged));
    assert!(h.redo(&mut doc)); assert_eq!(r.rgba8(&doc),merged);
    assert!(h.redo(&mut doc)); assert!(h.undo(&mut doc)); assert_eq!(r.rgba8(&doc),merged);
}

#[test]
fn merging_protects_hidden_and_last_layers_and_keeps_unrelated_layers() {
    let mut doc=sheet(); let mut h=History::default(); assert!(!h.merge_down(&mut doc));
    seed(&mut doc,0); doc.add_layer(); seed(&mut doc,1);
    doc.layers[0].visible=false; assert!(!h.merge_down(&mut doc)); doc.layers[0].visible=true;
    doc.layers[1].visible=false; assert!(!h.merge_down(&mut doc)); doc.layers[1].visible=true;
    doc.add_layer(); seed(&mut doc,2); let other=doc.surface.graphite_mass.clone(); doc.activate_layer(1);
    assert!(h.merge_down(&mut doc)); doc.activate_layer(1); assert_eq!(doc.surface.graphite_mass,other);
    assert!(h.undo(&mut doc)); doc.activate_layer(2); assert_eq!(doc.surface.graphite_mass,other);
}

#[test]
fn buildup_adds_fifteen_percent_capacity_without_darkening_light_tissue_passes() {
    let mut doc=sheet();
    // Compare paper capacities at the same historical transfer setting,
    // independently of the darker default introduced in v0.24.2.
    let settings=ToolSettings { tool:ToolKind::Tissue, flow:1., tissue_size_px:12., tissue_load:1., pencil_color_rgb:[0;3], ..Default::default() };
    let p=StrokePoint { x:12.5,y:12.5,pressure:1.,tilt_deg:8.,azimuth_deg:20.,rotation_deg:None };
    let i=doc.index(12,12); let old=0.38+1.55*(1.-doc.surface.current_height[i]).clamp(0.,1.)+0.24*doc.surface.fiber[i];
    assert!((doc.surface.local_capacity(i)/old-1.15).abs()<1e-6);
    let travel=0.1; let work=0.2*travel/12.*(0.8+0.2*doc.surface.contact_support[i] as f32/255.);
    let old_first=old*(-(-work).exp_m1());
    graphite_studio::core::powder::apply(&mut doc,&settings,p,travel,&mut EditTransaction::default());
    assert!((doc.surface.total_deposit(i)/old_first-1.).abs()<0.001);
    for _ in 0..20 { graphite_studio::core::powder::apply(&mut doc,&settings,p,1000.,&mut EditTransaction::default()); }
    assert!(doc.surface.total_deposit(i)>old*1.14);
    let dense=doc.surface.deposit_pixel(i);
    let old_depth=dense.optical_depth()/1.15;
    assert!((-dense.optical_depth()).exp()<(-old_depth).exp());
    assert_eq!(dense.color_r_mass,0.);
}

#[test]
fn merged_material_saves_in_native_and_both_psd_depths_and_can_be_erased() {
    use graphite_studio::{project::{self,ProjectRef}, core::{pencil::PencilTipState,stroke::StrokeEngine}, export::PsdBitDepth};
    let mut doc=sheet(); seed(&mut doc,0); doc.add_layer(); seed(&mut doc,1);
    doc.add_layer();seed(&mut doc,2);
    doc.layer_selection.ids=doc.layers.iter().map(|l|l.id).collect();
    let mut h=History::default(); assert!(h.merge_selected(&mut doc));
    let mut r=RasterRenderer::default(); let before=r.rgba8(&doc);
    let settings=ToolSettings::default(); let tip=PencilTipState::default(); let engine=StrokeEngine::default();
    let data=ProjectRef { document:&doc,settings:&settings,tip:&tip,engine:&engine,brushes:&[],custom_paper:&None,zoom:1.,pan:[0.;2] };
    for format in 0..3 {
        let path=std::env::temp_dir().join(format!("graphite-merge-{}-{format}.{}",std::process::id(),if format==0 {"graphite"}else{"psd"}));
        if format==0 { project::save(&path,&data).unwrap(); } else { project::save_psd(&path,&data,&mut r,if format==1 {PsdBitDepth::Eight}else{PsdBitDepth::Sixteen}).unwrap(); }
        let reopened=project::load(&path).unwrap(); std::fs::remove_file(&path).unwrap();
        assert_eq!(reopened.document.layer_count(),1);
        assert!(reopened.document.layers[0].vectors.base.is_some());
        assert_eq!(r.rgba8(&reopened.document),before);
    }
    let eraser=ToolSettings {tool:ToolKind::Eraser,eraser_diameter_mm:200.*25.4/doc.spec.dpi,eraser_strength:1.,..Default::default()};
    let p=StrokePoint {x:20.,y:16.,pressure:1.,tilt_deg:8.,azimuth_deg:20.,rotation_deg:None};
    let mut tx=EditTransaction::default();
    StrokeEngine::default().apply_segment(&mut doc,&eraser,&mut PencilTipState::default(),p,StrokePoint {x:22.,..p},&mut tx);
    assert!(doc.surface.graphite_mass.iter().all(|&m|m==0.));
    h.push(tx,&doc); assert!(h.undo(&mut doc)); assert_eq!(r.rgba8(&doc),before);
}
