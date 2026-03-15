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
}
