# Historical prototype notes: Native `.fpapp` packages

> This document records the initial native-loader spike and is retained for
> review history. The implemented runtime-v1 proposal, current format, full
> compatibility matrix, user experience, risks, and acceptance plan are in
> [`fpapp.md`](fpapp.md).

Status: **experimental reference implementation**

Container version: `0`
Runtime ABI version: `0`

This RFC defines an installable community-app format for Faderpunk. A user
selects a `.fpapp` in the Configurator, reviews its documentation and requested
access, chooses one of four slots, and uploads it over the existing config
connection. BOOTSEL and a replacement UF2 are not part of app installation.

Version 0 deliberately chooses a small native wrapper instead of a VM. An app
is compiled as position-independent Thumb code against one exact firmware
revision, stored in reserved flash, and called directly by that firmware. This
keeps the authoring model in Rust and avoids an interpreter, but it also means
that installed app code is trusted firmware-level code. There is no sandbox.

The implementation in this branch is testable end to end, but it is not yet a
stable author contract. The current host interface supports fader and button
events plus CV output. The compatibility adapter needed by the existing Sift
source also needs clock, gates, LEDs, MIDI, parameters, scenes, persistence,
timers, and quantization before Sift can run unchanged as an FPApp.

## User experience

1. Connect Faderpunk to the Configurator.
2. Open **FPApps** and choose a `.fpapp` file.
3. The Configurator parses the package locally and shows its identity,
   firmware match, setup guide, manual, settings schema, and signature status.
4. Confirm that the package contains trusted native code with hardware-level
   access.
5. Choose an empty slot, or replace an inactive app in a used slot.
6. The Configurator uploads sequential 256-byte chunks and asks the device to
   commit the package.
7. Only a completely written, structurally valid package for the exact running
   firmware becomes installed. It then joins the normal app catalog and may be
   placed in a layout.
8. Installed documentation and settings metadata remain available from the
   device. An inactive app may be removed from the same screen.

If upload is interrupted or validation fails, the selected slot is empty. The
user chooses the file and uploads it again. Other slots are unaffected.

## Decisions

### Native, exact-firmware code

Program kind `1` is position-independent Thumb code. The package declares a
32-byte firmware ABI derived from the complete 40-hex-digit Git revision. The
Configurator and firmware both require an exact match.

This is not a stable Rust ABI. Exact matching lets the host interface evolve
with the firmware while the format is experimental. After a firmware change,
an app must be rebuilt against that revision and uploaded again. An old package
is not executed by a different firmware.

### Four independent slots; no A/B pair

The final 512 KiB of the supported 2 MiB flash is four independent 128 KiB
slots. Each slot holds at most one app package. There is no active/inactive A/B
pair and no automatic rollback:

- a 4 KiB control sector says whether the slot is valid;
- up to 124 KiB stores package bytes;
- beginning an upload erases the slot's control sector first;
- commit writes the control record only after every package check passes;
- reset or power loss before commit therefore leaves that slot empty;
- the recovery operation is simply to upload again.

Replacing or removing an app that is present in the active layout is rejected.
This prevents a running layout from retaining a pointer into erased flash.

### Explicit trust, not implied isolation

Native app code executes from XIP flash on the same processor and privilege
level as firmware. CRC32 detects corruption; it is not publisher identity.
The optional signing section is transported and displayed but version 0 does
not yet verify it on the device.

The Configurator must not describe FPApps as sandboxed. Local unsigned packages
are allowed only after an explicit confirmation that the author and artifact
are trusted. A curated community catalog can later add reproducible builds,
reviewed source, checksums, and signature policy without changing the install
transaction.

## Module design

The implementation has four deep modules and narrow interfaces:

```text
Configurator
  |  package preview + config SysEx
  v
FPApp protocol adapter
  |
  +--> SlotStore ------> reserved RP2350 flash
  |
  +--> runtime catalog -> layout/app registration
                            |
                            v
                     native runtime host
                            |
                            v
                       fpapp-sdk app
```

- `libfp::fpapp` owns the allocation-free package parser and builder.
- `libfp::fpapp_store` owns slot validity and installation transactions behind
  the `SlotFlash` seam. Tests use an in-memory flash adapter; firmware uses the
  RP2350 flash adapter.
- `faderpunk::fpapps` owns the installed catalog and publishes immutable native
  runtime descriptors only after packages have been checked.
- `faderpunk::fpapp_runtime` owns one app instance's storage, events, host call
  table, output cleanup, and lifecycle.

