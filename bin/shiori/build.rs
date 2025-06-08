use std::env;
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
    let variant = if cfg!(feature = "ffmpeg") {
        "ffmpeg"
    } else {
        "core"
    };
    let hash = get_commit_hash().unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=SHIORI_VERSION={version} ({variant}-{hash})");

    if let Ok("windows") = std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        let mut res = winresource::WindowsResource::new();
        res.set_manifest_file("windows.manifest.xml");
        res.compile().unwrap();
    } else {
        // Avoid rerunning the build script every time.
        println!("cargo:rerun-if-changed=build.rs");
    }
}
