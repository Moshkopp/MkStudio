use std::path::PathBuf;

fn main() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../branding.conf");
    println!("cargo:rerun-if-changed={}", path.display());
    let content = std::fs::read_to_string(&path).expect("branding.conf muss lesbar sein");
    for line in content.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .expect("branding.conf erwartet SCHLUESSEL=WERT");
        match key {
            "PRODUCT_NAME" | "HUB_NAME" | "HUB_PROTOCOL_ID" | "APP_ID" | "DATA_DIR_NAME" => {
                println!("cargo:rustc-env={key}={value}");
            }
            _ => panic!("Unbekannter Branding-Schlüssel: {key}"),
        }
    }
}
