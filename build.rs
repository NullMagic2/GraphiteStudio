#[path = "build/app_icon.rs"]
mod app_icon;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build/app_icon.rs");
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let rgba = app_icon::render(256);
    std::fs::write(out.join("graphite-icon.rgba"), &rgba).unwrap();
    image::save_buffer(
        out.join("graphite-icon.png"),
        &rgba,
        256,
        256,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let icon = out.join("graphite.ico");
    std::fs::write(&icon, app_icon::ico()).unwrap();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon(icon.to_str().unwrap())
            .set("ProductName", "Graphite Studio")
            .set("FileDescription", "Graphite Studio")
            .set("OriginalFilename", "Graphite Studio.exe")
            .set("InternalName", "GraphiteStudio")
            .compile()
            .expect("compile Graphite Studio Windows icon and version resources");
    }
}