Layout and Configurator code do not know flash addresses or native entrypoint
details. The package tooling does not know how firmware schedules an instance.

## Flash allocation

```text
0x1000_0000 .. 0x1018_0000  firmware image       1536 KiB
0x1018_0000 .. 0x1020_0000  FPApp region          512 KiB
```

The FPApp region contains four equal slots:

| Slot | Control | Package capacity |
| ---: | ---: | ---: |
| 0 | 4 KiB | 124 KiB |
| 1 | 4 KiB | 124 KiB |
| 2 | 4 KiB | 124 KiB |
| 3 | 4 KiB | 124 KiB |

The firmware linker is limited to the first 1536 KiB, so normal firmware
linking cannot overwrite package storage. Physical flash operations pause the
other RP2350 core through the Embassy flash implementation.

The control record contains magic, version, slot number, app ID, semantic
version, package length, package CRC, and its own CRC. Package bytes are valid
only when that record is present and agrees with the parsed package.

## `.fpapp` container version 0

All integers in the container header and section table are unsigned
little-endian. Section offsets start at the package's first byte and are
four-byte aligned.

### Fixed header

| Offset | Size | Field |
| ---: | ---: | --- |
| 0 | 8 | Magic `FPAPP\0\r\n` |
| 8 | 2 | Container version (`0`) |
| 10 | 2 | Runtime ABI version (`0`) |
| 12 | 4 | Total package length |
| 16 | 2 | Section count, at most 16 |
| 18 | 2 | Header plus section-table length |
| 20 | 4 | CRC32 of all bytes after the section table |

Each 12-byte section descriptor contains `kind: u16`, `flags: u16`,
`offset: u32`, and `length: u32`. Flag bit zero marks a required section.
Sections may not overlap the table or each other. Unknown optional sections are
ignored; unknown required sections reject the package.

| Kind | Required | Contents |
| ---: | :---: | --- |
| 1 | yes | CBOR manifest |
| 2 | yes | native program envelope |
| 3 | no | UTF-8 Markdown manual |
| 4 | no | UTF-8 Markdown setup guide |
| 5 | no | UTF-8 JSON Schema settings hook |
| 6 | no | opaque signing metadata |

### CBOR manifest

The manifest is a map with integer keys:

| Key | Field | Version-0 constraint |
| ---: | --- | --- |
| 0 | app ID | `100..=255` |
| 1 | semantic version | three `u16` values |
| 2 | program kind | `1` for Thumb ROPI |
| 3 | name | ASCII, 1-32 bytes |
| 4 | description | ASCII, 1-96 bytes |
| 5 | author | UTF-8, 1-64 bytes |
| 6 | channel count | `1..=16` |
| 7 | display color | 24-bit RGB in a `u32` |
| 8 | icon | Faderpunk icon discriminant |
| 9 | parameter definitions | empty in version 0 |
| 10 | requested persistent bytes | reserved; not connected yet |
| 11 | execution units per event | reserved for policy |
| 12 | capability bitmap | reserved for policy and display |
| 13 | firmware ABI | exactly 32 bytes |

App ID must not duplicate another installed package. Firmware repeats every
check performed by the builder and does not trust browser-side parsing.

### Native program envelope

The program section begins with a 28-byte `FPN0` envelope:

| Offset | Size | Field |
| ---: | ---: | --- |
| 0 | 4 | Magic `FPN0` |
| 4 | 2 | native envelope version (`0`) |
| 6 | 2 | header length (`28`) |
| 8 | 4 | `fpapp_required_bytes` offset |
| 12 | 4 | `fpapp_init` offset |
| 16 | 4 | `fpapp_poll` offset |
| 20 | 4 | `fpapp_drop` offset |
| 24 | 4 | native image length |

The remaining bytes are the app's `.text` image linked at address zero with a
read-only position-independent relocation model. Entrypoint offsets must be
even and inside the image. The builder rejects ELF files with relocations or
writable allocated sections before extracting `.text`.

## Native host ABI version 0

`fpapp-sdk` exports four C entrypoints from an async Rust app factory:

```text
fpapp_required_bytes() -> u32
fpapp_init(storage, storage_len, host) -> status
fpapp_poll(storage, host) -> status
fpapp_drop(storage) -> status
```

Firmware provides aligned caller-owned storage; the SDK never allocates. The
current firmware limit is 8 KiB per active instance. Each instance has a
bounded queue of 32 events. Overflow discards the oldest event.

