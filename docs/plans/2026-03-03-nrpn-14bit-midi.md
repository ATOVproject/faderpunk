# NRPN / 14-bit MIDI Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add bidirectional NRPN support so Faderpunk apps can send/receive high-resolution 14-bit MIDI parameter data (e.g. for Elektron devices) while keeping the internal 0-4095 value API unchanged.

**Architecture:** NRPN assembly/disassembly happens in the MIDI task layer (Core 0). A new `MidiEvent` wrapper enum replaces `LiveEvent<'static>` on PubSub channels. Apps opt in via a per-app `Param::MidiNrpn` boolean toggle. The `MidiOutput` caches the last NRPN parameter number to skip redundant CC 98/99 messages.

**Tech Stack:** Embedded Rust (no_std), Embassy async, midly MIDI library, React/TypeScript configurator, postcard serialization + COBS framing.

**Design doc:** `docs/plans/2026-03-03-nrpn-14bit-midi-design.md`

---

### Task 1: Add scaling utilities to libfp

**Files:**
- Modify: `libfp/src/utils.rs:16-18` (after existing `scale_bits_7_12`)

**Step 1: Add 12↔14 bit scaling functions**

Add after `scale_bits_7_12` (line 18):

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

**Step 2: Verify it compiles**

Run from `faderpunk/`: `cargo check`
Expected: compiles with no new errors.

**Step 3: Commit**

```bash
git add libfp/src/utils.rs
git commit -m "feat: add 12-bit to 14-bit scaling utilities for NRPN support"
```

---

### Task 2: Add `Param::MidiNrpn` and widen `MidiCc` to u16

This task adds the NRPN toggle parameter type and changes `MidiCc` from wrapping `u8` to `u16` so it can hold NRPN parameter numbers (0-16383).

**Files:**
- Modify: `libfp/src/lib.rs`

**Step 1: Widen `MidiCc` from u8 to u16**

`MidiCc` is defined around line 957. Change:

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, PostcardBindings)]
pub struct MidiCc(u16);
```

Update `From<u8>` to also support `From<u16>`:

```rust
impl From<u8> for MidiCc {
    fn from(value: u8) -> Self {
        Self(value as u16)
    }
}

impl From<u16> for MidiCc {
    fn from(value: u16) -> Self {
        Self(value.min(16383))
    }
}
```

The existing `From<MidiCc> for u7` still works (lossy conversion for CC mode). Add a method to get the full u16 value:

```rust
impl MidiCc {
    pub fn as_u16(&self) -> u16 {
        self.0
    }
}
```

**Step 2: Add `Param::MidiNrpn` variant**

In the `Param` enum (around line 662), add after the existing MIDI params:

```rust
MidiNrpn,
```

In the `Value` enum (around line 716), add:

```rust
MidiNrpn(bool),
```

Add `FromValue` for bool in the NRPN context — this should already work since `bool` has `FromValue`. The `MidiNrpn` param represents a simple on/off toggle.

**Step 3: Verify it compiles**

Run from `faderpunk/`: `cargo check`
Expected: May get warnings about unused variants. Fix any compilation errors related to exhaustive match statements on `Param` or `Value` — add match arms for the new variants wherever needed.

**Step 4: Fix exhaustive matches**

Search the codebase for `match` on `Param` and `Value` enums and add the new variants. Key locations:
- `libfp/src/lib.rs` — any match on `Param` or `Value`
- `faderpunk/src/app.rs` — param deserialization
- `configurator/` will be handled in a later task (TypeScript side)

**Step 5: Commit**

```bash
git add libfp/src/lib.rs
git commit -m "feat: add Param::MidiNrpn toggle and widen MidiCc to u16 for NRPN parameter numbers"
```

---

### Task 3: Add `MidiEvent` wrapper enum and `NrpnTracker`

This is the core NRPN receive-side logic. The `MidiEvent` wrapper replaces `LiveEvent<'static>` on the PubSub channels so apps can receive assembled NRPN events.

**Files:**
- Modify: `faderpunk/src/tasks/midi.rs`

**Step 1: Define `MidiEvent` enum**

Add near the top of the file (after the existing type definitions, around line 103):

```rust
#[derive(Clone, Copy)]
pub enum MidiEvent {
    Live(LiveEvent<'static>),
    Nrpn { channel: u4, param: u16, value: u16 },
}
```

