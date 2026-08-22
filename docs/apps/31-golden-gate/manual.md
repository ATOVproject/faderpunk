Golden Gate is a one-channel gate / pitch generator whose hit spacing comes from the Fibonacci sequence (1, 1, 2, 3, 5, 8, 13, and so on). Successive Fibonacci ratios converge on φ (the golden ratio ≈ 1.618), so as the pattern uses deeper gaps the rhythm feels increasingly “golden” — still built from whole-number steps, not continuous φ timing by default.

At shallow depth the hits are denser and more regular; deeper Fibonacci values open larger holes, so the same cycle length feels sparser and more asymmetric. Reverse plays the gaps of this filled cycle backwards (not the whole table from its largest value). In pitch modes the jack carries 1V/oct while MIDI sends related notes — either chromatic Fibonacci intervals (12-TET) or near-φ intervals (~833 cents × gap), with MIDI using nearest note + pitch bend (±2 semitone bend range assumed).

Needs clock. Jack can be Gate Out, Pitch Out, or CV In for depth / cycle / reset modulation.
