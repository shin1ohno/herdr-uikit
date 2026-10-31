// Wave 0 skeleton: the renderer stream generates `scene.h` with cbindgen from
// gpui-pre's scene types and stitches it into the Metal shader source here,
// for iOS targets only (checked through CARGO_CFG_TARGET_OS, since this script
// runs on the host).
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
}
