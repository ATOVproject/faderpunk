#!/usr/bin/env python3
"""
package-simulator-workshop.py

Packages a self-contained, offline workshop bundle of the Faderpunk simulator
with bundled Rust toolchain, vendored dependencies, launcher scripts, and starter project.
"""

import argparse
import json
import os
import platform as py_platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path

PLATFORMS = {
    "macos-aarch64": {
        "host_triple": "aarch64-apple-darwin",
        "child_target": "aarch64-apple-darwin",
        "binary_name": "fp-sim",
        "launcher_template": "launch-macos.command",
        "launcher_name": "launch-macos.command",
        "archive_ext": ".tar.gz",
        "sysroot_bin_subpaths": ["lib/rustlib/aarch64-apple-darwin/bin"],
    },
    "linux-x86_64": {
        "host_triple": "x86_64-unknown-linux-gnu",
        "child_target": "x86_64-unknown-linux-musl",
        "binary_name": "fp-sim",
        "launcher_template": "launch-linux.sh",
        "launcher_name": "launch-linux.sh",
        "archive_ext": ".tar.gz",
        "sysroot_bin_subpaths": ["lib/rustlib/x86_64-unknown-linux-gnu/bin"],
    },
    "windows-x86_64": {
        "host_triple": "x86_64-pc-windows-gnu",
        "child_target": "x86_64-pc-windows-gnu",
        "binary_name": "fp-sim.exe",
        "launcher_template": "launch-windows.cmd",
        "launcher_name": "launch-windows.cmd",
        "archive_ext": ".zip",
        "sysroot_bin_subpaths": ["lib/rustlib/x86_64-pc-windows-gnu/bin"],
    },
}


def log(msg: str) -> None:
    print(f"==> {msg}", flush=True)


def error(msg: str) -> None:
    print(f"Error: {msg}", file=sys.stderr, flush=True)
    sys.exit(1)


def copy_tree_filtered(src: Path, dst: Path, ignore_dirs: set[str] | None = None) -> None:
    if ignore_dirs is None:
        ignore_dirs = {"target", ".git"}
    
    def _ignore(directory: str, contents: list[str]) -> set[str]:
        ignored = set()
        for item in contents:
            if item in ignore_dirs:
                ignored.add(item)
        return ignored

    shutil.copytree(src, dst, ignore=_ignore, symlinks=True)


def validate_toolchain(toolchain_path: Path, platform_name: str, expected_rust_version: str) -> Path:
    cfg = PLATFORMS[platform_name]
    host_triple = cfg["host_triple"]
    is_win = platform_name == "windows-x86_64"

    rustc_name = "rustc.exe" if is_win else "rustc"
    cargo_name = "cargo.exe" if is_win else "cargo"

    rustc_bin = toolchain_path / "bin" / rustc_name
    cargo_bin = toolchain_path / "bin" / cargo_name

    if not rustc_bin.is_file():
        error(f"rustc binary not found at {rustc_bin}")
    if not cargo_bin.is_file():
        error(f"cargo binary not found at {cargo_bin}")

    # Validate rustc -Vv
    try:
        proc = subprocess.run(
            [str(rustc_bin), "-Vv"],
            capture_output=True,
            text=True,
            check=True,
        )
    except Exception as e:
        error(f"Failed to run rustc -Vv: {e}")

    lines = proc.stdout.splitlines()
    release_val = None
    host_val = None
    for line in lines:
        if line.startswith("release:"):
            release_val = line.split(":", 1)[1].strip()
        elif line.startswith("host:"):
            host_val = line.split(":", 1)[1].strip()

    if not release_val or not release_val.startswith(expected_rust_version):
        error(
            f"Rustc version mismatch: expected {expected_rust_version}, got {release_val}"
        )

    if host_val != host_triple:
        error(f"Rustc host triple mismatch: expected {host_triple}, got {host_val}")

    # Validate host standard library
    host_lib = toolchain_path / "lib" / "rustlib" / host_triple / "lib"
    if not host_lib.is_dir() or not any(host_lib.iterdir()):
        error(f"Host standard library missing or empty at {host_lib}")

    # Linux-specific checks
    if platform_name == "linux-x86_64":
        musl_lib = toolchain_path / "lib" / "rustlib" / "x86_64-unknown-linux-musl" / "lib"
        if not musl_lib.is_dir() or not any(musl_lib.iterdir()):
            error(f"Target musl standard library missing at {musl_lib}")
        rust_lld = toolchain_path / "lib" / "rustlib" / host_triple / "bin" / "rust-lld"
        if not rust_lld.is_file() and not (toolchain_path / "bin" / "rust-lld").is_file():
            error(f"rust-lld linker missing from toolchain ({rust_lld})")

    # Windows-specific checks
    if platform_name == "windows-x86_64":
        win_bin = toolchain_path / "lib" / "rustlib" / host_triple / "bin"
        if not win_bin.is_dir() or not (win_bin / "gcc.exe").is_file() or not (win_bin / "ld.exe").is_file():
            error(f"rust-mingw linker/runtime missing from {win_bin}")

    return cargo_bin


