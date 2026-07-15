# NRPN / 14-bit MIDI Support Design

## Goal

Add bidirectional NRPN support to Faderpunk, enabling high-resolution (14-bit) MIDI parameter control for devices like Elektron synths. Apps remain unaware of the wire format — they keep working with 0-4095 values. The MIDI layer handles all NRPN assembly/disassembly and scaling.

## Core Principle

Apps always work with 0-4095 values. NRPN vs CC is a wire format concern handled entirely by the MIDI layer. The only new thing apps see is an opt-in mode toggle parameter.

## Architecture

### Event Type: MidiEvent Wrapper Enum

The MIDI PubSub channels change from `LiveEvent<'static>` to a wrapper enum:

```rust
enum MidiEvent {
    Live(LiveEvent<'static>),
    Nrpn { channel: u4, param: u16, value: u16 },
}
```

- `Live` carries existing MIDI messages (CC, notes, pitch bend, etc.)
- `Nrpn` carries assembled NRPN events with parameter number (0-16383) and value (0-16383)
- Both `MIDI_USB_PUBSUB` and `MIDI_DIN_PUBSUB` publish `MidiEvent`

### Receive Path (Core 0 MIDI Task)

Stateful `NrpnTracker` per MIDI channel (16 instances) assembles CC 98/99/6/38 sequences:

```rust
struct NrpnTracker {
    param_msb: Option<u8>,  // set by CC 99
    param_lsb: Option<u8>,  // set by CC 98
    value_msb: Option<u8>,  // set by CC 6
}
```

**Assembly logic:**
- CC 99 → store `param_msb`, clear `value_msb`
- CC 98 → store `param_lsb`, clear `value_msb`
- CC 6  → if both param bytes set, store `value_msb`
- CC 38 → if `value_msb` set, emit `MidiEvent::Nrpn`, clear `value_msb`

**Smart detection:** CC 6/38 are only intercepted when `param_msb`/`param_lsb` have been set by preceding CC 98/99. Standalone CC 6/38 pass through as normal `MidiEvent::Live` messages.

**Timeout fallback:** If CC 6 arrives but CC 38 never follows (7-bit-only NRPN senders), emit after a short timeout or on the next non-CC-38 message, using `value_msb << 7` as the value.

**App-level filtering via MidiInput:**
- CC mode apps: receive `MidiEvent::Live` CC messages matching their configured param, scale 7-bit → 12-bit
- NRPN mode apps: receive `MidiEvent::Nrpn` events matching their configured param, scale 14-bit → 12-bit
- Existing apps unaffected — they keep matching on `MidiEvent::Live`

### Send Path (MidiOutput)

When an app calls `send_cc(param, value)`:

**CC mode (existing behavior):**
- Param clamped to 0-127
- Value scaled 12-bit → 7-bit via `scale_bits_12_7`
- Single CC message sent

**NRPN mode:**
- Param used as-is (0-16383)
- Value scaled 12-bit → 14-bit via `scale_bits_12_14`
- Sends CC sequence: CC 99 (param MSB), CC 98 (param LSB), CC 6 (value MSB), CC 38 (value LSB)

**Output caching:** `MidiOutput` remembers the last NRPN param number sent. If unchanged, skips CC 98/99 and only sends CC 6/38. Cuts bandwidth in half during fader sweeps.

### Per-App Configuration

**New parameter:** `Param::MidiNrpn` — a boolean toggle (CC vs NRPN mode). Each MIDI-capable app opts in by adding this to its CONFIG and Params struct.

**MidiCc range change:** When NRPN mode is enabled, the `MidiCc` parameter accepts 0-16383 instead of 0-127, allowing users to target NRPN parameter numbers.

Apps that don't add the NRPN toggle keep working as CC-only with zero changes.

### Scaling Utilities (libfp)

New functions in `libfp/src/utils.rs`:

```rust
/// Scale from 4095 (12-bit) to 16383 (14-bit)
pub fn scale_bits_12_14(value: u16) -> u16 {
    ((value as u32 * 16383) / 4095) as u16
}

/// Scale from 16383 (14-bit) to 4095 (12-bit)
pub fn scale_bits_14_12(value: u16) -> u16 {
    ((value as u32 * 4095) / 16383) as u16
}
```

### Configurator Changes

- New toggle input component for NRPN mode parameter
- `MidiCc` input component conditionally allows 0-16383 range when NRPN is enabled on the same app
- TypeScript bindings regenerated via `gen-bindings.sh` after libfp type changes

## Throttle Considerations

The MIDI distributor throttles to 1 message per 2ms. NRPN requires 2-4 CC messages per update:
- Full sequence (new param): 4 messages = ~8ms
- Cached (same param): 2 messages = ~4ms

Both are fast enough for smooth fader control. The caching optimization is important for keeping latency low during rapid value changes.

## Files Affected

- `libfp/src/lib.rs` — new `Param::MidiNrpn` variant, widen `MidiCc` range
- `libfp/src/utils.rs` — new scaling functions
- `faderpunk/src/tasks/midi.rs` — `NrpnTracker`, `MidiEvent` enum, assembly logic
- `faderpunk/src/app.rs` — `MidiOutput` NRPN send + caching, `MidiInput` NRPN filtering
- `faderpunk/src/events.rs` — PubSub type change to `MidiEvent`
- `faderpunk/src/apps/*.rs` — opt-in NRPN param for MIDI-capable apps
- `configurator/src/` — NRPN toggle component, conditional MidiCc range
- `gen-bindings/` — regenerate TypeScript bindings
