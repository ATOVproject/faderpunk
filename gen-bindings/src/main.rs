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

    catalog::generate(
        &repo_root.join("faderpunk").join("src"),
        &repo_root
            .join("configurator")
            .join("src")
            .join("demo")
            .join("catalog.ts"),
    );
}
