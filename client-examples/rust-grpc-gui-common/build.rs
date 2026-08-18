/// Build script for the gRPC GUI common library.
///
/// Compiles the ok-FPGA-Server proto file into Rust client stubs.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_path = "../../proto/okfpga/v1/okfpga.proto";
    let proto_dir = "../../proto/okfpga/v1";

    // Use the PROTOC environment variable if set, otherwise try system protoc.
    if std::env::var("PROTOC").is_err() {
        let winget_protoc = format!(
            "{}\\AppData\\Local\\Microsoft\\WinGet\\Packages\\Google.Protobuf_Microsoft.Winget.Source_8wekyb3d8bbwe\\bin\\protoc.exe",
            std::env::var("USERPROFILE").unwrap_or_default()
        );
        if std::path::Path::new(&winget_protoc).exists() {
            std::env::set_var("PROTOC", &winget_protoc);
        }
    }

    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(&[proto_path], &[proto_dir])?;

    println!("cargo:rerun-if-changed={}", proto_path);
    Ok(())
}
