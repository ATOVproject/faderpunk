# FPApp: installable community apps for Faderpunk

> **Superseded design exploration.** This document records the rejected
> sandbox/VM and A/B rollback design. The implemented native, exact-firmware,
> four-independent-slot proposal is [`fpapp.md`](fpapp.md).
> This draft is retained temporarily so review does not destroy earlier work.

Status: **experimental RFC**. This document proposes a feasibility contract,
not a stable package format. Container version `0` and runtime ABI `0` may
change or be discarded after the prototype gates below.

## Decision sought

Decide whether Faderpunk should support installing and removing sandboxed
community apps without rebuilding or replacing the complete firmware image.

The proposed user experience is:

1. Connect Faderpunk to the Configurator normally.
2. Choose a community app and press Install.
3. The Configurator transfers one `.fpapp` package over config SysEx.
4. Faderpunk validates and stores it transactionally.
5. After a restart, the app appears in the existing Apps tab and layout editor.
6. Update and Remove use the same page; BOOTSEL and UF2 are not involved.

This does **not** make existing native Rust apps dynamically loadable. Native
apps use compile-time modules, const-generic `App<N>` values, and statically
allocated Embassy task pools. FPApp introduces a separate, event-driven author
interface whose programs run inside one statically compiled runtime.

## Evidence from current `main`

Measurements below were taken from commit `c893125` using a release build for
`thumbv8m.main-none-eabihf`:

| Resource | Current use | Declared capacity | Approximate headroom |
| --- | ---: | ---: | ---: |
| Flash image | 970,348 bytes | 2,097,152 bytes | 1,126,804 bytes |
| Static RAM | 423,076 bytes | 524,288 bytes | 101,212 bytes |

The RAM number includes `.data`, `.bss`, and `.uninit`. It is the tighter
constraint: the firmware already reserves a 256 KiB Embassy task arena and a
128 KiB Core 1 stack.

Other constraints verified in the current source:

- `register_apps!` generates a static module list and `match app_id` spawn path.
- Each native app declares a compile-time Embassy task pool.
- The 128 KiB external FRAM address space is fully allocated; package binaries
  cannot live there.
- Config SysEx accepts at most 512 bytes of postcard payload per message.
- Layouts and app IDs are already dynamic data, and IDs 100-255 are available
  for community apps.
- The Configurator already obtains app metadata from the connected device via
  `GetAllApps`; an installed catalog can join that existing response.

## Goals

- Install, update, list, and remove community apps without flashing firmware.
- Keep factory/native app behavior and performance unchanged.
- Make malformed, incompatible, interrupted, or malicious packages unable to
  corrupt the active package catalog or access hardware directly.
- Bound program memory, execution time, event backlog, package size, and the
  number of active instances without a heap allocator.
- Preserve the existing layout, parameter, scene, MIDI, CV, gate, LED, clock,
  and button concepts where they make sense for community programs.
- Make packages reproducible and independently inspectable by community CI.
- Keep Rust as the community authoring language; authors do not write bytecode,
  container sections, or SysEx messages by hand.
- Keep BOOTSEL available as an unconditional firmware-recovery path.

## Non-goals for the first runtime

- Loading arbitrary Thumb machine code, ELF files, dylibs, or existing
  `faderpunk/src/apps/*.rs` files unchanged.
- A stable Rust ABI.
- Audio-rate DSP.
- Installing while a community app is actively driving hardware.
- Running package code during validation or installation.
- Treating CRC32 as publisher authentication.
- Replacing normal firmware releases or factory apps.

## Module and seam placement

The external seam is one deep `CommunityApps` module used by layout and config
code. Callers should not know how packages are stored or interpreted.

```text
LayoutManager / GetAllApps / parameter + scene routing
                         |
                 CommunityApps interface
                         |
       +-----------------+------------------+
       |                                    |
PackageStore implementation          Runtime implementation
(RP flash; in-memory test adapter)    (bounded VM; fake test adapter)
       |
SysEx installer adapter
```

The firmware-facing interface needs only these capabilities:

- enumerate installed app metadata;
- resolve channel count and configuration by app ID;
- start and stop a layout instance;
- route parameter and scene operations;
- begin, continue, commit, inspect, and abort a package transaction;
- remove an inactive installed app.

The implementation owns flash layout, validation, rollback, scheduling,
instruction accounting, and conversion from program effects to sanctioned
hardware operations.

One static Embassy task multiplexes every FPApp instance. FPApp programs do not
create Embassy tasks. Native IDs continue through `register_apps!`; unresolved
community IDs delegate to `CommunityApps`.

## `.fpapp` container version 0

Version 0 is a compact, length-delimited container intended to be parsed with
fixed buffers. It is not ZIP and does not require allocation.

All integers are unsigned little-endian. Offsets are from the start of the
package and must be four-byte aligned.

### Fixed header

| Offset | Size | Field |
| ---: | ---: | --- |
| 0 | 8 | Magic: `FPAPP\0\r\n` |
| 8 | 2 | Container version (`0`) |
| 10 | 2 | Runtime ABI version (`0` during prototyping) |
| 12 | 4 | Total package length |
| 16 | 2 | Section count |
| 18 | 2 | Header plus section-table length |
| 20 | 4 | CRC32 of every byte after the section table |