**Step 2: Change PubSub channel types from `LiveEvent<'static>` to `MidiEvent`**

Update the three type aliases (lines 97-121):

```rust
pub type MidiPubSubChannel = PubSubChannel<
    CriticalSectionRawMutex,
    MidiEvent,  // was LiveEvent<'static>
    MIDI_PUBSUB_SIZE,
    MIDI_PUBSUB_SUBS,
    MIDI_PUBSUB_SENDERS,
>;

pub type MidiPubSubSubscriber = Subscriber<
    'static,
    CriticalSectionRawMutex,
    MidiEvent,  // was LiveEvent<'static>
    MIDI_PUBSUB_SIZE,
    MIDI_PUBSUB_SUBS,
    MIDI_PUBSUB_SENDERS,
>;

pub type MidiPubSubPublisher = Publisher<
    'static,
    CriticalSectionRawMutex,
    MidiEvent,  // was LiveEvent<'static>
    MIDI_PUBSUB_SIZE,
    MIDI_PUBSUB_SUBS,
    MIDI_PUBSUB_SENDERS,
>;
```

**Step 3: Implement `NrpnTracker`**

Add the tracker struct:

```rust
#[derive(Default)]
struct NrpnTracker {
    param_msb: Option<u8>,
    param_lsb: Option<u8>,
    value_msb: Option<u8>,
}

impl NrpnTracker {
    /// Process a CC message. Returns Some(MidiEvent::Nrpn) if a complete NRPN
    /// message has been assembled, or None if the CC was consumed (part of an
    /// NRPN sequence). If the CC is not NRPN-related, returns it wrapped in
    /// MidiEvent::Live.
    fn process_cc(&mut self, channel: u4, controller: u7, value: u7) -> Option<MidiEvent> {
        let cc = controller.as_int();
        match cc {
            99 => {
                // NRPN parameter MSB
                self.param_msb = Some(value.as_int());
                self.value_msb = None;
                None // consumed
            }
            98 => {
                // NRPN parameter LSB
                self.param_lsb = Some(value.as_int());
                self.value_msb = None;
                None // consumed
            }
            6 => {
                // Data Entry MSB
                if self.param_msb.is_some() && self.param_lsb.is_some() {
                    self.value_msb = Some(value.as_int());
                    None // consumed, waiting for CC 38
                } else {
                    // No NRPN context — pass through as normal CC
                    Some(MidiEvent::Live(LiveEvent::Midi {
                        channel,
                        message: MidiMessage::Controller { controller, value },
                    }))
                }
            }
            38 => {
                // Data Entry LSB
                if let Some(val_msb) = self.value_msb {
                    let param = ((self.param_msb.unwrap() as u16) << 7)
                        | (self.param_lsb.unwrap() as u16);
                    let nrpn_value = ((val_msb as u16) << 7) | (value.as_int() as u16);
                    self.value_msb = None;
                    Some(MidiEvent::Nrpn {
                        channel,
                        param,
                        value: nrpn_value,
                    })
                } else {
                    // No pending Data MSB — pass through as normal CC
                    Some(MidiEvent::Live(LiveEvent::Midi {
                        channel,
                        message: MidiMessage::Controller { controller, value },
                    }))
                }
            }
            _ => {
                // Not NRPN-related. If we were waiting for CC 38, flush a 7-bit NRPN.
                if let Some(val_msb) = self.value_msb.take() {
                    let param = ((self.param_msb.unwrap() as u16) << 7)
                        | (self.param_lsb.unwrap() as u16);
                    let nrpn_value = (val_msb as u16) << 7;
                    // Emit the 7-bit NRPN first. The current CC will be
                    // published separately after this returns.
                    // NOTE: This means we need to handle the "other CC"
                    // outside this function. Consider returning both events
                    // or restructuring the caller.
                    return Some(MidiEvent::Nrpn {
                        channel,
                        param,
                        value: nrpn_value,
                    });
                }
                // Normal CC pass-through
                Some(MidiEvent::Live(LiveEvent::Midi {
                    channel,
                    message: MidiMessage::Controller { controller, value },
                }))
            }
        }
    }

    /// Call this to flush any pending 7-bit NRPN (e.g. on timeout).
    fn flush(&mut self, channel: u4) -> Option<MidiEvent> {
        if let Some(val_msb) = self.value_msb.take() {
            let param = ((self.param_msb.unwrap_or(0) as u16) << 7)
                | (self.param_lsb.unwrap_or(0) as u16);
            Some(MidiEvent::Nrpn {
                channel,
                param,
                value: (val_msb as u16) << 7,
            })
        } else {
            None
        }
    }
}
```

