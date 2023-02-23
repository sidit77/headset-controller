
#[cfg(windows)]
fn main() {
    let icon_path = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("icon.ico");
    println!("cargo:rerun-if-changed=resources/icon.png");
    let mut icon_dir = ico::IconDir::new(ico::ResourceType::Icon);
    let file = std::fs::File::open("resources/icon.png").unwrap();
    let image = ico::IconImage::read_png(file).unwrap();
    icon_dir.add_entry(ico::IconDirEntry::encode(&image).unwrap());
    icon_dir.write(std::fs::File::create(&icon_path).unwrap()).unwrap();

    let mut res = tauri_winres::WindowsResource::new();
    res.set_icon(icon_path.to_str().unwrap());
    res.compile().unwrap();
}

#[cfg(not(windows))]
fn main() {}