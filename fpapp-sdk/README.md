# fpapp-sdk

`fpapp-sdk` is the experimental, allocation-free Rust interface for native
Faderpunk `.fpapp` programs. Apps compile to position-independent Thumb code
and are loaded only by the exact firmware revision named in their package.
They are trusted native code, not sandboxed plugins.

The smallest app is an async factory plus one export macro:

```rust
#![no_std]
#![no_main]

use core::future::Future;
use fpapp_sdk::{EventReader, HostV1};

fn app(host: *const HostV1) -> impl Future<Output = ()> {
    async move {
        let mut events = unsafe { EventReader::new(host) };
        loop {
            let event = events.next_event().await;
            unsafe {
                ((*host).set_output)((*host).context, event.channel, event.value)
            };
        }
    }
}

fpapp_sdk::export_app!(app);

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop {}
}
```

## Build a native ELF

From the Faderpunk workspace root, the included probe is built with:

```sh
RUSTFLAGS='-C relocation-model=ropi -C panic=abort -C link-arg=-Tfpapp-sdk/fpapp.x' \
  cargo +nightly build \
  -p fpapp-sdk \
  --example native_probe \
  --features native-example \
  --release \
  --target thumbv8m.main-none-eabihf
```

For an external app, copy `fpapp.x` into the app crate and adjust the linker
path. The result must have one allocated executable `.text` section at address
zero, no writable allocated section, and only the supported position-relative
relocations. `fpapp pack` retains and audits relocation records, rejecting
absolute or unknown relocation kinds before it accepts the ELF.

## Package the ELF

Build the `fpapp` tool once with `cargo build -p fpapp`, then run:

```sh
target/debug/fpapp pack \
  --elf target/thumbv8m.main-none-eabihf/release/examples/native_probe \
  --output native-probe.fpapp \
  --id 100 \
  --version 0.1.0 \
  --name 'Native probe' \
  --description 'Minimal FPApp SDK example' \
  --author 'Your name' \
  --channels 1 \
  --color ffcc00 \
  --icon 13 \
  --firmware-revision 40_HEX_DIGIT_GIT_REVISION \
  --manual manual.json \
  --setup setup.md \
  --settings settings.schema.json

target/debug/fpapp inspect native-probe.fpapp
target/debug/fpapp verify native-probe.fpapp
```

The firmware revision must be the exact revision used for the target firmware
build. A package built for another revision is rejected before upload and
again during device commit.

Community manuals use a `faderpunk-manual-v1` JSON envelope whose `app` value
matches `manual-tab.json`. This lets the Configurator render installed manuals
with the same `ManualApp` component, channel diagram, and visual hierarchy as
built-in manuals. The setup section remains Markdown.

## Build all community apps

The `fpapp` tool can build every entry in `faderpunk-community-apps` without
copying the app logic into a second crate:

```sh
target/debug/fpapp build-community \
  --repo /path/to/faderpunk-community-apps \
  --output /path/to/faderpunk-community-apps/build/fpapps \
  --firmware-revision 40_HEX_DIGIT_GIT_REVISION
```

The builder reads each app's real `CONFIG`, manual, setup notes, color, and
icon. It replaces only the static Embassy task wrapper, compiles the same
`run(&App<N>)` logic, validates the ELF, and writes one `.fpapp` per catalog
entry.

Parameter metadata describes the controls only. Runtime defaults stay in each
app's ordinary Rust `ParamStore` constructor. The compatibility facade keeps
those defaults when no valid saved record exists and reports the live values
through the same parameter request path as a built-in app.

## Host ABI v1

`HostV1` is a size-versioned C table. The compatibility module supplies the
firmware-style `App<N>` facade for faders, buttons, Shift, persistent clock
subscriptions, scenes, parameters, FRAM-backed app storage, timers, CV and gate
jacks, all LED modes, USB/DIN MIDI input and output, I2C output, random values,
the global quantizer, swing, and fader takeover.

Each asynchronous consumer has an independent event cursor, so concurrent app
tasks observe the same firmware event. A clock object keeps its cursor between
waits, matching the built-in clock subscriber. Firmware owns all peripheral and
storage access; package channels are relative to the app's layout allocation.

See [`docs/fpapp.md`](../docs/fpapp.md) for the format, installation transaction,
capability matrix, trust boundary, and current acceptance status.
