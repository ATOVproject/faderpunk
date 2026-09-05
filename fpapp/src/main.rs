use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use serde::Deserialize;
use serde_json::{Value as JsonValue, json};

use libfp::fpapp::{
    Manifest, NativeEntrypoints, NativeProgram, PROGRAM_KIND_THUMB_ROPI, Package, PackageBuilder,
    Version, firmware_abi_from_revision,
};

const USAGE: &str = r#"fpapp - build, inspect, and verify Faderpunk apps

Usage:
  fpapp pack --elf APP.elf --output APP.fpapp --id 100 --version 1.0.0 \
    --name NAME --description TEXT --author NAME --channels 1 --color ff00ff \
    --icon 13 (--firmware-revision 40_HEX_DIGITS | --firmware-abi 64_HEX_DIGITS) \
    [--parameter-count N] [--manual FILE] [--setup FILE] \
    [--settings FILE] [--signing FILE]
  fpapp inspect APP.fpapp
  fpapp verify APP.fpapp [--firmware-abi 64_HEX_DIGITS]
  fpapp abi 40_HEX_DIGIT_GIT_REVISION
  fpapp build-community --repo PATH --output DIR \
    (--firmware-revision 40_HEX_DIGITS | --firmware-abi 64_HEX_DIGITS) \
    [--version MAJOR.MINOR.PATCH]

`pack` requires arm-none-eabi-readelf, arm-none-eabi-nm, and
arm-none-eabi-objcopy. It rejects absolute relocations and writable allocated
sections.
"#;

