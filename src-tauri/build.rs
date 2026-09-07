use std::path::PathBuf;

/// Locates libvpx the same way the native helper script locates FreeRDP, so the
/// build needs no package-config tooling beyond Homebrew.
fn libvpx_prefix() -> PathBuf {
    if let Ok(prefix) = std::env::var("VPX_PREFIX") {
        return PathBuf::from(prefix);
    }
    let brewed = std::process::Command::new("brew")
        .args(["--prefix", "libvpx"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|path| !path.is_empty());
    match brewed {
        Some(path) => PathBuf::from(path),
        None => panic!("libvpx not found. Run `brew install libvpx`, or set VPX_PREFIX."),
    }
}

fn main() {
    // Vendored protoc keeps the build free of a system protobuf toolchain.
    std::env::set_var(
        "PROTOC",
        protoc_bin_vendored::protoc_bin_path().expect("vendored protoc"),
    );
    prost_build::compile_protos(&["protos/rustdesk.proto"], &["protos"])
        .expect("compile RustDesk protocol description");
    println!("cargo:rerun-if-changed=protos/rustdesk.proto");

    // libvpx is linked statically so the bundled app carries no extra dylib and
    // needs no install-name rewriting before signing.
    let vpx = libvpx_prefix();
    cc::Build::new()
        .include(vpx.join("include"))
        .file("../native/vpx_decoder.c")
        .opt_level(2)
        .compile("agentsmith_vpx");
    // Name the archive outright. With both libvpx.a and libvpx.dylib in the
    // same directory, `-lvpx` on macOS takes the dylib even when asked for a
    // static link, and the app then depends on a Homebrew path that other Macs
    // do not have.
    let archive = vpx.join("lib/libvpx.a");
    assert!(archive.is_file(), "libvpx.a not found at {}", archive.display());
    println!("cargo:rustc-link-arg={}", archive.display());
    println!("cargo:rerun-if-changed=../native/vpx_decoder.c");
    println!("cargo:rerun-if-env-changed=VPX_PREFIX");

    tauri_build::build()
}