**Step 4: Integrate NrpnTracker into `process_midi_event`**

The `process_midi_event` function (line 494) currently publishes `LiveEvent` directly. It needs to:
1. Accept `&mut [NrpnTracker; 16]` as an additional parameter
2. For `LiveEvent::Midi` with `MidiMessage::Controller`, route through the tracker
3. For all other events, wrap in `MidiEvent::Live` and publish

Update the function signature and body:

```rust
async fn process_midi_event(
    event: &LiveEvent<'_>,
    publisher: &MidiPubSubPublisher,
    nrpn_trackers: &mut [NrpnTracker; 16],
    thru_targets: [bool; 3],
    clock_src: ClockSrc,
    clock_in_sender: &Sender<'static, ThreadModeRawMutex, ClockInEvent, 16>,
    midi_sender: &Sender<'static, CriticalSectionRawMutex, MidiOutEvent, 16>,
) {
    match event {
        LiveEvent::Realtime(msg) => { /* unchanged clock handling */ },
        LiveEvent::Midi { channel, message } => {
            if let MidiMessage::Controller { controller, value } = message {
                let tracker = &mut nrpn_trackers[channel.as_int() as usize];
                if let Some(midi_event) = tracker.process_cc(*channel, *controller, *value) {
                    publisher.publish_immediate(midi_event);
                }
                // Still pass the raw CC through for MIDI thru
            } else {
                publisher.publish_immediate(MidiEvent::Live(event.to_static()));
            }
            // Passthrough for MIDI thru (send raw event regardless of NRPN)
            midi_sender
                .send(MidiOutEvent::Event(MidiMsg::new(
                    event.to_static(),
                    MidiOut(thru_targets),
                    MidiEventSource::Passthrough,
                )))
                .await;
        },
        _ => {
            publisher.publish_immediate(MidiEvent::Live(event.to_static()));
            midi_sender
                .send(MidiOutEvent::Event(MidiMsg::new(
                    event.to_static(),
                    MidiOut(thru_targets),
                    MidiEventSource::Passthrough,
                )))
                .await;
        }
    }
}
```

**Step 5: Initialize NrpnTracker array in `midi_in_task`**

In the `midi_in_task` function, add near the beginning:

```rust
let mut nrpn_trackers: [NrpnTracker; 16] = Default::default();
```

Pass `&mut nrpn_trackers` to all calls to `process_midi_event`.

**Step 6: Verify it compiles**

Run from `faderpunk/`: `cargo check`
Expected: Compilation errors in `app.rs` where `MidiInput` subscribes to PubSub and expects `LiveEvent`. These will be fixed in the next task.

**Step 7: Commit**

```bash
git add faderpunk/src/tasks/midi.rs
git commit -m "feat: add MidiEvent wrapper enum and NrpnTracker for NRPN receive assembly"
```

---

### Task 4: Update `MidiInput` to handle `MidiEvent`

`MidiInput` in `app.rs` currently receives `LiveEvent<'static>` from PubSub. Update it to receive `MidiEvent` and handle both CC and NRPN variants.

**Files:**
- Modify: `faderpunk/src/app.rs`

**Step 1: Update `MidiInput::wait_for_message`**

The method (around line 482) currently subscribes to PubSub and returns `MidiMessage`. It needs to unwrap `MidiEvent::Live` to get the inner `LiveEvent`, and also handle `MidiEvent::Nrpn`.

Update the return type or extend the existing `MidiMessage` handling. The simplest approach: `wait_for_message` continues to return `MidiMessage` from `midly`, but also needs a way to return NRPN events. Options:

**Recommended approach:** Add a new method `wait_for_nrpn` that returns NRPN events, and update `wait_for_message` to unwrap `MidiEvent::Live` events (skipping NRPN events). This way existing apps are unchanged.

