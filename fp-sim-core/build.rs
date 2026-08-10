//! Mirrors the `faderpunk` crate version into the simulator so it reports
//! the same firmware version to the configurator as the hardware would.

use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let manifest = fs::read_to_string("../faderpunk/Cargo.toml")
        .expect("failed to read ../faderpunk/Cargo.toml");

    // The `version = "x.y.z"` line in [package] (dependency tables inline
    // their version keys, so a line starting with `version` is unambiguous).
    let version = manifest
        .lines()
        .map(str::trim)
        .find_map(|line| {
            line.strip_prefix("version")?
                .trim_start()
                .strip_prefix('=')?
                .trim()
                .strip_prefix('"')?
                .strip_suffix('"')
        })
        .expect("no version in faderpunk/Cargo.toml");

    // Take leading digits per component so prerelease suffixes
    // (e.g. "1.12.0-beta.1") parse as (1, 12, 0).
    let mut parts = version.split('.').map(|p| {
        let digits: String = p.chars().take_while(char::is_ascii_digit).collect();
        digits
            .parse::<u8>()
            .unwrap_or_else(|_| panic!("non-numeric version component in {version:?}"))
    });
    let (major, minor, patch) = (
        parts.next().expect("missing major version"),
        parts.next().expect("missing minor version"),
        parts.next().expect("missing patch version"),
    );

    let out_dir = env::var("OUT_DIR").unwrap();
    fs::write(
        Path::new(&out_dir).join("firmware_version.rs"),
        format!(
            "/// Firmware version reported to the configurator, mirrored from\n\
             /// `faderpunk/Cargo.toml` by the build script.\n\
             pub const FIRMWARE_VERSION: (u8, u8, u8) = ({major}, {minor}, {patch});\n"
        ),
    )
    .unwrap();

    println!("cargo:rerun-if-changed=../faderpunk/Cargo.toml");
}
