fn main() {
    if cfg!(target_os = "linux") {
        let lib_dir =
            std::env::var("NDI_LIB_DIR").unwrap_or_else(|_| "/usr/local/lib".to_string());
        println!("cargo:rustc-link-search=native={lib_dir}");
        println!("cargo:rustc-link-lib=dylib=ndi");
    } else if cfg!(target_os = "macos") {
        // Allow undefined symbols at link time so the crate builds without libndi
        // installed on the dev host. Tests don't invoke ndi_find functions; if a
        // binary actually does on macOS without libndi present, it'll fail at
        // first call.
        println!("cargo:rustc-link-arg=-Wl,-undefined,dynamic_lookup");
    }
    println!("cargo:rerun-if-env-changed=NDI_LIB_DIR");
}
