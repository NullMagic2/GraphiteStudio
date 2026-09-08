//! Opt-in full-size file regression; private artwork is never part of the fixture bundle.
use graphite_studio::{import,project::{self,ProjectRef},render::{DocumentRenderer,RasterRenderer},export::PsdBitDepth};
use std::path::PathBuf;

#[test]
#[ignore = "Set GRAPHITE_TEST_PSD, GRAPHITE_TEST_RGB and GRAPHITE_TEST_OUTPUT for a local full-size regression"]
fn full_size_psd_pixels_and_editable_save_roundtrips() {
    let path=PathBuf::from(std::env::var_os("GRAPHITE_TEST_PSD").expect("input PSD"));
    let expected=std::fs::read(std::env::var_os("GRAPHITE_TEST_RGB").expect("independent RGB reference")).unwrap();
    let output=PathBuf::from(std::env::var_os("GRAPHITE_TEST_OUTPUT").expect("scratch output folder"));
    std::fs::create_dir_all(&output).unwrap();
    let p=import::open(&path).unwrap().project;
    assert_eq!((p.document.spec.width_px,p.document.spec.height_px),(4032,3024));
    assert_eq!(p.document.layers.len(),1);
    let check=|p:&project::Project| {
        let rendered=RasterRenderer::default().rgba8(&p.document);
        assert_eq!(rendered.len()/4,expected.len()/3);
        for (i,(a,b)) in rendered.chunks_exact(4).zip(expected.chunks_exact(3)).enumerate() {
            assert_eq!(&a[..3],b,"pixel {i}");assert_eq!(a[3],255);
        }
        eprintln!("Verified {} x {} pixels and {} layer(s).",p.document.spec.width_px,p.document.spec.height_px,p.document.layers.len());
    };
    check(&p);
    let data=ProjectRef{document:&p.document,settings:&p.settings,tip:&p.tip,engine:&p.engine,brushes:&p.brushes,custom_paper:&p.custom_paper,zoom:p.zoom,pan:p.pan};
    eprintln!("Saving native project...");
    project::save(&output.join("full-size.graphite"),&data).unwrap();
    eprintln!("Saving editable PSD...");
    project::save_psd(&output.join("full-size.psd"),&data,&mut RasterRenderer::default(),PsdBitDepth::Eight).unwrap();
    drop(p);
    for name in ["full-size.graphite","full-size.psd"] {
        eprintln!("Reopening {name}...");
        let reopened=import::open(&output.join(name)).unwrap();assert!(reopened.editable_source);check(&reopened.project);
    }
}
