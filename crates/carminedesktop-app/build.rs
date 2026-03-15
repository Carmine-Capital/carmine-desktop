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
/// On Windows + desktop, Tauri already compiles `resource.rc` (app icon,
/// version info, manifest) and emits `cargo:rustc-link-lib=static=resource`.
/// A second `embed_resource::compile` targeting the same stem would duplicate
/// that directive, making link.exe process `resource.lib` twice (CVT1100 on
/// VERSION). Compiling to a *different* stem would create two `.lib` files
/// whose auto-assigned internal ICON ordinals collide (CVT1100 on ICON).
///
/// Fix: append our icons to Tauri's `resource.rc`, compile the combined file
/// under a new name (`combined_resources.lib`), then replace Tauri's
/// `resource.lib` with an empty COFF archive so it contributes nothing.
///
/// Without desktop (or on non-Windows) we compile icons standalone — on
/// non-Windows this is a no-op via `manifest_optional()`.
fn embed_file_icons() {
    #[cfg(all(target_os = "windows", feature = "desktop"))]
    {
        let out_dir = std::env::var("OUT_DIR").unwrap();
        let out_path = std::path::Path::new(&out_dir);
        let rc_path = out_path.join("resource.rc");
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

        // Write under a different stem so embed_resource emits a NEW link
        // directive (combined_resources) instead of duplicating "resource".
        let combined_rc = out_path.join("combined_resources.rc");
        std::fs::write(&combined_rc, &content).unwrap();

        // Replace Tauri's resource.lib with a minimal empty COFF archive
        // (signature + first linker member with 0 symbols = 72 bytes).
        // Tauri's link directive still references it, but it now contributes
        // nothing — all resources come from combined_resources.lib instead.
        #[rustfmt::skip]
        let empty_archive: Vec<u8> = [
            b"!<arch>\n"        .as_slice(), //  8: AR magic
            b"/               ",             // 16: member name (first linker member)
            b"0           ",                 // 12: timestamp
            b"0     ",                       //  6: UID
            b"0     ",                       //  6: GID
            b"0       ",                     //  8: mode
            b"4         ",                   // 10: size (4 bytes of content)
            b"`\n",                          //  2: end marker
            &[0u8; 4],                       //  4: 0 symbols (big-endian u32)
        ].concat();
        std::fs::write(out_path.join("resource.lib"), empty_archive).unwrap();

        embed_resource::compile(&combined_rc, embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }

    #[cfg(not(all(target_os = "windows", feature = "desktop")))]
    embed_resource::compile("icons/files/file_icons.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
