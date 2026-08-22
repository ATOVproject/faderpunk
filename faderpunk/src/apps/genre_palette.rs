//! Shared genre labels + 8-bar tropes for Grooves, Chord Vamp, and Bassment.
//!
//! Scrub / commit LEDs use the open red→blue spectrum in [`super::led_fx::spectrum_color`] —
//! there is no discrete per-genre chrome on device.

pub const NUM_GENRES: usize = 9;

/// Morph axis (club spine → breaks → UK bass). Indices match Shift+Fader
/// buckets and Enum params — keep identical across apps.
pub const GENRE_NAMES: &[&str] = &[
    "Dub",
    "Disco",
    "House",
    "Techno",
    "Trip-Hop",
    "Hip-Hop",
    "Jungle",
    "UK Garage",
    "Dubstep",
];

/// Shared 8-bar genre tropes (scale degrees 0–6). First 4 ≈ statement;
/// bars 5–8 = answer / turnaround. Used by Chord Vamp + Bassment — keep in sync.
pub const GENRE_PROG_8: [[u8; 8]; NUM_GENRES] = [
    // Dub — i–IV–i–V | i–IV–V–i
    [0, 3, 0, 4, 0, 3, 4, 0],
    // Disco — I–vi–IV–V | I–IV–V–I
    [0, 5, 3, 4, 0, 3, 4, 0],
    // House — i–VII–VI–VII | i–VI–VII–i
    [0, 6, 5, 6, 0, 5, 6, 0],
    // Techno — pedal + rare V | pedal + drop
    [0, 0, 0, 4, 0, 0, 4, 0],
    // Trip-Hop — i–VII–VI–v | i–VI–v–i
    [0, 6, 5, 4, 0, 5, 4, 0],
    // Hip-Hop — i–VI–III–VII | i–III–VI–VII
    [0, 5, 2, 6, 0, 2, 5, 6],
    // Jungle — i–VII–VI–III | i–VI–III–VII
    [0, 6, 5, 2, 0, 5, 2, 6],
    // UK Garage — i–III–VI–VII | i–VI–III–VII
    [0, 2, 5, 6, 0, 5, 2, 6],
    // Dubstep — i–i–VI–VII | i–VI–VII–i
    [0, 0, 5, 6, 0, 5, 6, 0],
];

/// Fader position at the center of genre bucket `index` (seeds Alt latch target).
pub fn genre_fader_center(index: usize, picks: usize) -> u16 {
    let picks = picks.max(1);
    let i = index.min(picks - 1) as u32;
    let p = picks as u32;
    ((((i * 2) + 1) * 4095) / (p * 2)) as u16
}
