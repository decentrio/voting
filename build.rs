fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(false) // client only
        .compile_protos(
            &[
                "proto/query.proto",
                "proto/service.proto",
            ],
            &["proto"], // include path for imports
        )?;
    Ok(())
}