fn main() -> ExitCode {
    match run(env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("fpapp: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: Vec<OsString>) -> Result<(), String> {
    let Some(command) = arguments.first().and_then(|value| value.to_str()) else {
        print!("{USAGE}");
        return Ok(());
    };
    match command {
        "pack" => pack(parse_options(&arguments[1..])?),
        "inspect" => {
            let path = positional_path(&arguments[1..], "inspect")?;
            inspect(&path)
        }
        "verify" => verify(&arguments[1..]),
        "abi" => {
            if arguments.len() != 2 {
                return Err("abi requires one 40-digit Git revision".into());
            }
            let revision = arguments[1].to_str().ok_or("revision is not UTF-8")?;
            let abi = firmware_abi_from_revision(revision)
                .ok_or("revision must contain exactly 40 hexadecimal digits")?;
            println!("{}", format_abi(&abi));
            Ok(())
        }
        "build-community" => build_community(parse_options(&arguments[1..])?),
        "help" | "--help" | "-h" => {
            print!("{USAGE}");
            Ok(())
        }
        other => Err(format!("unknown command {other:?}\n\n{USAGE}")),
    }
}

fn pack(options: BTreeMap<String, String>) -> Result<(), String> {
    let elf = required_path(&options, "elf")?;
    let output = required_path(&options, "output")?;
    let readelf = tool_output(
        "arm-none-eabi-readelf",
        &[OsString::from("-SW"), elf.clone().into_os_string()],
    )?;
    verify_sections(&readelf)?;
    let relocations = tool_output(
        "arm-none-eabi-readelf",
        &[OsString::from("-rW"), elf.clone().into_os_string()],
    )?;
    verify_relocations(&relocations)?;

    let symbols = tool_output(
        "arm-none-eabi-nm",
        &[
            OsString::from("-n"),
            OsString::from("--defined-only"),
            elf.clone().into_os_string(),
        ],
    )?;
    let entrypoints = NativeEntrypoints {
        required_bytes: symbol_offset(&symbols, "fpapp_required_bytes")?,
        init: symbol_offset(&symbols, "fpapp_init")?,
        poll: symbol_offset(&symbols, "fpapp_poll")?,
        drop: symbol_offset(&symbols, "fpapp_drop")?,
    };

    let image_path = temporary_image_path(&output);
    let objcopy_result = Command::new("arm-none-eabi-objcopy")
        .args(["-O", "binary", "-j", ".text"])
        .arg(&elf)
        .arg(&image_path)
        .output()
        .map_err(|error| format!("could not run arm-none-eabi-objcopy: {error}"))?;
    if !objcopy_result.status.success() {
        return Err(format!(
            "arm-none-eabi-objcopy failed: {}",
            String::from_utf8_lossy(&objcopy_result.stderr).trim()
        ));
    }
    let image_result = fs::read(&image_path)
        .map_err(|error| format!("could not read {}: {error}", image_path.display()));
    let _ = fs::remove_file(&image_path);
    let image = image_result?;

    let mut native_bytes = vec![0u8; image.len() + 28];
    let native = NativeProgram::encode(entrypoints, &image, &mut native_bytes)
        .map_err(|error| format!("invalid native image: {error:?}"))?;
    let version = parse_version(required(&options, "version")?)?;
    let manifest = Manifest {
        app_id: parse_number(required(&options, "id")?, "id")?,
        version,
        program_kind: PROGRAM_KIND_THUMB_ROPI,
        name: required(&options, "name")?,
        description: required(&options, "description")?,
        author: required(&options, "author")?,
        channels: parse_number(required(&options, "channels")?, "channels")?,
        color_rgb: parse_hex_u32(required(&options, "color")?, "color")?,
        icon: parse_number(required(&options, "icon")?, "icon")?,
        parameter_count: parse_optional_number(&options, "parameter-count", 0)?,
        persistent_state_bytes: parse_optional_number(&options, "persistent-state-bytes", 0)?,
        execution_units_per_event: 0,
        capabilities: parse_optional_hex_u32(&options, "capabilities", 0)?,
        firmware_abi: firmware_abi_option(&options)?,
    };

    let manual = read_optional_text(&options, "manual")?;
    let setup = read_optional_text(&options, "setup")?;
    let settings = read_optional_text(&options, "settings")?;
    let signing = read_optional_bytes(&options, "signing")?;
    let mut builder = PackageBuilder::new(manifest, native);
    if let Some(value) = manual.as_deref() {
        builder = builder.with_manual(value);
    }
    if let Some(value) = setup.as_deref() {
        builder = builder.with_setup(value);
    }
    if let Some(value) = settings.as_deref() {
        builder = builder.with_settings(value);
    }
    if let Some(value) = signing.as_deref() {
        builder = builder.with_signing(value);
    }
    let extra_len = manual.as_ref().map_or(0, String::len)
        + setup.as_ref().map_or(0, String::len)
        + settings.as_ref().map_or(0, String::len)
        + signing.as_ref().map_or(0, Vec::len);
    let mut package_bytes = vec![0u8; native.len() + extra_len + 2048];
    let package = builder
        .encode(&mut package_bytes)
        .map_err(|error| format!("could not encode package: {error:?}"))?;
    fs::write(&output, package)
        .map_err(|error| format!("could not write {}: {error}", output.display()))?;
    println!(
        "Built {} ({} bytes, {}-byte native image)",
        output.display(),
        package.len(),
        image.len()
    );
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommunityCatalogEntry {
    app_id: u8,
    module: String,
    author: String,
    #[serde(default)]
    version: Option<String>,
}

fn build_community(options: BTreeMap<String, String>) -> Result<(), String> {
    let repository = required_path(&options, "repo")?;
    let output_dir = required_path(&options, "output")?;
    let default_version = options
        .get("version")
        .cloned()
        .unwrap_or_else(|| "0.1.0".into());
    parse_version(&default_version)?;
    let firmware_abi = firmware_abi_option(&options)?;

    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("fpapp crate has no workspace parent")?;
    let sdk = workspace.join("fpapp-sdk");
    let libfp = workspace.join("libfp");
    let linker = sdk.join("fpapp.x");
    let staging = workspace.join("target/fpapp-community-build");
    fs::create_dir_all(&staging)
        .map_err(|error| format!("could not create {}: {error}", staging.display()))?;
    // Every app gets its own `[workspace]` crate (see write_metadata_crate /
    // write_native_crate) so a stray Cargo.toml in one app's staging dir can't
    // pull in another's — but that also means dependency compilation (midly,
    // libm, embassy-*, ...) would otherwise happen from scratch per app, once
    // per catalog entry. Pointing every build at one shared target dir lets
    // Cargo's own artifact cache (keyed by package+features+version, not by
    // which workspace asked for it) amortize that across the whole catalog.
    let cargo_target_dir = staging.join("cargo-target");
    fs::create_dir_all(&cargo_target_dir)
        .map_err(|error| format!("could not create {}: {error}", cargo_target_dir.display()))?;
    fs::create_dir_all(&output_dir)
        .map_err(|error| format!("could not create {}: {error}", output_dir.display()))?;

    let catalog: Vec<CommunityCatalogEntry> = read_json(&repository.join("apps-catalog.json"))?;
    let manuals: Vec<JsonValue> = read_json(&repository.join("manual-tab.json"))?;
    if catalog.is_empty() {
        return Err("community catalog contains no apps".into());
    }

    for entry in &catalog {
        let source_path = repository.join("apps").join(format!("{}.rs", entry.module));
        let source = fs::read_to_string(&source_path)
            .map_err(|error| format!("could not read {}: {error}", source_path.display()))?;
        let transformed = transform_community_source(&source)?;
        let manual = manuals
            .iter()
            .find(|manual| {
                manual.get("appId").and_then(JsonValue::as_u64) == Some(entry.app_id.into())
            })
            .ok_or_else(|| format!("app {} has no manual-tab entry", entry.app_id))?;
        let title = json_string(manual, "title")?;
        let description = json_string(manual, "description")?;
        let color_name = json_string(manual, "color")?;
        let icon_name = json_string(manual, "icon")?;
        let version = entry.version.as_deref().unwrap_or(&default_version);
        parse_version(version)?;

        let app_root = staging.join(&entry.module);
        let metadata_root = app_root.join("metadata");
        let native_root = app_root.join("native");
        write_metadata_crate(&metadata_root, &transformed, &sdk, &libfp)?;
        let params = run_metadata_helper(&metadata_root, &cargo_target_dir)?;
        write_native_crate(&native_root, &transformed, &sdk, &libfp)?;
        build_native_crate(&native_root, &linker, &cargo_target_dir)?;

        let manual_path = app_root.join("manual.json");
        let setup_path = app_root.join("setup.md");
        let settings_path = app_root.join("settings.json");
        fs::write(&manual_path, render_manual(manual)?)
            .map_err(|error| format!("could not write {}: {error}", manual_path.display()))?;
        fs::write(&setup_path, render_setup(manual)?)
            .map_err(|error| format!("could not write {}: {error}", setup_path.display()))?;
        let settings = json!({
            "format": "faderpunk-app-config-v1",
            "app": {
                "color": color_name,
                "icon": icon_pascal(icon_name),
                "params": params,
            }
        });
        fs::write(
            &settings_path,
            serde_json::to_vec_pretty(&settings).map_err(|error| error.to_string())?,
        )
        .map_err(|error| format!("could not write {}: {error}", settings_path.display()))?;

        let binary_name = format!("fpapp_{}", entry.module);
        let elf = cargo_target_dir
            .join("thumbv8m.main-none-eabihf/release")
            .join(&binary_name);
        let output = output_dir.join(format!("{}.fpapp", entry.module.replace('_', "-")));
        let mut pack_options = BTreeMap::from([
            ("elf".into(), elf.display().to_string()),
            ("output".into(), output.display().to_string()),
            ("id".into(), entry.app_id.to_string()),
            ("version".into(), version.to_owned()),
            ("name".into(), title.to_owned()),
            ("description".into(), description.to_owned()),
            ("author".into(), entry.author.clone()),
            (
                "channels".into(),
                json_number(manual, "channels", |value| value.as_array().map(Vec::len))?
                    .to_string(),
            ),
            ("color".into(), color_hex(color_name)?.into()),
            ("icon".into(), icon_id(icon_name)?.to_string()),
            (
                "parameter-count".into(),
                params
                    .as_array()
                    .ok_or("community metadata helper did not return a parameter array")?
                    .len()
                    .to_string(),
            ),
            ("manual".into(), manual_path.display().to_string()),
            ("setup".into(), setup_path.display().to_string()),
            ("settings".into(), settings_path.display().to_string()),
            ("firmware-abi".into(), format_abi(&firmware_abi)),
        ]);
        if let Some(capabilities) = options.get("capabilities") {
            pack_options.insert("capabilities".into(), capabilities.clone());
        }
        pack(pack_options)?;
    }
    println!(
        "Built {} community FPApps in {}",
        catalog.len(),
        output_dir.display()
    );
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("could not parse {}: {error}", path.display()))
}

fn json_string<'a>(value: &'a JsonValue, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(JsonValue::as_str)
        .ok_or_else(|| format!("manual entry is missing string field {key}"))
}

fn json_number<F>(value: &JsonValue, key: &str, convert: F) -> Result<usize, String>
where
    F: FnOnce(&JsonValue) -> Option<usize>,
{
    value
        .get(key)
        .and_then(convert)
        .ok_or_else(|| format!("manual entry is missing field {key}"))
}

fn write_metadata_crate(root: &Path, source: &str, sdk: &Path, libfp: &Path) -> Result<(), String> {
    let src = root.join("src");
    fs::create_dir_all(&src)
        .map_err(|error| format!("could not create {}: {error}", src.display()))?;
    fs::write(src.join("community_app.rs"), source)
        .map_err(|error| format!("could not write metadata source: {error}"))?;
    fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[package]
name = "fpapp-community-metadata"
version = "0.0.0"
edition = "2021"

[dependencies]
embassy-futures = "0.1"
embassy-sync = "0.7"
embassy-time = "0.4.0"
fpapp-sdk = {{ path = "{}" }}
heapless = {{ version = "0.7.17", features = ["serde"] }}
libfp = {{ path = "{}" }}
libm = "0.2.16"
midly = {{ version = "0.5.3", default-features = false }}
portable-atomic = {{ version = "1.13.1", features = ["critical-section"] }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
smart-leds = "0.4.0"

[workspace]
"#,
            toml_path(sdk),
            toml_path(libfp)
        ),
    )
    .map_err(|error| format!("could not write metadata manifest: {error}"))?;
    fs::write(src.join("main.rs"), METADATA_HELPER)
        .map_err(|error| format!("could not write metadata helper: {error}"))?;
    Ok(())
}

