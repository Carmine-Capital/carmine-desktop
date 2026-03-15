fn main() {
    // Ensure winfsp-x64.dll is delay-loaded so the process can start without
    // the DLL present (e.g. when launched from Explorer via context menu).
    // The winfsp-sys crate also emits these flags, but they may not propagate
    // reliably to the final binary — so we repeat them here for safety.
    #[cfg(all(target_os = "windows", target_env = "msvc"))]
    {
        println!("cargo:rustc-link-lib=dylib=delayimp");
        #[cfg(target_arch = "x86_64")]
        println!("cargo:rustc-link-arg=/DELAYLOAD:winfsp-x64.dll");
        #[cfg(target_arch = "x86")]
        println!("cargo:rustc-link-arg=/DELAYLOAD:winfsp-x86.dll");
        #[cfg(target_arch = "aarch64")]
        println!("cargo:rustc-link-arg=/DELAYLOAD:winfsp-a64.dll");
    }

    #[cfg(feature = "desktop")]
    tauri_build::build();

    embed_file_icons();
}

/// Embed per-file-type icon resources (doc, xls, ppt, pdf) for Windows shell
/// integration.
///
/// On Windows + desktop, Tauri already compiles a `resource.rc` containing the
/// app icon (ICON ordinal 1). A separate `embed_resource::compile` call would
/// produce a second `.lib` whose auto-assigned internal ICON ordinals clash
/// (CVT1100 / LNK1123). We fix this by appending our icon definitions to
/// Tauri's `resource.rc` and recompiling once, producing a single `resource.lib`.
///
/// Without desktop (or on non-Windows) we compile icons standalone — on
/// non-Windows this is a no-op via `manifest_optional()`.
fn embed_file_icons() {
    #[cfg(all(target_os = "windows", feature = "desktop"))]
    {
        let out_dir = std::env::var("OUT_DIR").unwrap();
        let rc_path = std::path::Path::new(&out_dir).join("resource.rc");
        let icons_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("icons")
            .join("files");
        let mut content =
            std::fs::read_to_string(&rc_path).expect("tauri resource.rc not found in OUT_DIR");
        // rc.exe accepts forward slashes — avoids double-backslash escaping.
        let dir = icons_dir.display().to_string().replace('\\', "/");
        use std::fmt::Write;
        write!(
            content,
            "\n101 ICON \"{dir}/doc.ico\"\
             \n102 ICON \"{dir}/xls.ico\"\
             \n103 ICON \"{dir}/ppt.ico\"\
             \n104 ICON \"{dir}/pdf.ico\"\n"
        )
        .unwrap();
        std::fs::write(&rc_path, &content).unwrap();
        // Recompile the combined resource file — overwrites Tauri's resource.lib.
        embed_resource::compile(&rc_path, embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }

    #[cfg(not(all(target_os = "windows", feature = "desktop")))]
    embed_resource::compile("icons/files/file_icons.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
