use std::error::Error;
use std::process::Command;

fn get_commit_hash() -> Result<String, Box<dyn Error>> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()?;
    let hash = String::from_utf8(output.stdout)?;
    Ok(hash.trim().to_string())
}

fn main() {
    let version = env!("CARGO_PKG_VERSION");
    let hash = get_commit_hash().unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=IORI_MINYAMI_VERSION={version} ({hash})");
}
