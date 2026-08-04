# PR 474 rebase — review notes

Branch `feat/combine-rebased` (commit 4514a9b7) squashes and rebases
[PR #474](https://github.com/ATOVproject/faderpunk/pull/474) ("Add CV Combine
and Gate Combine apps", by rjsmith) onto current `origin/main`. That PR's
original branch (`feature/combine`) had drifted far enough from `main` that a
normal commit-by-commit rebase wasn't practical — see the conversation this
file came from for the full account of what changed structurally during the
rebase (new non-persisted `JackRegistry` in `state.rs`, `GATE_LEVELS` replacing
the deleted `MAX_TRIGGERS_GPO`, etc).

`/code-review` was run against this branch after the rebase. Findings below,
none fixed yet.

## Real logic bugs in the new apps (pre-existing in the original PR)

1. **`gatecombine.rs:292`** — the probability gate re-rolls every 1ms instead
   of once per rising edge. `old_out_gate_was_high` is set from the
   *post*-probability output instead of the *pre*-probability combined
   signal, so a failed die roll doesn't stick. For a 20ms gate at 50%
   probability the effective pass rate is `1-(0.5)^20` ≈ 99.9999%, not 50%.
   The app's main control is close to non-functional for realistic gate
   lengths.

2. **`cvcombine.rs:297`** — "A − B" combine mode subtracts an extra 4095.
   `a_plus_5v - b_plus_5v - _10V` reduces to `a_in_v - b_in_v - 4095`, so a
   subtraction that should land near the top of the range gets clamped to the
   bottom instead. Copy-pasted from the Add mode's shift-cancellation trick,
   which doesn't apply here.

3. **`cvcombine.rs:298`** — "Max" combine mode has no single-channel special
   case (unlike Min and Average). A single active bipolar channel reading
   negative gets wrongly clamped to 0, because the disabled channel's forced
   "0V" midpoint beats the real negative value in the comparison.

4. **`cvcombine.rs:422`** (and duplicated at line 369) — fader-to-divisor
   (1–12) mapping truncates instead of rounds, biasing low across most of the
   fader's range. The documented "12 = semitones" setting is only reachable
   at the exact top ADC reading instead of a normal top-of-travel band.

## Gaps in the JackRegistry mechanism built during this rebase (mine to own)

5. **Confirmed — param-only respawns don't clear the registry.**
   `storage.rs:710-748`'s `param_handler` just `break`s its loop on a config
   change, which respawns `run()` fresh but never goes through
   `App::exit_handler`/`reset()` (only a full layout change triggers that).
   Any app that changes jack *type* via a plain parameter toggle — e.g.
   Turing's "Gate output" switch — leaves the old registry entry behind
   alongside the new one. Structural, not specific to Turing.

6. **`app.rs:808`** — `get_out_global_jack_value`'s doc comment claims
   pointing it at a gate jack "will return 0," but `MAX_VALUES_DAC` is never
   cleared on reset (only `GATE_LEVELS` and the registry are). A channel that
   used to run a CV app and now runs something else returns whatever stale
   voltage was last written there.

7. **Low-severity** — `layout.rs`'s `exit_app` 10ms grace timer was already a
   heuristic, not a real barrier, before this rebase. The new
   `clear_jacks().await` call added to `reset()` rides on that same soft
   deadline. Unlikely to bite in practice (uncontended mutex lock), but worth
   naming.

8. **Root cause of #5** — `JackRegistry` duplicates per-channel ownership
   info that `LayoutManager` already tracks, via convention only (nothing
   compiler-enforced ties a jack-type change to a registry update).

## Lower priority / style

9. `cvcombine.rs:244` — jack config sampled once at spawn, never refreshed;
   could race cold-boot spawn order or go stale if the sampled app's Range
   changes independently later.
10. Mute/LED toggle logic duplicated 3-4x per file instead of factored out;
    existing `Global<bool>::toggle()` helper goes unused.
11. `get_out_jacks()`/etc. copy the full 16-entry array by value instead of
    indexing under the lock — harmless at current call volumes.

## Open question

Items 1-4 are the contributor's own app logic — candidates to send back to
rjsmith rather than fix here. Items 5-6 are gaps in this session's own
rework and are probably worth fixing before this goes anywhere. Nothing here
has been fixed yet as of commit 4514a9b7.
