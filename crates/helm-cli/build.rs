fn main() {
    if std::env::var("CARGO_FEATURE_HARDWARE").is_err() {
        return;
    }

    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace = manifest_dir.join("../..");
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let bin = workspace.join("target").join(profile).join("helm-fake-device");
    println!("cargo:rustc-env=HELM_FAKE_DEVICE_BIN={}", bin.display());
    println!("cargo:rerun-if-changed=../helm-hardware");
}
