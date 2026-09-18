// Each app's `@ <bytes>` is its measured embassy-executor task pool cost —
// see `register_apps!` in `macros.rs` for how and why. While developing,
// use 40_000 as a placeholder (safely above every entry below) and measure
// for real before shipping — too low silently reopens the bug this exists
// to close.
register_apps!(
    1 => control @ 22528,
    2 => lfo @ 27264,
    3 => ad @ 13504,
    4 => rnd @ 26368,
    5 => seq8 @ 7792,
    6 => turing @ 29312,
    7 => clkturing @ 13504,
    8 => euclid @ 12416,
    9 => probatrigger @ 24064,
    10 => notefader @ 27392,
    11 => offset_att @ 9728,
    12 => slew @ 9920,
    13 => follower @ 10304,
    14 => quantizer @ 10304,
    15 => midi2cv @ 25088,
    16 => cv2midi @ 20992,
    17 => cv2midinote @ 11008,
    18 => clk_div @ 25472,
    19 => panner @ 12672,
    20 => rnd_plus @ 14016,
    21 => clk_div_plus @ 14400,
    22 => lfo_plus @ 14080,
    23 => fp_grids @ 11072,
    24 => tb3po @ 9840,
    25 => automator @ 38656,
    26 => genseq @ 8280,
    27 => bernoulli @ 12352,
);