`HostV0` is a size-versioned C table containing:

- `take_event(context, event)`;
- `set_output(context, relative_channel, dac_counts)`;
- `schedule_poll(context)`.

Event kinds are fader, button down, button up, and button long press. Channel
numbers are relative to the app's assigned layout span. On start, all channels
are configured as 0-10 V outputs and current fader values are queued. On exit,
completion, or initialization failure, all owned channels return to high
impedance.

This host table is intentionally small enough to prove native loading. It is
not yet the full author interface. A version capable of compiling the existing
Sift implementation must add adapters for the existing `App<N>` facilities
rather than duplicating Sift's behavior inside the loader.

## Installation protocol

The existing 512-byte config transport has these appended request variants:

- `GetFpAppSupport`;
- `GetFpAppSlots`;
- `BeginFpAppInstall { slot, total_len }`;
- `WriteFpAppChunk { offset, len, data }`;
- `CommitFpAppInstall`;
- `AbortFpAppInstall`;
- `RemoveFpApp { slot }`;
- `ReadFpAppSection { slot, section, offset }`.

Support reports the 32-byte firmware ABI, four slots, 124 KiB package maximum,
and a 256-byte chunk size. Chunks are strictly sequential. A protocol unit test
serializes a full request and response and proves each remains below 512 bytes.

Commit reparses the complete package from flash, checks the native envelope,
requires the exact firmware ABI, rejects duplicate IDs, then writes the control
record last. Abort does not restore the previous package; it leaves the chosen
slot empty, ready for a retry.

## Configurator hooks

The Configurator implements both connected-device and simulator paths. It:

- parses and CRC-checks the selected file before transfer;
- rejects an exact-ABI mismatch before uploading;
- shows package metadata and setup Markdown before installation;
- requires the explicit native-code trust confirmation;
- displays per-slot progress and status-specific recovery guidance;
- retrieves installed manual, setup, and settings sections from the device;
- summarizes JSON Schema properties for settings documentation;
- refreshes the normal app catalog and layout after install or remove.

The settings section is a configurator hook in version 0, not runtime state.
Schema-driven value editing, persistence, and delivery to an app are future
work and must not be implied by the UI.

## Author tooling

The lowercase `fpapp` crate is the reference command-line tool:

```text
fpapp abi GIT_REVISION
fpapp pack --elf app.elf --output app.fpapp [metadata and hook options]
fpapp inspect app.fpapp
fpapp verify app.fpapp [--firmware-abi HEX]
```

An author writes a `no_std` Rust crate using `fpapp-sdk`, exports an async app
factory with `export_app!`, links it with `fpapp.x`, and packs the resulting ELF
for the target firmware revision. Community CI should build the ELF and package
from reviewed source rather than accepting an opaque contributor binary.
The exact reproducible build and pack commands are maintained in
[`fpapp-sdk/README.md`](../fpapp-sdk/README.md).

## Verification and acceptance gates

The reference implementation currently has automated evidence for:

- container build/parse, optional hooks, CRC, overlap and section validation;
- native envelope and exact firmware ABI checks;
- interrupted upload leaving only the selected slot empty;
- completed installation surviving store reopen;
- sequential chunks, remove, duplicate ID, active-app, and size rejection;
- SDK future initialization, polling, output, and drop through a fake host;
- native probe cross-compilation with no relocations or writable sections;
- real package pack, inspect, and verify commands;
- config messages fitting the 512-byte transport;
- firmware release build inside the reduced 1536 KiB linker region;
- Configurator production build and browser-driven desktop/mobile workflows.

Before format or runtime ABI version 1, the project still requires:

1. Extend the host adapter until Sift compiles from the shared community source
   without copying its logic into firmware.
2. Exercise Sift on physical hardware across clock, gate, CV, MIDI, LEDs,
   parameters, scenes, persistence, faders, and buttons.
3. Measure mixed static/dynamic layout task-arena use, event latency, and output
   jitter at worst-case occupancy.
4. Make task-arena exhaustion and native faults visible without panicking the
   layout manager.
5. Define and implement settings value persistence and app delivery.
6. Decide curated-build and signature policy; do not label CRC as trust.
7. Confirm firmware update behavior. Version 0 preserves package flash but
   ignores ABI-mismatched slots until they are replaced by a compatible upload.
8. Build and run a second independently authored app through the same SDK.

BOOTSEL remains the unconditional firmware recovery mechanism. FPApp install
failure must never weaken or replace it.
