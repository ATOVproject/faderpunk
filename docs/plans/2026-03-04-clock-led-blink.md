# Clock LED Blink Overlay Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a `ClockFlash` LED overlay effect that pulses bright on each quarter-note beat and holds a constant dim glow between beats, driven by a shared `AtomicBool` owned by the clock task.

**Architecture:** A new `pub static CLOCK_FLASH_HIGH: AtomicBool` in `leds.rs` acts as a one-directional signal — the new `run_clock_led_sync` task in `clock.rs` writes it (true on beat, false after 80ms), and `LedEffect::ClockFlash` only reads it. Apps activate the effect via the existing overlay API.

**Tech Stack:** Embedded Rust, Embassy async, `portable_atomic`, `no_std`

---

## Reference: Design Doc

See `docs/plans/2026-03-04-clock-led-overlay-design.md` for full rationale.

---

### Task 1: Add `CLOCK_FLASH_HIGH` and the `ClockFlash` LED effect

**Files:**
- Modify: `faderpunk/src/tasks/leds.rs`

**Step 1: Add `AtomicBool` to the `portable_atomic` import**

Find the existing import (line ~11):
```rust
use portable_atomic::{AtomicU8, Ordering};
```
Change to:
```rust
use portable_atomic::{AtomicBool, AtomicU8, Ordering};
```

**Step 2: Add the `CLOCK_FLASH_HIGH` static**

After `pub static LED_BRIGHTNESS: AtomicU8 = ...` (line ~21), add:
```rust
pub static CLOCK_FLASH_HIGH: AtomicBool = AtomicBool::new(false);
```

**Step 3: Add `ClockFlash` to `LedMode`**

In the `LedMode` enum (currently has Static, FadeOut, Flash, StaticFade), add:
```rust
ClockFlash(Color, Brightness),
```

**Step 4: Add `ClockFlash` to `LedEffect`**

In the `LedEffect` enum, add:
```rust
ClockFlash {
    color: RGB8,
    brightness: u8,
},
```

**Step 5: Add the `into_effect()` arm**

In `LedMode::into_effect()`, add a match arm:
```rust
LedMode::ClockFlash(color, brightness) => LedEffect::ClockFlash {
    color: color.into(),
    brightness: brightness.into(),
},
```

**Step 6: Add the `update()` arm**

In `LedEffect::update()`, add a match arm. The dim level is 30/255 (~12%), a named constant makes it easy to tune:

```rust
LedEffect::ClockFlash { color, brightness } => {
    const DIM: u8 = 30;
    if CLOCK_FLASH_HIGH.load(Ordering::Relaxed) {
        color.scale(*brightness)
    } else {
        color.scale(DIM)
    }
}
```

**Step 7: Verify it compiles**

```bash
cd faderpunk && cargo check
```
Expected: no errors. If `color.scale()` doesn't exist on `RGB8`, check `libfp::ext::BrightnessExt` — the trait is already in scope via the existing imports.

**Step 8: Commit**

```bash
git add faderpunk/src/tasks/leds.rs
git commit -m "feat(leds): add ClockFlash overlay effect"
```

---

### Task 2: Add `run_clock_led_sync` task to manage the AtomicBool

**Files:**
- Modify: `faderpunk/src/tasks/clock.rs`

**Step 1: Add `AtomicBool` to the `portable_atomic` import**

Find the existing import (line ~17):
```rust
use portable_atomic::{AtomicU64, Ordering};
```
Change to:
```rust
use portable_atomic::{AtomicBool, AtomicU64, Ordering};
```

**Step 2: Bump `CLOCK_PUBSUB_SUBSCRIBERS` from 16 to 17**

Find (line ~34):
```rust
const CLOCK_PUBSUB_SUBSCRIBERS: usize = 16;
```
Change to:
```rust
const CLOCK_PUBSUB_SUBSCRIBERS: usize = 17;
```
The comment above it says "16 apps" — update it to "16 apps + 1 clock LED sync".