fn write_native_crate(root: &Path, source: &str, sdk: &Path, libfp: &Path) -> Result<(), String> {
    let src = root.join("src");
    fs::create_dir_all(&src)
        .map_err(|error| format!("could not create {}: {error}", src.display()))?;
    fs::write(src.join("community_app.rs"), source)
        .map_err(|error| format!("could not write native app source: {error}"))?;
    let package_name = root
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .ok_or("native app staging path is invalid")?;
    let binary_name = format!("fpapp_{}", package_name.replace('-', "_"));
    fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[package]
name = "{binary_name}"
version = "0.0.0"
edition = "2021"

[dependencies]
embassy-futures = "0.1"
embassy-sync = "0.7"
embassy-time = "0.4.0"
fpapp-sdk = {{ path = "{}" }}
heapless = {{ version = "0.7.17", features = ["serde"] }}
libfp = {{ path = "{}" }}
libm = "0.2.16"
midly = {{ version = "0.5.3", default-features = false }}
portable-atomic = {{ version = "1.13.1", features = ["critical-section"] }}
serde = {{ version = "1", default-features = false, features = ["derive"] }}
smart-leds = "0.4.0"

[profile.release]
codegen-units = 1
lto = true
opt-level = "z"
panic = "abort"

[workspace]
"#,
            toml_path(sdk),
            toml_path(libfp)
        ),
    )
    .map_err(|error| format!("could not write native manifest: {error}"))?;
    fs::write(src.join("main.rs"), NATIVE_WRAPPER)
        .map_err(|error| format!("could not write native wrapper: {error}"))?;
    Ok(())
}

