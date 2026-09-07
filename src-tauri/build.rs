fn main() {
    // Vendored protoc keeps the build free of a system protobuf toolchain.
    std::env::set_var(
        "PROTOC",
        protoc_bin_vendored::protoc_bin_path().expect("vendored protoc"),
    );
    prost_build::compile_protos(&["protos/rustdesk.proto"], &["protos"])
        .expect("compile RustDesk protocol description");
    println!("cargo:rerun-if-changed=protos/rustdesk.proto");
    tauri_build::build()
}
