use postcard_bindgen::{generate_bindings, javascript, PackageInfo};

mod catalog;

fn main() {
    let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();

    javascript::build_package(
        repo_root
            .join("configurator")
            .join("node_modules")
            .as_path(),
        PackageInfo {
            name: "@atov/fp-config".into(),
            version: "0.1.0".try_into().unwrap(),
        },
        javascript::GenerationSettings::enable_all(),
        generate_bindings!(
            libfp::AppIcon,
            libfp::AuxJackMode,
            libfp::ClockConfig,
            libfp::ClockDivision,
            libfp::ClockSrc,
            libfp::Color,
            libfp::ConfigMsgIn,
            libfp::ConfigMsgOut,
            libfp::Curve,
            libfp::CustomVoOctCurve,
            libfp::FpAppSection,
            libfp::FpAppStatus,
            libfp::GlobalConfig,
            libfp::I2cMode,
            libfp::Key,
            libfp::Layout,
            libfp::MidiCc,
            libfp::MidiChannel,
            libfp::MidiConfig,
            libfp::MidiIn,
            libfp::MidiMode,
            libfp::MidiNote,
            libfp::MidiOut,
            libfp::MidiOutConfig,
            libfp::MidiOutMode,
            libfp::Note,
            libfp::Param,
            libfp::QuantizerConfig,
            libfp::Range,
            libfp::ResetSrc,
            libfp::TakeoverMode,
            libfp::Value,
            libfp::VoltPerOct,
            libfp::Waveform
        ),
    )
    .unwrap();

    patch_string_codec(
        &repo_root
            .join("configurator")
            .join("node_modules")
            .join("@atov")
            .join("fp-config")
            .join("index.js"),
    );

    catalog::generate(
        &repo_root.join("faderpunk").join("src"),
        &repo_root
            .join("configurator")
            .join("src")
            .join("demo")
            .join("catalog.ts"),
    );
}

/// Rewrite `postcard-bindgen`'s string codec to use UTF-8.
///
/// The generated codec is Latin-1 in both directions: it decodes with
/// `String.fromCharCode` per byte, so a UTF-8 `é` (`C3 A9`) renders as `Ã©`, and
/// it encodes by pushing `charCodeAt(0)` per character behind a *character*
/// count, where postcard expects a UTF-8 byte length followed by UTF-8 bytes.
///
/// That went unnoticed while every app name was ASCII — enforced at compile time
/// by `Config::new` since the "reject non-ASCII app names" change. Installable
/// apps may carry UTF-8 manifests, so device-reported names now reach this codec
/// and come back mangled.
///
/// Patching generated output is not lovely, but the alternative is an upstream
/// release of `postcard-bindgen` we do not control. The replacement asserts on
/// the exact source it expects, so an upstream change breaks the build loudly
/// rather than silently restoring the bug.
fn patch_string_codec(index_js: &std::path::Path) {
    const OLD_SER: &str = r#"serialize_string = (str) => { this.push_n(varint(U32_BYTES, str.length)); const bytes = []; for (const c of str) { bytes.push(c.charCodeAt(0)) } this.push_n(bytes) }"#;
    const NEW_SER: &str = r#"serialize_string = (str) => { const bytes = new TextEncoder().encode(str); this.push_n(varint(U32_BYTES, bytes.length)); this.push_n([...bytes]) }"#;
    const OLD_DE: &str = r#"deserialize_string = () => { const str = this.pop_n(Number(this.try_take(U32_BYTES))); return String.fromCharCode(...str) }"#;
    const NEW_DE: &str = r#"deserialize_string = () => { const str = this.pop_n(Number(this.try_take(U32_BYTES))); return new TextDecoder().decode(new Uint8Array(str)) }"#;

    let source = std::fs::read_to_string(index_js)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", index_js.display()));

    for (needle, what) in [(OLD_SER, "serialize_string"), (OLD_DE, "deserialize_string")] {
        assert!(
            source.contains(needle),
            "gen-bindings: {what} in {} no longer matches the source this patch expects. \
             postcard-bindgen's codec changed — re-check whether it handles UTF-8 now \
             (drop this patch if so) and update the expected text otherwise.",
            index_js.display()
        );
    }

    let patched = source.replace(OLD_SER, NEW_SER).replace(OLD_DE, NEW_DE);
    std::fs::write(index_js, patched)
        .unwrap_or_else(|error| panic!("could not write {}: {error}", index_js.display()));
}