fn run_metadata_helper(root: &Path, cargo_target_dir: &Path) -> Result<JsonValue, String> {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--release"])
        .env("CARGO_TARGET_DIR", cargo_target_dir)
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not run community metadata helper: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "community metadata helper failed:\n{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("community metadata helper returned invalid JSON: {error}"))
}

fn build_native_crate(root: &Path, linker: &Path, cargo_target_dir: &Path) -> Result<(), String> {
    let output = Command::new("cargo")
        .arg("+nightly")
        .args([
            "build",
            "--release",
            "--target",
            "thumbv8m.main-none-eabihf",
        ])
        .env(
            "RUSTFLAGS",
            format!(
                "-C relocation-model=ropi -C panic=abort -C link-arg=-T{} -C link-arg=--emit-relocs",
                linker.display()
            ),
        )
        .env("CARGO_TARGET_DIR", cargo_target_dir)
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not compile community FPApp: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "community FPApp compile failed:\n{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

fn render_manual(manual: &JsonValue) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(&json!({
        "format": "faderpunk-manual-v1",
        "app": manual,
    }))
    .map_err(|error| format!("could not serialize structured manual: {error}"))
}

fn render_setup(manual: &JsonValue) -> Result<String, String> {
    let mut output = format!("# {} setup\n", json_string(manual, "title")?);
    if let Some(channels) = manual.get("channels").and_then(JsonValue::as_array) {
        for (index, channel) in channels.iter().enumerate() {
            if let Some(description) = channel.get("jackDescription").and_then(JsonValue::as_str) {
                output.push_str(&format!("\n- Channel {}: {}\n", index + 1, description));
            }
        }
    }
    Ok(output)
}

fn color_hex(color: &str) -> Result<&'static str, String> {
    match color {
        "White" => Ok("ffffff"),
        "Yellow" => Ok("ffe13b"),
        "Orange" => Ok("ff8a24"),
        "Red" => Ok("ff354f"),
        "Lime" => Ok("b7ef3f"),
        "Green" => Ok("39d98a"),
        "Cyan" => Ok("36e2e2"),
        "SkyBlue" => Ok("4bb8ff"),
        "Blue" => Ok("506cff"),
        "Violet" => Ok("9d5cff"),
        "Pink" => Ok("ff4fba"),
        "PaleGreen" => Ok("9ee6b8"),
        "Sand" => Ok("ddb878"),
        "Rose" => Ok("ff6685"),
        "Salmon" => Ok("ff8a78"),
        "LightBlue" => Ok("8fd4ff"),
        other => Err(format!("unsupported community app color {other:?}")),
    }
}

