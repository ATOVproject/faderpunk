@echo off
setlocal enabledelayedexpansion

set "ROOT_DIR=%~dp0"

set "FP_SIM_CARGO=%ROOT_DIR%toolchain\bin\cargo.exe"
set "RUSTC=%ROOT_DIR%toolchain\bin\rustc.exe"
set "PATH=%ROOT_DIR%toolchain\bin;%ROOT_DIR%toolchain\lib\rustlib\x86_64-pc-windows-gnu\bin;%PATH%"

set "CARGO_HOME=%ROOT_DIR%cache\cargo-home"
set "CARGO_TARGET_DIR=%ROOT_DIR%cache\target"
set FP_SIM_CARGO_FROZEN=1
set "FP_SIM_FRAM=%ROOT_DIR%cache\fp-sim-fram.bin"
set "FP_SIM_PANEL_STATE=%ROOT_DIR%cache\fp-sim-panel.json"

if not exist "%ROOT_DIR%cache\cargo-home" (
    mkdir "%ROOT_DIR%cache\cargo-home"
)

"%ROOT_DIR%bin\fp-sim.exe" --project "%ROOT_DIR%workshop-app" %*