```rust
// In MidiInput:

/// Wait for the next standard MIDI message (skips NRPN events)
pub async fn wait_for_message(&mut self) -> MidiMessage {
    loop {
        let midi_event = /* receive from pubsub */;
        match midi_event {
            MidiEvent::Live(LiveEvent::Midi { channel, message }) => {
                if channel == self.midi_channel {
                    return message;
                }
            }
            _ => continue, // skip NRPN and non-midi events
        }
    }
}

/// Wait for the next NRPN message matching the given parameter number.
/// Returns the 14-bit value scaled to 12-bit (0-4095).
pub async fn wait_for_nrpn(&mut self, param: u16) -> u16 {
    loop {
        let midi_event = /* receive from pubsub */;
        match midi_event {
            MidiEvent::Nrpn { channel, param: p, value } => {
                if channel == self.midi_channel && p == param {
                    return scale_bits_14_12(value);
                }
            }
            _ => continue,
        }
    }
}
```

**Important:** Since both methods need to receive from the same PubSub subscriber, and an app in NRPN mode needs to listen for both standard messages AND NRPN, consider a unified method that returns an enum:

```rust
pub enum AppMidiEvent {
    Message(MidiMessage),
    Nrpn { param: u16, value: u16 },
}

/// Wait for any MIDI event (standard or NRPN) on this channel
pub async fn wait_for_event(&mut self) -> AppMidiEvent {
    loop {
        let midi_event = /* receive from pubsub */;
        match midi_event {
            MidiEvent::Live(LiveEvent::Midi { channel, message }) => {
                if channel == self.midi_channel {
                    return AppMidiEvent::Message(message);
                }
            }
            MidiEvent::Nrpn { channel, param, value } => {
                if channel == self.midi_channel {
                    return AppMidiEvent::Nrpn { param, value: scale_bits_14_12(value) };
                }
            }
            _ => continue,
        }
    }
}
```

Keep the existing `wait_for_message` as a convenience that calls `wait_for_event` and filters. This ensures existing apps compile unchanged.

**Step 2: Update PubSub subscriber type references**

Anywhere `app.rs` references `LiveEvent<'static>` from the MIDI PubSub, update to `MidiEvent`. The `MidiPubSubSubscriber` type alias in `midi.rs` already changed (Task 3), so this should flow through.

**Step 3: Verify it compiles**

Run from `faderpunk/`: `cargo check`
Expected: Should compile now. Existing apps still call `wait_for_message()` which works as before.

**Step 4: Commit**

```bash
git add faderpunk/src/app.rs
git commit -m "feat: update MidiInput to handle MidiEvent wrapper, add wait_for_nrpn/wait_for_event"
```

---

### Task 5: Update `MidiOutput` for NRPN send with caching

Add NRPN output capability to `MidiOutput` in `app.rs`.

**Files:**
- Modify: `faderpunk/src/app.rs`

**Step 1: Add NRPN state to `MidiOutput`**

The `MidiOutput` struct (around line 368) needs to track the last sent NRPN parameter for caching:

```rust
pub struct MidiOutput {
    start_channel: usize,
    midi_channel: u4,
    midi_out: MidiOut,
    midi_sender: AppMidiSender,
    nrpn_mode: bool,
    last_nrpn_param: Option<u16>,  // for output caching
}
```

Update the constructor accordingly (add `nrpn_mode` parameter).

**Step 2: Add `send_nrpn` method**

```rust
/// Send an NRPN value. `param` is 0-16383, `value` is 0-4095 (scaled to 14-bit).
/// Caches the parameter number — skips CC 98/99 if unchanged.
pub async fn send_nrpn(&mut self, param: u16, value: u16) {
    let value_14 = scale_bits_12_14(value);
    let param_msb = u7::new((param >> 7) as u8);
    let param_lsb = u7::new((param & 0x7F) as u8);
    let value_msb = u7::new((value_14 >> 7) as u8);
    let value_lsb = u7::new((value_14 & 0x7F) as u8);

    // Only send parameter number if changed
    if self.last_nrpn_param != Some(param) {
        self.send_midi_msg(MidiMessage::Controller {
            controller: u7::new(99),
            value: param_msb,
        }).await;
        self.send_midi_msg(MidiMessage::Controller {
            controller: u7::new(98),
            value: param_lsb,
        }).await;
        self.last_nrpn_param = Some(param);
    }

    // Always send data entry
    self.send_midi_msg(MidiMessage::Controller {
        controller: u7::new(6),
        value: value_msb,
    }).await;
    self.send_midi_msg(MidiMessage::Controller {
        controller: u7::new(38),
        value: value_lsb,
    }).await;
}
```

