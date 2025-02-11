use std::io::Result;

fn main() -> Result<()> {
    prost_build::compile_protos(&["src/license_protocol.proto"], &["src/widevine"])?;
    Ok(())
}
