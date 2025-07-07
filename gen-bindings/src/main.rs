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
            config::ConfigMsgIn,
            config::ConfigMsgOut,
            config::Param,
            config::Curve,
            config::Waveform,
            config::ClockSrc,
            config::Value,
            config::GlobalConfig,
            config::Layout
        ),
    )
    .unwrap();
}
