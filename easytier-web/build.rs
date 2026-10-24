use std::env;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    // enable thunk-rs when target os is windows and arch is x86_64 or i686
    if target_os == "windows" && (target_arch == "x86" || target_arch == "x86_64") {
        thunk::thunk();
    } else {
        // Keep embedded assets tracked, including directory additions/deletions,
        // without treating a directory-served frontend as native build input.
        println!("cargo:rerun-if-changed=build.rs");
        println!("cargo:rerun-if-changed=resources");
        println!("cargo:rerun-if-changed=locales");
        if env::var_os("CARGO_FEATURE_EMBED").is_some() {
            println!("cargo:rerun-if-changed=frontend/dist");
        }
    }
}

