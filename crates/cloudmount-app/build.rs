fn main() {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not determine workspace root");

    let defaults_path = workspace_root.join("build").join("defaults.toml");
    let example_path = workspace_root.join("build").join("defaults.toml.example");

    if !defaults_path.exists() {
        std::fs::copy(&example_path, &defaults_path)
            .expect("failed to copy defaults.toml.example to defaults.toml");
    }

    println!("cargo:rerun-if-changed={}", defaults_path.display());

    #[cfg(feature = "desktop")]
    tauri_build::build();
}
