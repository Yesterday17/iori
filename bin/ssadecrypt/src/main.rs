use clap::Parser;
use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Write},
    path::PathBuf,
};

#[derive(Parser, Debug, Clone)]
#[clap(version = env!("BUILD_VERSION"), author)]
/// Decrypts an Sample-AES encrypted MPEG-TS file.
pub struct SsaDecryptArgs {
    /// The key to use for decryption.
    #[clap(short, long)]
    pub key: String,

    /// The initialization vector to use for decryption. Usually specified in M3U8 playlist.
    #[clap(short, long)]
    pub iv: String,

    /// The input file to decrypt.
    pub input: Option<PathBuf>,

    /// The output file to write the decrypted data to. If not specified, the decrypted data will be written to stdout.
    pub output: Option<PathBuf>,
}

fn main() -> Result<(), iori_ssa::Error> {
    let args = SsaDecryptArgs::parse();
    let key = hex::decode(args.key).expect("Invalid key");
    let iv = hex::decode(args.iv).expect("Invalid iv");

    let input = args.input.map_or_else(
        || Box::new(BufReader::new(std::io::stdin())) as Box<dyn Read>,
        |input| {
            Box::new(BufReader::new(
                File::open(input).expect("Failed to open input file"),
            ))
        },
    );
    let output = args.output.map_or_else(
        || Box::new(BufWriter::new(std::io::stdout())) as Box<dyn Write>,
        |output| {
            Box::new(BufWriter::new(
                File::create(output).expect("Failed to create output file"),
            ))
        },
    );

    iori_ssa::decrypt(
        input,
        output,
        key.try_into().expect("Invalid key length"),
        iv.try_into().expect("Invalid iv length"),
    )?;

    Ok(())
}
