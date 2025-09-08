fn main() {
    // Basic build script without Windows resource generation
    println!("cargo:rerun-if-changed=tauri.conf.json");
}