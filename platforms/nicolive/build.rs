fn main() -> std::io::Result<()> {
    // std::env::set_var("PROTOC", protobuf_src::protoc());

    prost_build::compile_protos(
        &[
            "src/proto/dwango/nicolive/chat/data/atoms.proto",
            "src/proto/dwango/nicolive/chat/data/message.proto",
            // "src/proto/dwango/nicolive/chat/data/origin.proto",
            // "src/proto/dwango/nicolive/chat/data/state.proto",
            "src/proto/dwango/nicolive/chat/edge/payload.proto",
        ],
        &["src/proto/"],
    )?;
    Ok(())
}