def package_workshop(
    platform_name: str,
    fp_sim_binary: Path,
    toolchain_path: Path,
    rust_version: str,
    output_dir: Path,
) -> Path:
    if platform_name not in PLATFORMS:
        error(f"Unsupported platform: {platform_name}. Must be one of {list(PLATFORMS.keys())}")

    cfg = PLATFORMS[platform_name]
    repo_root = Path(__file__).resolve().parent.parent

    if not fp_sim_binary.is_file():
        error(f"fp-sim binary not found: {fp_sim_binary}")
    if not (repo_root / "Cargo.lock").is_file():
        error(f"Cargo.lock not found at repository root: {repo_root / 'Cargo.lock'}")
    if not (repo_root / "LICENSE").is_file():
        error(f"LICENSE not found at repository root: {repo_root / 'LICENSE'}")

    launcher_src = repo_root / "fp-sim" / "packaging" / cfg["launcher_template"]
    if not launcher_src.is_file():
        error(f"Launcher template not found: {launcher_src}")

    cargo_bin = validate_toolchain(toolchain_path, platform_name, rust_version)

    bundle_name = f"faderpunk-sim-workshop-{platform_name}"
    staging_temp = Path(tempfile.mkdtemp(prefix="fp-workshop-staging-"))
    stage_dir = staging_temp / bundle_name

    try:
        log(f"Staging workshop directory: {stage_dir}")
        stage_dir.mkdir(parents=True)

        # 1. Copy fp-sim binary to bin/
        bin_dir = stage_dir / "bin"
        bin_dir.mkdir()
        dest_binary = bin_dir / cfg["binary_name"]
        shutil.copy2(fp_sim_binary, dest_binary)
        if platform_name != "windows-x86_64":
            dest_binary.chmod(0o755)

        # 2. Copy complete sysroot to toolchain/
        log("Copying toolchain sysroot...")
        copy_tree_filtered(toolchain_path, stage_dir / "toolchain")

        # 3. Copy starter app
        log("Copying workshop-app starter...")
        copy_tree_filtered(repo_root / "fp-sim-app-example", stage_dir / "workshop-app")

        # 4. Copy sibling crates and faderpunk/Cargo.toml
        log("Copying shared simulator crates...")
        copy_tree_filtered(repo_root / "fp-core", stage_dir / "fp-core")
        copy_tree_filtered(repo_root / "fp-sim-core", stage_dir / "fp-sim-core")
        copy_tree_filtered(repo_root / "fp-sim-protocol", stage_dir / "fp-sim-protocol")
        copy_tree_filtered(repo_root / "libfp", stage_dir / "libfp")

        faderpunk_dir = stage_dir / "faderpunk"
        faderpunk_dir.mkdir()
        shutil.copy2(repo_root / "faderpunk" / "Cargo.toml", faderpunk_dir / "Cargo.toml")

        # 5. Copy README, LICENSE, launcher
        shutil.copy2(repo_root / "LICENSE", stage_dir / "LICENSE")
        shutil.copy2(repo_root / "fp-sim" / "README.md", stage_dir / "README.md")

        dest_launcher = stage_dir / cfg["launcher_name"]
        shutil.copy2(launcher_src, dest_launcher)
        if platform_name != "windows-x86_64":
            dest_launcher.chmod(0o755)

        # 6. Create cache directories
        (stage_dir / "cache" / "cargo-home").mkdir(parents=True)
        (stage_dir / "cache" / "target").mkdir(parents=True)

        # 7. Vendor dependencies
        log("Vendoring Cargo dependencies...")
        vendor_dir = stage_dir / "vendor"
        vendor_cmd = [
            str(cargo_bin),
            "vendor",
            "--locked",
            "--versioned-dirs",
            "--manifest-path",
            str(stage_dir / "workshop-app" / "Cargo.toml"),
            str(vendor_dir),
        ]
        proc = subprocess.run(vendor_cmd, capture_output=True, text=True)
        if proc.returncode != 0:
            error(f"cargo vendor failed:\n{proc.stderr}\n{proc.stdout}")

        # 8. Create workshop-app/.cargo/config.toml
        dot_cargo = stage_dir / "workshop-app" / ".cargo"
        dot_cargo.mkdir(parents=True, exist_ok=True)
        
        config_content = [
            "[source.crates-io]",
            'replace-with = "vendored-sources"',
            "",
            "[source.vendored-sources]",
            'directory = "../vendor"',
        ]

        if platform_name == "linux-x86_64":
            config_content.extend([
                "",
                "[build]",
                'target = "x86_64-unknown-linux-musl"',
                "",
                "[target.x86_64-unknown-linux-musl]",
                'linker = "rust-lld"',
            ])

        (dot_cargo / "config.toml").write_text("\n".join(config_content) + "\n")

        # 9. Warm cache and check relocation
        log("Warming target cache...")
        is_win = platform_name == "windows-x86_64"
        staged_rustc = stage_dir / "toolchain" / "bin" / ("rustc.exe" if is_win else "rustc")
        staged_cargo = stage_dir / "toolchain" / "bin" / ("cargo.exe" if is_win else "cargo")

        env = os.environ.copy()
        env["CARGO_HOME"] = str(stage_dir / "cache" / "cargo-home")
        env["CARGO_TARGET_DIR"] = str(stage_dir / "cache" / "target")
        env["FP_SIM_CARGO_FROZEN"] = "1"
        env["RUSTC"] = str(staged_rustc)

        path_elements = [str(stage_dir / "toolchain" / "bin")]
        for sub in cfg["sysroot_bin_subpaths"]:
            path_elements.append(str(stage_dir / "toolchain" / sub))
        env["PATH"] = os.pathsep.join(path_elements) + os.pathsep + env.get("PATH", "")

        warm_cmd = [
            str(staged_cargo),
            "build",
            "--manifest-path",
            str(stage_dir / "workshop-app" / "Cargo.toml"),
            "--message-format=json",
            "--frozen",
        ]

        warm_proc = subprocess.run(
            warm_cmd,
            cwd=str(stage_dir / "workshop-app"),
            env=env,
            capture_output=True,
            text=True,
        )
        if warm_proc.returncode != 0:
            error(f"Cache warm build failed:\n{warm_proc.stderr}\n{warm_proc.stdout}")

        # Test relocation
        log("Testing target cache relocation...")
        reloc_temp = Path(tempfile.mkdtemp(prefix="fp-workshop-reloc-"))
        try:
            reloc_stage = reloc_temp / bundle_name
            shutil.copytree(stage_dir, reloc_stage, symlinks=True)

            reloc_rustc = reloc_stage / "toolchain" / "bin" / ("rustc.exe" if is_win else "rustc")
            reloc_cargo = reloc_stage / "toolchain" / "bin" / ("cargo.exe" if is_win else "cargo")

            reloc_env = os.environ.copy()
            reloc_env["CARGO_HOME"] = str(reloc_stage / "cache" / "cargo-home")
            reloc_env["CARGO_TARGET_DIR"] = str(reloc_stage / "cache" / "target")
            reloc_env["FP_SIM_CARGO_FROZEN"] = "1"
            reloc_env["RUSTC"] = str(reloc_rustc)

            reloc_paths = [str(reloc_stage / "toolchain" / "bin")]
            for sub in cfg["sysroot_bin_subpaths"]:
                reloc_paths.append(str(reloc_stage / "toolchain" / sub))
            reloc_env["PATH"] = os.pathsep.join(reloc_paths) + os.pathsep + reloc_env.get("PATH", "")

            reloc_cmd = [
                str(reloc_cargo),
                "build",
                "--manifest-path",
                str(reloc_stage / "workshop-app" / "Cargo.toml"),
                "--message-format=json",
                "--frozen",
            ]

            reloc_proc = subprocess.run(
                reloc_cmd,
                cwd=str(reloc_stage / "workshop-app"),
                env=reloc_env,
                capture_output=True,
                text=True,
            )

            all_deps_fresh = True
            if reloc_proc.returncode != 0:
                all_deps_fresh = False
            else:
                for line in reloc_proc.stdout.splitlines():
                    try:
                        msg = json.loads(line)
                    except Exception:
                        continue
                    if msg.get("reason") == "compiler-artifact":
                        target_name = msg.get("target", {}).get("name")
                        fresh = msg.get("fresh", False)
                        if target_name != "fp-sim-app-example" and not fresh:
                            log(f"Dependency artifact '{target_name}' was not fresh upon relocation")
                            all_deps_fresh = False

            if all_deps_fresh:
                log("Target cache verified relocatable and warm.")
            else:
                log("Notice: Target cache is not relocatable. Clearing target cache seed.")
                shutil.rmtree(stage_dir / "cache" / "target")
                (stage_dir / "cache" / "target").mkdir(parents=True)
        finally:
            shutil.rmtree(reloc_temp, ignore_errors=True)

        # 10. Create output archive
        output_dir.mkdir(parents=True, exist_ok=True)
        archive_path = output_dir / f"{bundle_name}{cfg['archive_ext']}"
        log(f"Creating archive: {archive_path}")

        if cfg["archive_ext"] == ".tar.gz":
            with tarfile.open(archive_path, "w:gz") as tar:
                tar.add(stage_dir, arcname=bundle_name)
        elif cfg["archive_ext"] == ".zip":
            with zipfile.ZipFile(archive_path, "w", zipfile.ZIP_DEFLATED) as zipf:
                for root, dirs, files in os.walk(stage_dir):
                    for file in files:
                        full_path = Path(root) / file
                        rel_path = full_path.relative_to(staging_temp)
                        zipf.write(full_path, arcname=str(rel_path))
        else:
            error(f"Unknown archive extension: {cfg['archive_ext']}")

        log(f"Successfully packaged {archive_path} ({archive_path.stat().st_size / (1024 * 1024):.1f} MB)")
        return archive_path

    finally:
        shutil.rmtree(staging_temp, ignore_errors=True)


def main() -> None:
    parser = argparse.ArgumentParser(description="Package Faderpunk Simulator Workshop Bundle")
    parser.add_argument(
        "--platform",
        required=True,
        choices=list(PLATFORMS.keys()),
        help="Target platform bundle to assemble",
    )
    parser.add_argument(
        "--fp-sim",
        required=True,
        type=Path,
        help="Path to prebuilt fp-sim release executable",
    )
    parser.add_argument(
        "--toolchain",
        required=True,
        type=Path,
        help="Path to rustc sysroot",
    )
    parser.add_argument(
        "--rust-version",
        required=True,
        default="1.97.1",
        help="Expected Rust toolchain version (e.g. 1.97.1)",
    )
    parser.add_argument(
        "--output",
        default=Path("dist"),
        type=Path,
        help="Output directory for archives (default: dist)",
    )

    args = parser.parse_args()
    package_workshop(
        platform_name=args.platform,
        fp_sim_binary=args.fp_sim.resolve(),
        toolchain_path=args.toolchain.resolve(),
        rust_version=args.rust_version,
        output_dir=args.output.resolve(),
    )


if __name__ == "__main__":
    main()