fn icon_id(icon: &str) -> Result<u8, String> {
    match icon {
        "fader" => Ok(0),
        "ad-env" => Ok(1),
        "random" => Ok(2),
        "euclid" => Ok(3),
        "attenuate" => Ok(4),
        "die" => Ok(5),
        "quantize" => Ok(6),
        "sequence" => Ok(7),
        "note" => Ok(8),
        "env-follower" => Ok(9),
        "soft-random" => Ok(10),
        "sine" => Ok(11),
        "note-box" => Ok(12),
        "sequence-square" => Ok(13),
        "note-grid" => Ok(14),
        "knob-round" => Ok(15),
        "stereo" => Ok(16),
        "sift" => Ok(17),
        other => Err(format!("unsupported community app icon {other:?}")),
    }
}

fn icon_pascal(icon: &str) -> String {
    icon.split('-')
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect()
}

fn toml_path(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

const NATIVE_WRAPPER: &str = r#"#![no_std]
#![no_main]

use core::future::Future;
use fpapp_sdk::HostV1;

mod app {
    pub use fpapp_sdk::compat::*;
}

mod tasks {
    pub mod leds {
        pub use fpapp_sdk::compat::LedMode;
    }
    pub mod global_config {
        pub use fpapp_sdk::compat::get_global_config;
    }
}

mod community_app;

fn app_future(host: *const HostV1) -> impl Future<Output = ()> {
    async move {
        let app = app::App::<{ community_app::CHANNELS }>::from_host(host);
        community_app::wrapper(app).await;
    }
}

fpapp_sdk::export_app!(app_future);

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {}
}
"#;

const METADATA_HELPER: &str = r#"mod app {
    pub use fpapp_sdk::compat::*;
}

mod tasks {
    pub mod leds {
        pub use fpapp_sdk::compat::LedMode;
    }
    pub mod global_config {
        pub use fpapp_sdk::compat::get_global_config;
    }
}

mod community_app;

use libfp::Param;
use serde_json::{Value, json};

fn tags<T: core::fmt::Debug>(values: &[T]) -> Value {
    Value::Array(values.iter().map(|value| json!({ "tag": format!("{value:?}") })).collect())
}

fn param(value: &Param) -> Value {
    match value {
        Param::None => json!({ "tag": "None" }),
        Param::i32 { name, min, max } => json!({ "tag": "i32", "value": { "name": name, "min": min, "max": max } }),
        Param::f32 { name, min, max } => json!({ "tag": "f32", "value": { "name": name, "min": min, "max": max } }),
        Param::bool { name } => json!({ "tag": "bool", "value": { "name": name } }),
        Param::Enum { name, variants } => json!({ "tag": "Enum", "value": { "name": name, "variants": variants } }),
        Param::Curve { name, variants } => json!({ "tag": "Curve", "value": { "name": name, "variants": tags(variants) } }),
        Param::Waveform { name, variants } => json!({ "tag": "Waveform", "value": { "name": name, "variants": tags(variants) } }),
        Param::Color { name, variants } => json!({ "tag": "Color", "value": { "name": name, "variants": tags(variants) } }),
        Param::Range { name, variants } => json!({ "tag": "Range", "value": { "name": name, "variants": tags(variants) } }),
        Param::Note { name, variants } => json!({ "tag": "Note", "value": { "name": name, "variants": tags(variants) } }),
        Param::MidiCc { name } => json!({ "tag": "MidiCc", "value": { "name": name } }),
        Param::MidiChannel { name } => json!({ "tag": "MidiChannel", "value": { "name": name } }),
        Param::MidiIn => json!({ "tag": "MidiIn" }),
        Param::MidiMode => json!({ "tag": "MidiMode" }),
        Param::MidiNote { name } => json!({ "tag": "MidiNote", "value": { "name": name } }),
        Param::MidiOut => json!({ "tag": "MidiOut" }),
        Param::MidiNrpn => json!({ "tag": "MidiNrpn" }),
        Param::VoltPerOct => json!({ "tag": "VoltPerOct" }),
    }
}

fn main() {
    let meta = community_app::CONFIG.get_meta();
    let params: Vec<Value> = meta.5.iter().map(param).collect();
    println!("{}", serde_json::to_string(&params).unwrap());
}
"#;

