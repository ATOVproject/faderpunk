# Plan: Bullet-proof latch refactor

## Context

The audit confirms the `AnalogLatch` state machine is structurally correct. All state transitions are consistent, `prev_target` is tracked in every branch, Scale mode arithmetic is safe, and the crossover detection is bidirectionally correct. The bugs are not in the logic — they are in the **signal quality** fed into the logic, and in one **hysteresis asymmetry** in the external-target-change path.

Two root causes remain:

1. **Single-sample ADC spikes** (charge injection from MAX11300's internal sweep when high-voltage input ports precede P16 in ContinuousSweep order) produce one corrupted reading every ~0.4 ms sweep cycle. With NSAMPLES=1, that bad reading goes directly into `latch.update()`. A spike > `pickup_tolerance` (25 LSBs) spanning the target causes involuntary latching; a spike > `jitter_tolerance` (50 LSBs) causes a false output when latched.

2. **Symmetric hysteresis in the external-target-change path.** Currently, both the "latch on entering" and "stay latched despite external change" checks use `in_pickup_zone` (25 LSBs). This is too easy to escape: a parameter externally perturbed by 26 LSBs unlatches the fader. The correct design is asymmetric — easy to enter, hard to leave.

---

## What the audit found is already correct (do not change)

- Crossover detection (`prev..=value` range + proximity) — bidirectionally correct
- `prev_target` tracking in all branches — complete, no missed paths
- Scale mode delta calculation and runway factor — arithmetically safe, no overflow
- Absolute-edge (0 / 4095) bypass — intentional and correct
- Scale mode zero-delta skip when fader held still — intentional
- Third layer (`target = 0`) — safe sentinel; max.rs never sends LatchLayer::Third in practice (scene button only toggles Main ↔ Alt)

---

## Change 1 — 3-sample rolling median filter (`faderpunk/src/tasks/max.rs`)

**Where:** in `read_fader()`, between the ADC read and the `latch.update()` call.

**Why median and not averaging:** A spike is a single outlier sample. The median of `[good, good, spike]` = `good`, so a spike is completely eliminated. Averaging only attenuates it by 1/N. Median has no lag for monotonic fader movement (all three samples move together); averaging always lags by `N/2` samples.

**Implementation:**

Add a `fader_history: [[u16; 2]; 16]` array alongside the existing `fader_latches` array. After scaling the raw ADC value, compute `median3(history[0], history[1], current)` and pass the result to `latch.update()`. Advance the history ring.

```rust
fn median3(a: u16, b: u16, c: u16) -> u16 {
    // Returns the middle of three values without branching-heavy sorting
    let lo = a.min(b).min(c);
    let hi = a.max(b).max(c);
    a + b + c - lo - hi
}
```

Memory cost: 16 channels × 2 u16 = 64 bytes. Negligible.

**Effect on Scale mode:** The `fader_delta` in Scale mode becomes `median(t) − median(t−1)` instead of `raw(t) − raw(t−1)`. Spike deltas (e.g. +60 LSBs from noise) become 0. Real fader movement still produces correct deltas with at most 16ms lag — imperceptible on a physical fader.

**History initialization:** At startup the init loop already reads all 16 fader positions. Initialise `fader_history[channel] = [value, value]` at the end of each init loop iteration, so the first main-loop median is `median(initial, initial, first_read)` — a clean start with no stale zeros.

**Critical path in `read_fader()`:**
```
read ADC → scale → median3(history[0], history[1], scaled) → latch.update(filtered, ...) → diff gate → publish
```

---

## Change 2 — Asymmetric hysteresis for "stay latched" (`libfp/src/latch.rs`)

**Current (symmetric, problematic):**
```rust
// External target change while latched
self.is_latched = in_pickup_zone(value, new_target)  // 25 LSBs — too easy to lose control
```

**New (asymmetric):**
```rust
// External target change while latched — use jitter zone (wider) to stay latched
self.is_latched = in_jitter_zone(value, new_target)  // 50 LSBs — harder to lose control
```

**Layer switch** keeps `in_pickup_zone` (25 LSBs) — an intentional layer switch should be precise.
**Crossover to latch** keeps `in_pickup_zone` (25 LSBs) — entry condition unchanged.
**Unlatched: target moves to fader** keeps `in_pickup_zone` (25 LSBs) — auto-latch only when genuinely coincident.

**Why this matters:** In Scale mode Alt layer, the target (`global_settings_fader_values`) can change as the user adjusts a value. If the target drifts by 26 LSBs (one increment + noise), the current code unlatches the fader. With this change, the target must drift 51+ LSBs before the fader loses control — matching the 50-LSB dead zone already in effect for latched tracking.

**File:** `libfp/src/latch.rs` — only the `else if self.is_latched` branch (the external-target-change check), specifically the `_ =>` arm of the `match self.mode` inside it.

---

## Change 3 — Update existing tests and add new ones (`libfp/src/latch.rs`)

**New tests needed for Change 2:**

- `test_stays_latched_when_target_changes_within_jitter_zone`: fader latched at 1000, target externally changes to 1040 (40 LSBs < jitter_tolerance 50) → stays latched, no emit.
- `test_unlatches_when_target_changes_beyond_jitter_zone`: fader latched at 1000, target changes to 1060 (60 LSBs > 50) → unlatches, no emit.
- `test_layer_switch_still_uses_pickup_zone`: explicit check that layer switch uses the smaller 25-LSB zone (not the wider jitter zone), so the symmetry break is tested directly.

**Existing tests that will need updating** (due to Change 2 changing the unlatch threshold from 25 to 50):
- `test_jitter_tolerance_on_target_change` (line ~367): This test checks that a target change of 8 LSBs (> old pickup_tolerance=3 in that test, since it uses `with_tolerance(100, 3, ...)`) unlatches. With asymmetric hysteresis, an 8-LSB target change on a latch with jitter_tolerance=3 STAYS latched. Need to verify what this test intends and update accordingly.

---

## Files to modify

| File | Change |
|---|---|
| `faderpunk/src/tasks/max.rs` | Add `fader_history: [[u16; 2]; 16]`, `median3()` fn, wire into read loop |
| `libfp/src/latch.rs` | Change one arm in `else if self.is_latched` branch: `in_pickup_zone` → `in_jitter_zone` |
| `libfp/src/latch.rs` (tests) | Add 3 new tests; audit `test_jitter_tolerance_on_target_change` for updated semantics |

---

## What this does NOT change (and why)

- `pickup_tolerance = 25`: Unchanged. Crossover detection stays precise.
- `jitter_tolerance = 50`: Unchanged. Already set to absorb charge injection transients.
- Scale mode delta logic: Unchanged. Median filter in max.rs fixes the spike-delta problem at the source.
- The crossover range check: Unchanged and correct.
- The `diff >= 4` gate in max.rs: Unchanged. Output filter, not latch condition.

---

## Verification

1. `cargo test -p libfp` — all tests pass.
2. **Spike immunity (latched):** Hold a fader still, inject a jack voltage. Fader output should be rock-stable with zero false CC emissions.
3. **Spike immunity (unlatched):** Set up Pickup mode, hold fader near target but don't cross. Apply jack voltage. Should not involuntarily latch.
4. **Scale mode scene button:** Hold scene button, move fader. Convergence should feel smooth and responsive, no jitter-induced jumps in global config values.
5. **Normal pickup:** Move fader deliberately across target. Should latch cleanly at the crossing point with no extra delay.
6. **External target change (asymmetric hysteresis):** Programmatically change the parameter while fader is latched. Small changes (< 50 LSBs) → fader stays in control. Large changes (> 50 LSBs) → fader unlatches and waits for re-pickup.
