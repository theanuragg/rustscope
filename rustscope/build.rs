// build.rs — runs at compile time to capture rustc version.
fn main() {
    // Emit rustc version into an env var readable via env!()
    let version = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=RUSTC_VERSION_FOR_RUSTSCOPE={}", version);
    println!("cargo:rerun-if-changed=build.rs");
}
