// The React build is embedded with include_dir; rebuild when it changes.
// The git description is exposed to the export's state.json; "unknown"
// when git is unavailable (tarballs, bundles without history).
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=web-dist");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    let describe = Command::new("git")
        .args(["describe", "--always", "--dirty", "--tags"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=CS_GIT_DESCRIBE={describe}");
}
