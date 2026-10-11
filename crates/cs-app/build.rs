// The React build is embedded with include_dir; rebuild when it changes.
fn main() {
    println!("cargo:rerun-if-changed=web-dist");
}