**Step 3: Add the flash duration constant**

Near the other constants at the top of the file, add:
```rust
/// How long the CLOCK_FLASH_HIGH signal stays true after each beat (ms).
const CLOCK_FLASH_HIGH_MS: u64 = 80;
```

**Step 4: Add the `run_clock_led_sync` task**

Add this new task anywhere in the file (e.g. just before `run_clock_gatekeeper`):

```rust
#[embassy_executor::task]
async fn run_clock_led_sync() {
    use crate::tasks::leds::CLOCK_FLASH_HIGH;

    let mut sub = CLOCK_PUBSUB.subscriber().unwrap();
    let mut tick_count: u64 = 0;

    loop {
        match sub.next_message().await {
            embassy_sync::pubsub::WaitResult::Message(event) => match event {
                ClockEvent::Tick => {
                    tick_count += 1;
                    // Fire on the first tick of each quarter note (every 24 ppqn ticks).
                    if tick_count % 24 == 1 {
                        CLOCK_FLASH_HIGH.store(true, Ordering::Relaxed);
                        Timer::after_millis(CLOCK_FLASH_HIGH_MS).await;
                        CLOCK_FLASH_HIGH.store(false, Ordering::Relaxed);
                    }
                }
                ClockEvent::Start | ClockEvent::Reset => {
                    tick_count = 0;
                    CLOCK_FLASH_HIGH.store(false, Ordering::Relaxed);
                }
                ClockEvent::Stop => {
                    CLOCK_FLASH_HIGH.store(false, Ordering::Relaxed);
                }
            },
            // If we lagged (missed messages during the 80ms timer wait), skip and continue.
            embassy_sync::pubsub::WaitResult::Lagged(_) => {}
        }
    }
}
```

**Why `tick_count % 24 == 1`:** After a Start/Reset, `tick_count` is zeroed. The next tick increments it to 1. `1 % 24 == 1` is true, so the flash fires on the very first tick of the new bar. Subsequent beats fire at tick_count 25, 49, 73, etc.

**Step 5: Spawn the task in `start_clock`**

In the `start_clock` function (line ~107), add the spawn:
```rust
pub async fn start_clock(spawner: &Spawner, aux_inputs: AuxInputs) {
    spawner.spawn(run_clock_sources(aux_inputs)).unwrap();
    spawner.spawn(run_clock_gatekeeper()).unwrap();
    spawner.spawn(run_clock_led_sync()).unwrap();  // <-- add this
}
```

**Step 6: Verify it compiles**

```bash
cd faderpunk && cargo check
```
Expected: no errors.

**Step 7: Commit**

```bash
git add faderpunk/src/tasks/clock.rs
git commit -m "feat(clock): add run_clock_led_sync to drive CLOCK_FLASH_HIGH AtomicBool"
```

---

### Task 3: Full release build and hardware test

**Step 1: Release build**

```bash
cd faderpunk && cargo build --release
```
Expected: builds successfully, UF2 ready to generate.

**Step 2: Generate UF2**

```bash
cd .. && ./build-uf2.sh
```

**Step 3: Flash and test**

Hold SHIFT (bottom-right yellow button), connect USB, copy UF2 file to the device.

**Manual test checklist:**
- Start the internal clock (play button)
- Activate `ClockFlash` on a button LED from an app that calls `set_led_overlay_mode(channel, Led::Button, LedMode::ClockFlash(Color::White, Brightness::Full))`
- Verify: bright flash on each quarter note, dim glow between beats
- Change BPM — verify the flash rate tracks correctly
- Stop the clock — verify LED settles to dim (not bright)
- Reset — verify flash fires on the very next tick when restarted

**Step 4: Final commit (if any fixups needed)**

```bash
git add -p
git commit -m "fix(clock-leds): <describe fixup>"
```