The fixed header is followed by `section_count` descriptors.

### Section descriptor

| Offset | Size | Field |
| ---: | ---: | --- |
| 0 | 2 | Section kind |
| 2 | 2 | Flags; bit 0 means required |
| 4 | 4 | Section offset |
| 8 | 4 | Section length |

Sections may not overlap the header, the descriptor table, or each other. A
reader skips unknown optional sections and rejects unknown required sections.

Initial section kinds:

| Kind | Required | Contents |
| ---: | --- | --- |
| 1 | yes | CBOR manifest |
| 2 | yes | Tagged runtime program |
| 3 | no | UTF-8 manual JSON matching the community manual entry |
| 4 | no | Publisher/signing metadata reserved for a later policy |

CRC32 detects damaged or incomplete transfers. Authenticity, when required,
must be established by catalog metadata or a future signature section.

### Manifest version 0

The manifest is a CBOR map with integer keys. Version 0 carries:

| Key | Field | Constraint |
| ---: | --- | --- |
| 0 | app ID | `100..=255`; must not collide with factory or installed IDs |
| 1 | package version | three `u16` values: major, minor, patch |
| 2 | program kind | identifies the interpreter/program format |
| 3 | name | ASCII, 1-32 bytes |
| 4 | description | ASCII, 1-96 bytes |
| 5 | author | UTF-8, 1-64 bytes |
| 6 | channel count | `1..=16` |
| 7 | color | existing `Color` discriminant/data |
| 8 | icon | existing `AppIcon` discriminant |
| 9 | parameter definitions | at most `APP_MAX_PARAMS` |
| 10 | persistent-state bytes | bounded by the selected runtime budget |
| 11 | execution units per event | bounded by firmware policy, not trusted |
| 12 | capability bitmap | declared subset of the runtime host interface |

The package builder validates metadata before emitting a package. Firmware
repeats every structural and policy check; builder success is not trusted.

### Program section

The container deliberately tags, rather than assumes, the program format. The
first feasibility implementation may use an experimental compact bytecode, but
its instruction set must not be declared stable until Sift passes the hardware
gates below.

WebAssembly remains a candidate only if measurements prove it can support the
maximum active-instance case within the RAM budget. A conventional Wasm linear
memory page is 64 KiB, so one interpreter instance per layout app is unlikely
to fit the measured 101 KiB static-RAM headroom. Arbitrary native machine code
is rejected for version 0 because it has neither isolation nor a stable ABI.

## Event and effect interface

Programs receive bounded events rather than owning async tasks:

- `Start`, `Stop`;
- `ClockStart`, `ClockStop`, `ClockReset`, `ClockTick`;
- `FaderChanged`, `ButtonDown`, `ButtonUp`, `ShiftChanged`;
- `Midi` events allowed by the manifest;
- `ParametersChanged`;
- `SceneLoad`, `SceneSave`;
- optional bounded `Timer` events.

Programs emit effects which the runtime validates before applying:

- configure a relative channel jack as input, output, or gate;
- set output/gate values;
- set relative LEDs;
- send MIDI;
- update bounded instance state;
- request a timer.

Programs cannot receive raw peripheral handles, firmware pointers, flash/FRAM
addresses, task channels, or access outside their assigned channel range.

Every dispatch has an instruction/effect budget. Exceeding it aborts that
dispatch, clears unsafe held outputs, records a fault, and eventually
quarantines a repeatedly faulting package. Loops are allowed only insofar as
they consume the finite dispatch budget.

## Package flash store

Version 0 proposes reserving the final 512 KiB of the safe 2 MiB flash region:

```text
firmware/linker region: 0x1000_0000 .. 0x1018_0000  (1536 KiB)
FPApp package region:   0x1018_0000 .. 0x1020_0000  ( 512 KiB)
```

This leaves approximately 566 KiB of firmware-link headroom over the measured
970,348-byte image before adding the runtime. The production partition size is
not accepted until a stable-toolchain build and runtime prototype confirm it.

The store is log-structured and uses the flash implementation's erase/write
sizes rather than hard-coded sector assumptions. It contains:

- two alternating catalog/superblock records with generation and CRC;
- immutable package records;
- tombstones for removals;
- a commit record written only after complete package verification.

Install/update sequence:

1. Reject if the target app is present in the active layout.
2. Enter install mode and place all outputs owned by FPApp instances in a safe
   state.
3. Erase/allocate staging space without changing the active catalog.
4. Accept sequential chunks, recording the next expected offset.
5. Verify length, container, section table, manifest, program, and CRC.
6. Append the package record and then the commit record.
7. Write the next catalog generation last.
8. Require a restart before the new generation can execute.

On boot, incomplete staging data is ignored. If the newest committed update
fails runtime validation, the previous committed generation remains available
for rollback. Garbage collection is never part of the first install path; it
runs only when no FPApp instance is active and must itself be power-loss safe.

Factory reset continues to reset layouts/settings but does not silently erase
installed packages. The installer offers an explicit Remove All operation.