fn inspect(path: &Path) -> Result<(), String> {
    let bytes =
        fs::read(path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let package = Package::parse(&bytes).map_err(|error| format!("invalid package: {error:?}"))?;
    let native = package
        .native_program()
        .map_err(|error| format!("invalid native program: {error:?}"))?;
    let manifest = package.manifest;
    println!("File: {}", path.display());
    println!("App: {} ({})", manifest.name, manifest.app_id);
    println!(
        "Version: {}.{}.{}",
        manifest.version.major, manifest.version.minor, manifest.version.patch
    );
    println!("Author: {}", manifest.author);
    println!("Channels: {}", manifest.channels);
    println!("Parameters: {}", manifest.parameter_count);
    println!("Description: {}", manifest.description);
    println!("Firmware ABI: {}", format_abi(&manifest.firmware_abi));
    println!("Native image: {} bytes", native.image.len());
    println!(
        "Entrypoints: required=0x{:x} init=0x{:x} poll=0x{:x} drop=0x{:x}",
        native.entrypoints.required_bytes,
        native.entrypoints.init,
        native.entrypoints.poll,
        native.entrypoints.drop
    );
    println!("Manual: {}", yes_no(package.manual.is_some()));
    println!("Setup guide: {}", yes_no(package.setup.is_some()));
    println!("Settings hook: {}", yes_no(package.settings.is_some()));
    println!("Signature: {}", yes_no(package.signing.is_some()));
    Ok(())
}

fn verify(arguments: &[OsString]) -> Result<(), String> {
    let Some(path) = arguments.first() else {
        return Err("verify requires an .fpapp path".into());
    };
    let path = PathBuf::from(path);
    let options = parse_options(&arguments[1..])?;
    let bytes =
        fs::read(&path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let package = Package::parse(&bytes).map_err(|error| format!("invalid package: {error:?}"))?;
    package
        .native_program()
        .map_err(|error| format!("invalid native program: {error:?}"))?;
    if let Some(expected) = options.get("firmware-abi") {
        let expected = parse_abi(expected)?;
        if package.manifest.firmware_abi != expected {
            return Err(format!(
                "firmware ABI mismatch: package={}, expected={}",
                format_abi(&package.manifest.firmware_abi),
                format_abi(&expected)
            ));
        }
    }
    println!("Verified {}", path.display());
    Ok(())
}

fn verify_sections(readelf: &str) -> Result<(), String> {
    let mut found_text = false;
    for line in readelf.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        let Some(type_index) = fields
            .iter()
            .position(|field| *field == "PROGBITS" || *field == "NOBITS")
        else {
            continue;
        };
        if type_index == 0 || fields.len() <= type_index + 5 {
            continue;
        }
        let name = fields[type_index - 1];
        let address = fields[type_index + 1];
        let flags = fields[type_index + 5];
        if flags.contains('W') && flags.contains('A') {
            return Err(format!(
                "writable allocated ELF section is not allowed: {name}"
            ));
        }
        if name == ".text" {
            found_text = true;
            if address != "00000000" || !flags.contains('A') || !flags.contains('X') {
                return Err(".text must be executable, allocated, and linked at address 0".into());
            }
        }
    }
    if !found_text {
        return Err("ELF has no .text section".into());
    }
    Ok(())
}

fn verify_relocations(readelf: &str) -> Result<(), String> {
    for line in readelf.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        let Some(relocation_index) = fields.iter().position(|field| field.starts_with("R_ARM_"))
        else {
            continue;
        };
        let relocation = fields[relocation_index];
        if !matches!(
            relocation,
            "R_ARM_REL32" | "R_ARM_THM_CALL" | "R_ARM_THM_JUMP24"
        ) {
            let offset = fields.first().copied().unwrap_or("unknown offset");
            return Err(format!(
                "native image contains unsupported relocation {relocation} at {offset}; static pointers must be supplied by the firmware host ABI"
            ));
        }
    }
    Ok(())
}

fn symbol_offset(symbols: &str, expected: &str) -> Result<u32, String> {
    for line in symbols.lines() {
        let mut fields = line.split_whitespace();
        let Some(address) = fields.next() else {
            continue;
        };
        let _kind = fields.next();
        let Some(name) = fields.next() else { continue };
        if name == expected {
            return u32::from_str_radix(address, 16)
                .map_err(|_| format!("invalid address for {expected}"));
        }
    }
    Err(format!("ELF is missing required export {expected}"))
}