**Step 3: Update `send_cc` to dispatch based on NRPN mode**

Modify the existing `send_cc` method (around line 400):

```rust
pub async fn send_cc(&mut self, cc: MidiCc, value: u16) {
    if self.nrpn_mode {
        self.send_nrpn(cc.as_u16(), value).await;
    } else {
        let msg = MidiMessage::Controller {
            controller: cc.into(),
            value: scale_bits_12_7(value),
        };
        self.send_midi_msg(msg).await;
    }
}
```

Note: `send_cc` signature changes from `&self` to `&mut self` because of the NRPN cache state. Update all call sites in apps accordingly.

**Step 4: Update `use_midi_output` factory method**

In `App::use_midi_output` (around line 738), add `nrpn_mode` parameter:

```rust
pub fn use_midi_output(&self, midi_out: MidiOut, midi_channel: MidiChannel, nrpn_mode: bool) -> MidiOutput {
    MidiOutput::new(midi_out, self.start_channel, midi_channel.into(), self.midi_sender, nrpn_mode)
}
```

**Step 5: Fix all existing `use_midi_output` call sites**

All existing apps that call `use_midi_output` need to pass `false` for `nrpn_mode`. These apps use `MidiCc` params (from the search earlier):

- `faderpunk/src/apps/control.rs`
- `faderpunk/src/apps/cv2midi.rs`
- `faderpunk/src/apps/lfo.rs`
- `faderpunk/src/apps/lfo_plus.rs`
- `faderpunk/src/apps/panner.rs`
- `faderpunk/src/apps/rndcvcc.rs`
- `faderpunk/src/apps/turing.rs`
- `faderpunk/src/apps/clkturing.rs`

Also fix any apps calling `midi_out.send_cc()` — the `&self` to `&mut self` change means `midi_out` must be declared `mut`.

**Step 6: Verify it compiles**

Run from `faderpunk/`: `cargo check`

**Step 7: Commit**

```bash
git add faderpunk/src/app.rs faderpunk/src/apps/
git commit -m "feat: add NRPN output with caching to MidiOutput, update all call sites"
```

---

### Task 6: Add NRPN toggle to one pilot app (`control.rs`)

Pick the `control` app as the first app to fully support NRPN. This validates the end-to-end flow.

**Files:**
- Modify: `faderpunk/src/apps/control.rs`

**Step 1: Add NRPN parameter to CONFIG**

In the CONFIG definition (line 22), add after `Param::MidiOut` (line 68):

```rust
.add_param(Param::MidiNrpn)
```

Update `PARAMS` constant from 12 to 13.

**Step 2: Add to Params struct**

```rust
pub struct Params {
    // ... existing fields ...
    nrpn: bool,
}
```

**Step 3: Update `from_values` and `to_values`**

Add `nrpn: bool::from_value(values[12])` to `from_values`.
Add `vec.push(Value::MidiNrpn(self.nrpn)).unwrap()` to `to_values`.

**Step 4: Update Default**

Add `nrpn: false` to the Default impl.

**Step 5: Pass NRPN mode to `use_midi_output`**

Where the app creates its MIDI output, pass `params.nrpn`:

```rust
let mut midi_out = app.use_midi_output(midi_out_param, midi_channel, params.nrpn);
```

**Step 6: Verify it compiles**

Run from `faderpunk/`: `cargo check`

**Step 7: Commit**

```bash
git add faderpunk/src/apps/control.rs
git commit -m "feat: add NRPN mode toggle to control app as pilot"
```

---

### Task 7: Add NRPN toggle to remaining MIDI-capable apps

Apply the same pattern from Task 6 to all other apps that use `MidiCc`.

**Files (all follow the same pattern as Task 6):**
- `faderpunk/src/apps/cv2midi.rs`
- `faderpunk/src/apps/lfo.rs`
- `faderpunk/src/apps/lfo_plus.rs`
- `faderpunk/src/apps/panner.rs` (has 2 MidiCc params — one NRPN toggle applies to both)
- `faderpunk/src/apps/rndcvcc.rs`
- `faderpunk/src/apps/turing.rs`
- `faderpunk/src/apps/clkturing.rs`
- `faderpunk/src/apps/midi2cv.rs` (receive-side — use NRPN mode to filter for `MidiEvent::Nrpn` instead of CC)

