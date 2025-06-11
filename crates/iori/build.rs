fn main() {
    #[cfg(all(target_os = "windows", target_env = "gnu"))]
    {
        println!("cargo:rustc-link-search=/usr/x86_64-w64-mingw32/lib");
        println!("cargo:rustc-link-search=/usr/mingw64/lib");

        // Force static linking of C runtime
        println!("cargo:rustc-link-arg=-static-libgcc");
        println!("cargo:rustc-link-arg=-static-libstdc++");
    }
}