fn tool_output(tool: &str, arguments: &[OsString]) -> Result<String, String> {
    let output = Command::new(tool)
        .args(arguments)
        .output()
        .map_err(|error| format!("could not run {tool}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{tool} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|_| format!("{tool} returned non-UTF-8 output"))
}

fn parse_options(arguments: &[OsString]) -> Result<BTreeMap<String, String>, String> {
    let mut options = BTreeMap::new();
    let mut index = 0;
    while index < arguments.len() {
        let key = arguments[index]
            .to_str()
            .ok_or("option name is not UTF-8")?;
        if !key.starts_with("--") {
            return Err(format!("unexpected positional argument {key:?}"));
        }
        let value = arguments
            .get(index + 1)
            .and_then(|value| value.to_str())
            .ok_or_else(|| format!("{key} requires a UTF-8 value"))?;
        options.insert(key[2..].to_owned(), value.to_owned());
        index += 2;
    }
    Ok(options)
}

fn positional_path(arguments: &[OsString], command: &str) -> Result<PathBuf, String> {
    if arguments.len() != 1 {
        return Err(format!("{command} requires exactly one path"));
    }
    Ok(PathBuf::from(&arguments[0]))
}

fn required<'a>(options: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, String> {
    options
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("missing --{key}"))
}

fn required_path(options: &BTreeMap<String, String>, key: &str) -> Result<PathBuf, String> {
    required(options, key).map(PathBuf::from)
}

fn parse_number<T: std::str::FromStr>(value: &str, name: &str) -> Result<T, String> {
    value
        .parse()
        .map_err(|_| format!("invalid --{name} value {value:?}"))
}

fn parse_optional_number<T: std::str::FromStr + Copy>(
    options: &BTreeMap<String, String>,
    key: &str,
    default: T,
) -> Result<T, String> {
    options
        .get(key)
        .map(|value| parse_number(value, key))
        .transpose()
        .map(|value| value.unwrap_or(default))
}

fn parse_optional_hex_u32(
    options: &BTreeMap<String, String>,
    key: &str,
    default: u32,
) -> Result<u32, String> {
    options
        .get(key)
        .map(|value| parse_hex_u32(value, key))
        .transpose()
        .map(|value| value.unwrap_or(default))
}

fn parse_hex_u32(value: &str, name: &str) -> Result<u32, String> {
    let value = value.trim_start_matches('#').trim_start_matches("0x");
    u32::from_str_radix(value, 16).map_err(|_| format!("invalid hexadecimal --{name}"))
}

fn parse_version(value: &str) -> Result<Version, String> {
    let mut fields = value.split('.');
    let major = parse_number(
        fields.next().ok_or("version requires major.minor.patch")?,
        "version",
    )?;
    let minor = parse_number(
        fields.next().ok_or("version requires major.minor.patch")?,
        "version",
    )?;
    let patch = parse_number(
        fields.next().ok_or("version requires major.minor.patch")?,
        "version",
    )?;
    if fields.next().is_some() {
        return Err("version requires major.minor.patch".into());
    }
    Ok(Version::new(major, minor, patch))
}

fn parse_abi(value: &str) -> Result<[u8; 32], String> {
    if value.len() != 64 || !value.is_ascii() {
        return Err("firmware ABI must contain exactly 64 hexadecimal digits".into());
    }
    let mut output = [0u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| "firmware ABI contains a non-hexadecimal digit".to_owned())?;
    }
    Ok(output)
}

fn firmware_abi_option(options: &BTreeMap<String, String>) -> Result<[u8; 32], String> {
    match (
        options.get("firmware-abi"),
        options.get("firmware-revision"),
    ) {
        (Some(_), Some(_)) => Err("use only one of --firmware-abi or --firmware-revision".into()),
        (Some(abi), None) => parse_abi(abi),
        (None, Some(revision)) => firmware_abi_from_revision(revision)
            .ok_or_else(|| "firmware revision must contain 40 hexadecimal digits".into()),
        (None, None) => Err("missing --firmware-revision (or explicit --firmware-abi)".into()),
    }
}

fn format_abi(value: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(64);
    for byte in value {
        write!(&mut output, "{byte:02x}").unwrap();
    }
    output
}

fn read_optional_text(
    options: &BTreeMap<String, String>,
    key: &str,
) -> Result<Option<String>, String> {
    options
        .get(key)
        .map(|path| {
            fs::read_to_string(path).map_err(|error| format!("could not read {path}: {error}"))
        })
        .transpose()
}

fn read_optional_bytes(
    options: &BTreeMap<String, String>,
    key: &str,
) -> Result<Option<Vec<u8>>, String> {
    options
        .get(key)
        .map(|path| fs::read(path).map_err(|error| format!("could not read {path}: {error}")))
        .transpose()
}