For each app:
1. Add `Param::MidiNrpn` to CONFIG
2. Increment PARAMS count
3. Add `nrpn: bool` to Params struct
4. Update `from_values`, `to_values`, Default
5. Pass `nrpn` mode to `use_midi_output` or use `wait_for_nrpn`/`wait_for_event` for receive-side apps

**For receive-side apps (midi2cv.rs):**
When `nrpn` is true, use `wait_for_event()` and match on `AppMidiEvent::Nrpn` instead of matching `MidiMessage::Controller`. The NRPN value is already scaled to 0-4095.

**Step: Verify it compiles**

Run from `faderpunk/`: `cargo check`

**Step: Commit**

```bash
git add faderpunk/src/apps/
git commit -m "feat: add NRPN mode toggle to all MIDI-capable apps"
```

---

### Task 8: Regenerate TypeScript bindings

The libfp type changes (new `Param::MidiNrpn`, `Value::MidiNrpn`, widened `MidiCc`) need to be reflected in the configurator.

**Files:**
- Modify: `gen-bindings/src/main.rs` (if new types need adding — check if `MidiNrpn` needs explicit inclusion)

**Step 1: Regenerate bindings**

Run from repository root:

```bash
./gen-bindings.sh
```

**Step 2: Verify the generated output**

Check that the generated TypeScript includes:
- `MidiNrpn` in the `Param` type
- `MidiNrpn` in the `Value` type
- `MidiCc` now uses a wider numeric type

**Step 3: Reinstall configurator dependencies**

```bash
rm -rf configurator/node_modules
cd configurator && pnpm install
```

**Step 4: Commit**

```bash
git add gen-bindings/ configurator/
git commit -m "feat: regenerate TypeScript bindings for NRPN types"
```

---

### Task 9: Add NRPN toggle component to configurator

**Files:**
- Create: `configurator/src/components/input/ParamMidiNrpn.tsx`
- Modify: `configurator/src/components/input/AppParam.tsx`
- Modify: `configurator/src/components/input/ParamMidiCc.tsx`

**Step 1: Create the NRPN toggle component**

Create `ParamMidiNrpn.tsx` following the pattern of other boolean param components:

```tsx
import { Props } from "./types";

export const ParamMidiNrpn = ({
  defaultValue,
  paramIndex,
  register,
}: Props) => (
  <label>
    <input
      type="checkbox"
      defaultChecked={!!defaultValue}
      {...register(`param-MidiNrpn-${paramIndex}`)}
    />
    NRPN Mode
  </label>
);
```

Look at how existing boolean params (e.g. `Param::bool`) are rendered in `AppParam.tsx` and follow that pattern exactly.

**Step 2: Register in AppParam dispatcher**

In `AppParam.tsx` (around line 135-163 where other MIDI params are dispatched), add the `MidiNrpn` case:

```tsx
case "MidiNrpn":
  return <ParamMidiNrpn {...commonProps} />;
```

**Step 3: Update ParamMidiCc for conditional range**

In `ParamMidiCc.tsx` (line 13-28), update `max` to be 16383 when NRPN is enabled. This requires the component to know about the NRPN state of the same app. Check how other params access sibling param state — likely through the form's watch/getValues API.

If cross-param awareness is complex, a simpler approach: always allow 0-16383 in the input. Values >127 simply won't work in CC mode (the firmware clamps). The configurator can add a visual hint.

**Step 4: Type check**

Run from `configurator/`:

```bash
pnpm tsc --noEmit
```

**Step 5: Test dev server**

```bash
pnpm dev
```

Verify the NRPN toggle shows up for apps that have the parameter. Verify MidiCc accepts wider range.

**Step 6: Commit**

```bash
git add configurator/src/components/input/
git commit -m "feat: add NRPN toggle component and update MidiCc range in configurator"
```

---

### Task 10: Build full firmware and verify

**Step 1: Full firmware build**

From repository root:

```bash
./build-uf2.sh
```

Expected: UF2 file generated successfully.

**Step 2: Clippy lint**

From `faderpunk/`:

```bash
cargo clippy
```

Fix any warnings.

**Step 3: Configurator production build**

From `configurator/`:

```bash
pnpm build
```

Expected: Builds without errors.

**Step 4: Commit any fixes**

```bash
git add -A
git commit -m "fix: address clippy warnings and build issues"
```