## Config SysEx installation

Installation reuses the existing class-compliant config cable. New message
variants must be appended so older variant ordinals remain stable.

Conceptual requests:

- `GetFpAppInstallStatus`;
- `BeginFpAppInstall { total_len, crc32 }`;
- `WriteFpAppChunk { offset, len, data }`;
- `CommitFpAppInstall`;
- `AbortFpAppInstall`;
- `RemoveFpApp { app_id }`.

Conceptual responses report state, next expected offset, and a bounded error
code. Chunks are sequential and at most 384 data bytes, leaving room under the
existing 512-byte postcard payload limit. Reconnecting clients query status
and resume from the reported offset; the firmware never accepts sparse writes.

Protocol addition, firmware storage/runtime, and Configurator UI are separate
PRs. The Configurator UI must not land until firmware messages have merged;
the repository explicitly disallows stacked PRs on unmerged work.

## Author and build workflow

An FPApp is authored as a Rust `no_std` crate using a new `fpapp-sdk`. Rust is
the author-facing language even if the selected sandbox program format is
WebAssembly or compact bytecode. Authors do not write that representation
directly.

Current `App<N>` Rust files do not compile to the SDK unchanged. The SDK
exposes event/effect types and a macro or manifest builder, but no firmware
internals, Embassy tasks, peripheral handles, or unrestricted allocation.
Pure Rust state-machine and sequencing logic can normally be retained; direct
hardware, task, and storage calls must be adapted to the SDK interface.

Proposed commands in `faderpunk-forge`:

```text
faderpunk-forge build-app --manifest fpapp.toml --out sift.fpapp
faderpunk-forge inspect-app sift.fpapp
faderpunk-forge verify-app sift.fpapp
```

`build-app` compiles the selected program target, extracts/validates metadata,
adds the matching community manual entry, creates the deterministic container,
and prints its SHA-256. Identical source, lockfile, SDK, and builder versions
must produce identical package bytes.

The community repository remains source-of-truth for IDs and manuals. Its CI
builds packages after merge and publishes immutable artifacts plus SHA-256
digests. The normal submission gate continues to review source; prebuilt
contributor binaries are never trusted as the reviewed artifact.

Sift is the reference port and conformance fixture. Its existing native source
remains available while the FPApp port is experimental.

## Repository and PR sequence

This feature intentionally does not fit one PR:

1. **Faderpunk docs:** this RFC, revised to maintainer agreement.
2. **libfp:** version-0 container/manifest parser with host tests and no runtime.
3. **Firmware core:** flash partition and transactional package-store module,
   tested through an in-memory adapter plus RP flash hardware checks.
4. **Firmware core:** one bounded runtime task and installed catalog/layout
   integration, initially with an embedded Sift fixture.
5. **Protocol/libfp:** chunked install/status/remove messages and regenerated
   bindings.
6. **Configurator:** install/update/remove experience against merged firmware.
7. **faderpunk-forge:** SDK/build/inspect/verify tooling.
8. **faderpunk-community-apps:** source convention, Sift port, CI publication.

Each PR branches from then-current `main`; later PRs wait for their dependency
to merge rather than stacking diffs.

## Prototype and acceptance gates

The throwaway state-model prototype lives at
`docs/prototypes/fpapp-install-state.html`. It exists to validate transaction
and recovery semantics; it is not intended for `main`.

The implementation proceeds beyond experimental version 0 only when all of
these are demonstrated:

- A malformed or unsupported package cannot enter the active catalog.
- Power loss at every install phase yields either the previous package or the
  fully committed update, never a partial active package.
- Install, update, rollback, and remove are driven from the Configurator over
  SysEx without BOOTSEL.
- `GetAllApps`, layout validation, parameters, and scenes work for installed
  apps without regressing native apps.
- Sift runs on physical hardware with gate, CV, MIDI, clock, faders, buttons,
  randomization, parameters, mute, and scene recall matching its native app.
- Worst-case active instances stay within explicit measured RAM and Core 1 CPU
  budgets; missed clock/input events and output jitter are measured.
- A faulting program is bounded, silenced, reported, and cannot starve native
  tasks or access raw hardware.
- Firmware update and downgrade behavior with installed packages is defined and
  recovery remains possible through BOOTSEL.
- Package output is reproducible and community CI validates source-to-artifact
  provenance.

Stopping rules:

- Reject the selected program format if it cannot support Sift within the
  worst-case instance budget.
- Reconsider the 512 KiB partition if the runtime leaves less than 25% linker
  headroom or cannot retain one previous generation during update.
- Do not add install UI before power-loss behavior is proven in the store.
- Do not call container/ABI version 1 stable until two independently authored
  community apps build and run through the same SDK without runtime changes.

## Open decisions for the prototype

- Compact custom VM versus another no-heap, bounded interpreter.
- Exact execution-unit and per-instance state budgets after measurement.
- Whether package signatures are required, optional, or catalog-only.
- Whether firmware updates preserve, quarantine, migrate, or erase packages on
  runtime ABI changes.
- Whether manuals remain in device flash or are stripped after host-side
  validation to maximize program capacity.