fn temporary_image_path(output: &Path) -> PathBuf {
    let mut path = output.as_os_str().to_owned();
    path.push(".image.tmp");
    PathBuf::from(path)
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn transform_community_source(source: &str) -> Result<String, String> {
    let mut output = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("#[embassy_executor::task"))
        .collect::<Vec<_>>()
        .join("\n");
    let original_signature = "pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>)";
    if !output.contains(original_signature) {
        return Err("community app is missing the standard wrapper signature".into());
    }
    output = output.replace(
        original_signature,
        "pub async fn wrapper(app: App<CHANNELS>)",
    );
    output = output.replace(
        "ParamStore::<Params>::new(",
        "ParamStore::<Params>::new(app.host(), ",
    );
    output = output.replace(
        "ManagedStorage::<Storage>::new(",
        "ManagedStorage::<Storage>::new(app.host(), ",
    );
    output = output.replace("get_global_config()", "app.global_config()");
    let exit_select = "select(app_loop, app.exit_handler(exit_signal)).await;";
    if !output.contains(exit_select) {
        return Err("community app wrapper is missing the standard exit select".into());
    }
    output = output.replace(exit_select, "app_loop.await;");
    output.push('\n');
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_required_native_symbols() {
        let symbols = "00000000 T fpapp_init\n00000060 T fpapp_poll\n00000094 T fpapp_drop\n000000da T fpapp_required_bytes\n";
        assert_eq!(symbol_offset(symbols, "fpapp_init"), Ok(0));
        assert_eq!(symbol_offset(symbols, "fpapp_required_bytes"), Ok(0xda));
    }

    #[test]
    fn rejects_writable_allocated_sections() {
        let sections = "  [ 1] .text PROGBITS 00000000 010000 00014c 00 AX 0 0 4\n  [ 2] .data PROGBITS 0000014c 01014c 000004 00 WA 0 0 4\n";
        assert!(verify_sections(sections).unwrap_err().contains(".data"));
    }

    #[test]
    fn rejects_absolute_relocations_but_allows_position_relative_ones() {
        let relocations = r#"
Relocation section '.rel.text' at offset 0x100 contains 3 entries:
 Offset     Info    Type                Sym. Value  Symbol's Name
00000020  0000010a R_ARM_THM_CALL        00000040   helper
00000024  00000203 R_ARM_REL32           00000080   table
00000080  00000302 R_ARM_ABS32           00000041   callback
"#;
        let error = verify_relocations(relocations).unwrap_err();
        assert!(error.contains("R_ARM_ABS32"));
        assert!(error.contains("00000080"));

        let relative_only = relocations
            .lines()
            .filter(|line| !line.contains("R_ARM_ABS32"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(verify_relocations(&relative_only), Ok(()));
    }

    #[test]
    fn abi_hex_round_trips() {
        let text = "0123456789abcdef".repeat(4);
        assert_eq!(format_abi(&parse_abi(&text).unwrap()), text);
    }

    #[test]
    fn community_source_transform_keeps_app_logic_and_retargets_only_the_runtime_shell() {
        let source = r#"
#[embassy_executor::task(pool_size = 4)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let params = ParamStore::<Params>::new(app.app_id, app.layout_id, Params::default());
    let storage = ManagedStorage::<Storage>::new(app.app_id, app.layout_id);
    let swing = get_global_config().clock.swing_amount;
    let app_loop = async { run(&app, &params, &storage, swing).await; };
    select(app_loop, app.exit_handler(exit_signal)).await;
}
"#;
        let transformed = transform_community_source(source).unwrap();

        assert!(!transformed.contains("embassy_executor::task"));
        assert!(transformed.contains("pub async fn wrapper(app: App<CHANNELS>)"));
        assert!(transformed.contains("ParamStore::<Params>::new(app.host(), app.app_id"));
        assert!(transformed.contains("ManagedStorage::<Storage>::new(app.host(), app.app_id"));
        assert!(transformed.contains("app.global_config().clock.swing_amount"));
        assert!(transformed.contains("app_loop.await;"));
        assert!(transformed.contains("run(&app, &params, &storage, swing).await"));
    }

    #[test]
    fn community_manual_keeps_the_structured_manual_app_shape() {
        let source = json!({
            "appId": 102,
            "title": "Sift",
            "description": "Two-channel sequencer",
            "icon": "sift",
            "color": "Rose",
            "text": "Manual body",
            "params": ["CV Steps"],
            "storage": ["Pattern banks"],
            "channels": [{
                "jackTitle": "Gate",
                "jackDescription": "Gate output",
                "faderTitle": "Gate density",
                "faderDescription": "Threshold",
                "ledTop": "Step",
                "ledBottom": "Density"
            }]
        });

        let bytes = render_manual(&source).unwrap();
        let document: JsonValue = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(document["format"], "faderpunk-manual-v1");
        assert_eq!(document["app"], source);
        assert_eq!(document["app"]["channels"][0]["jackTitle"], "Gate");
    }

    #[test]
    fn sift_icon_keeps_its_appended_discriminant() {
        assert_eq!(icon_id("sift"), Ok(17));
        assert_eq!(icon_pascal("sift"), "Sift");
    }
}
