# USB host compatibility (CME H4MIDI investigation)

**Status: RESOLVED** — the config protocol moved from a WebUSB vendor interface
to MIDI SysEx on virtual cable 2 (branch `feat/midi-only-usb`), making
Faderpunk a pure class-compliant USB-MIDI device. The 2-cable layout was
confirmed working with the H4MIDI on hardware. The `midi-only` cargo feature
described below was an interim workaround and has been removed. The
investigation below is kept for reference.

Investigation as of 2026-07-10.

## Problem

The CME H4MIDI WC (standalone USB MIDI host) enumerates Faderpunk and claims
ports in HxMIDI Tools, but **no MIDI flows in either direction** over USB.
MIDI via DIN works, and USB MIDI to/from a computer works (verified with
showMIDI), so the MIDI implementation itself is fine.

## What was tested

All builds flashed and tested against the H4MIDI on actual hardware:

| Build | Change vs. normal firmware | Result |
|---|---|---|
| Normal firmware | — (composite: MIDI + WebUSB vendor interface, IADs, class `0xEF/0x02/0x01`, MS-OS/BOS) | enumerates, no MIDI |
| `midi-only` feature | WebUSB function, MS-OS/BOS, IADs, `0xEF` class all removed; pure single-function MIDI device | **works** |
| WebUSB on interrupt endpoints | vendor endpoints interrupt instead of bulk (only MIDI pair is bulk) | no MIDI |
| Test A: no IADs | IADs removed, device class `0x00`; WebUSB + MS-OS kept (config descriptor 120 bytes, under the classic 128-byte host buffer) | no MIDI |
| Test B: no IADs, no MS-OS | Test A plus MS-OS/BOS removed (bcdUSB 2.00) | no MIDI |

## Conclusion

The H4MIDI's host stack rejects (or fails to bind MIDI on) any device that has
**any interface besides the MIDI function** — independent of endpoint type,
device class, IAD presence, MS-OS/BOS descriptors, and config descriptor
length. There is no descriptor-level trick left to dodge this; the vendor
interface itself is disqualifying. Faderpunk's descriptors are valid USB — a
class-compliant MIDI function plus one vendor interface is a common layout —
so this is a limitation on CME's side.

## Workaround available now

The `midi-only` cargo feature on the `faderpunk` crate builds a firmware
without the WebUSB function:

```bash
cd faderpunk
cargo build --release --features midi-only
# then picotool uf2 convert as in build-uf2.sh
```

The device presents as "Faderpunk MIDI" so builds are distinguishable.
**No configurator access in this build** — flash the normal firmware to change
configuration, then flash back.

## Next steps

### 1. Report to CME (support@cme-pro.com)

What to send:

- Device: Faderpunk (VID `0xF569`, PID `0x0001`), RP2350-based MIDI controller.
- Symptom: H4MIDI WC enumerates the device and HxMIDI Tools shows it and its
  port, but no MIDI flows in either direction. The same device works with
  computers (Windows/macOS/Linux) and via DIN.
- Key evidence: the device is a composite of a class-compliant USB-MIDI 1.0
  function (interfaces 0+1, first in the config) plus one vendor-specific
  interface (WebUSB, used by our browser configurator). A test firmware with
  the **identical MIDI function but the vendor interface removed works
  perfectly** with the H4MIDI. Variants without IADs, with device class 0x00,
  without MS-OS/BOS descriptors, and with interrupt instead of bulk endpoints
  on the vendor interface all still fail — only removing the extra interface
  entirely helps.
- Attach `lsusb -v` (or USB Prober/USBTreeView) dumps of **both** firmwares:
  the normal one (fails on H4MIDI) and the midi-only one (works).
- Ask: can a firmware update make the H4MIDI bind the MIDI-streaming
  interface on composite devices that carry additional non-MIDI interfaces?
  Many modern controllers ship this layout (audio+MIDI combos, devices with
  vendor/config interfaces), so the fix helps beyond Faderpunk.

### 2. Runtime "USB compatibility mode" toggle (recommended, shippable)

Turn the compile-time `midi-only` gates in `tasks/transport.rs` into a
runtime decision so users don't need a special firmware:

- Add a `usb_midi_only: bool` (name TBD) field to the global config in
  `libfp`, regenerate bindings (`./gen-bindings.sh`), add a configurator
  toggle with copy explaining it takes effect after reboot and disables
  configurator access.
- In `run_transports`, read the flag **before** building the USB stack (the
  builder runs once at boot, so this needs the persisted config from FRAM
  early — check task startup ordering; may need a direct FRAM read rather
  than `GLOBAL_CONFIG_WATCH`).
- Escape hatch so users can't lock themselves out: holding SHIFT (the
  bottom-right button already used for bootloader entry) during power-on
  forces the full USB stack with WebUSB regardless of the flag.

### 3. Long-term option: configurator over MIDI SysEx

Move the config protocol onto a second virtual MIDI cable (USB-MIDI supports
16 cables per endpoint pair) as SysEx. Faderpunk becomes a pure
single-function MIDI device — compatible with every hardware host, no mode
switch. Costs: configurator moves from WebUSB to WebMIDI (SysEx permission),
postcard/COBS payloads need 7-bit packing (~17% overhead plus framing), the
firmware needs a SysEx assembler, and the WebUSB landing-page URL feature is
lost. Only worth it if hardware-host compatibility becomes a headline
feature.
