//! Adapted from gpui-pre-apple 0.3.6 (zed@bcf6582) build.rs, Apache-2.0; see NOTICE.md.
//! Modified: locked registry resolution, paired ABI assertions, and iOS runtime shaders.
//! Generate a self-contained shader source on the host, only for iOS targets.
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CARGO_HOME");
    if env::var("CARGO_CFG_TARGET_OS")?.as_str() != "ios" {
        return Ok(());
    }
    let cargo_home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")))
        .ok_or("Cargo home is unavailable")?;
    let lock =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("manifest directory missing")?)
            .join("../../Cargo.lock");
    println!("cargo:rerun-if-changed={}", lock.display());
    let contents = fs::read_to_string(&lock)?;
    let packages: Vec<_> = contents
        .split("[[package]]")
        .filter(|section| {
            section
                .lines()
                .any(|line| line.trim() == "name = \"gpui-pre\"")
        })
        .collect();
    if packages.len() != 1 {
        return Err("expected exactly one locked gpui-pre package".into());
    }
    let package = packages[0];
    if !package
        .lines()
        .any(|line| line.starts_with("source = \"registry+"))
    {
        return Err("gpui-pre must use a locked Cargo registry source".into());
    }
    let version = package
        .lines()
        .find_map(|line| {
            line.strip_prefix("version = \"")
                .and_then(|value| value.strip_suffix('"'))
        })
        .ok_or("locked gpui-pre version missing")?;
    let mut gpui = None;
    for registry in fs::read_dir(cargo_home.join("registry/src"))? {
        let candidate = registry?.path().join(format!("gpui-pre-{version}"));
        if candidate.join("src/scene.rs").is_file() {
            gpui = Some(candidate);
            break;
        }
    }
    let gpui = gpui.ok_or("locked gpui-pre sources were not found in Cargo registry")?;
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is missing")?);
    let mut config = cbindgen::Config {
        include_guard: Some("SCENE_H".into()),
        language: cbindgen::Language::C,
        no_includes: true,
        ..Default::default()
    };
    config.enumeration.prefix_with_name = true;
    config.export.include.extend(
        [
            "Bounds",
            "Corners",
            "Edges",
            "Size",
            "Pixels",
            "PointF",
            "Hsla",
            "ContentMask",
            "Uniforms",
            "AtlasTile",
            "PathRasterizationInputIndex",
            "PathVertex_ScaledPixels",
            "PathRasterizationVertex",
            "ShadowInputIndex",
            "Shadow",
            "QuadInputIndex",
            "Underline",
            "UnderlineInputIndex",
            "Quad",
            "BorderStyle",
            "SpriteInputIndex",
            "MonochromeSprite",
            "PolychromeSprite",
            "PathSprite",
            "TransformationMatrix",
        ]
        .map(String::from),
    );
    let mut builder = cbindgen::Builder::new().with_config(config);
    for file in [
        "scene.rs",
        "geometry.rs",
        "color.rs",
        "window.rs",
        "platform.rs",
    ] {
        let path = gpui.join("src").join(file);
        println!("cargo:rerun-if-changed={}", path.display());
        builder = builder.with_src(path);
    }
    let types = PathBuf::from("src/renderer/types.rs");
    println!("cargo:rerun-if-changed={}", types.display());
    let bindings = builder.with_src(types).generate()?;
    let mut header = Vec::new();
    bindings.write(&mut header);
    fs::write(output.join("scene.h"), &header)?;
    let sizes_path = "src/renderer/abi_sizes.txt";
    println!("cargo:rerun-if-changed={sizes_path}");
    let sizes = fs::read_to_string(sizes_path)?;
    let mut rust = String::new();
    let mut msl = String::new();
    let mut checked = std::collections::HashSet::new();
    for line in sizes
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let fields: Vec<_> = line.split('|').collect();
        if fields.len() != 3 {
            return Err("invalid renderer ABI size row".into());
        }
        let (rust_type, msl_type, bytes) = (fields[0], fields[1], fields[2].parse::<usize>()?);
        if !checked.insert(msl_type.to_owned()) {
            return Err("duplicate renderer ABI size row".into());
        }
        rust.push_str(&format!(
            "const _: () = assert!(size_of::<{rust_type}>() == {bytes});\n"
        ));
        msl.push_str(&format!(
            "static_assert(sizeof({msl_type}) == {bytes}, \"{msl_type} ABI size\");\n"
        ));
    }
    // A snapshot upgrade adding another shared struct must update the paired checks.
    let generated = std::str::from_utf8(&header)?;
    for name in generated.lines().filter_map(|line| {
        line.strip_prefix("} ")
            .and_then(|name| name.strip_suffix(';'))
    }) {
        if !checked.contains(name) {
            return Err(format!("missing ABI assertion for {name}").into());
        }
    }
    fs::write(output.join("renderer_abi.rs"), rust)?;
    header.extend_from_slice(msl.as_bytes());
    let shaders = "src/renderer/shaders.metal";
    println!("cargo:rerun-if-changed={shaders}");
    header.extend_from_slice(fs::read(shaders)?.as_slice());
    let helpers = "src/renderer/shader_helpers.metal";
    println!("cargo:rerun-if-changed={helpers}");
    header.extend_from_slice(fs::read(helpers)?.as_slice());
    fs::write(output.join("stitched_shaders.metal"), header)?;
    Ok(())
}
