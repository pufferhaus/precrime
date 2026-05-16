// PRECRIME_NDI_STUB=1 → skip linking libndi (CI, dev without NDI SDK installed).
//                       Binaries built in stub mode will crash at first libndi call.
// On Pi/Linux without the env var: links -lndi from NDI_LIB_DIR (default /usr/local/lib).
// On macOS: always uses dynamic_lookup so cargo test works without NDI for Mac installed.

fn main() {
    let stub = std::env::var("PRECRIME_NDI_STUB").is_ok();

    if cfg!(target_os = "linux") && !stub {
        let lib_dir = std::env::var("NDI_LIB_DIR").unwrap_or_else(|_| "/usr/local/lib".to_string());
        println!("cargo:rustc-link-search=native={lib_dir}");
        println!("cargo:rustc-link-lib=dylib=ndi");
    } else if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-arg=-Wl,-undefined,dynamic_lookup");
    } else if cfg!(target_os = "linux") {
        // Stub mode on Linux: allow unresolved libndi symbols at link time.
        println!("cargo:rustc-link-arg=-Wl,--unresolved-symbols=ignore-all");
    }

    println!("cargo:rerun-if-env-changed=NDI_LIB_DIR");
    println!("cargo:rerun-if-env-changed=PRECRIME_NDI_STUB");
}
