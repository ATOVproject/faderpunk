use postcard_bindgen::{generate_bindings, javascript, PackageInfo};

fn main() {
    javascript::build_package(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
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
            libfp::MidiIn,
            libfp::MidiMode,
            libfp::MidiNote,
            libfp::MidiOut,
            libfp::Note,
            libfp::Param,
            libfp::QuantizerConfig,
            libfp::Range,
            libfp::ResetSrc,
            libfp::Value,
            libfp::Waveform
        ),
    )
    .unwrap();
}
