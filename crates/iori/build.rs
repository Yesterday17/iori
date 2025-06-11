use std::{path::Path, process::Command};

fn find_libcpp_path() -> std::io::Result<()> {
    let command = std::env::var("CXX").unwrap_or_else(|_| "x86_64-w64-mingw32-g++".to_string());
    let output = Command::new(command)
        .arg("-print-file-name=libstdc++.a")
        .output()
        .expect("Failed to find libstdc++.a");

    if !output.status.success() {
        return Err(std::io::Error::other(
            "Command -print-file-name=libstdc++.a returned empty output.",
        ));
    }

    let path = String::from_utf8_lossy(&output.stdout);
    let path = Path::new(path.trim());
    if let Some(path) = path.parent() {
        println!("cargo:rustc-link-search={}", path.canonicalize()?.display());
    }

    Ok(())
}

fn main() -> std::io::Result<()> {
    // Building for windows and (one of both):
    // - cross compile (not on Windows)
    // - or using -gnu target
    //
    // In this case, we need to link libstdc++ statically.
    // if std::env::var_os("CARGO_CFG_WINDOWS").is_some()
    //     && (cfg!(not(windows)) || cfg!(target_env = "gnu"))
    {
        if let Err(e) = find_libcpp_path() {
            println!("cargo:warning=Failed to find libstdc++.a: {}", e);
        }

        // Force static linking of C runtime
        println!("cargo:rustc-link-arg=-static-libgcc");
        println!("cargo:rustc-link-arg=-static-libstdc++");
    }

    Ok(())
}
