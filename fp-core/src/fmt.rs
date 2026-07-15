//! Logging facade: routes `info!`/`warn!`/`error!`/`debug!`/`trace!` to defmt
//! (feature `defmt`, firmware) or the `log` crate (feature `log`, simulator),
//! or compiles them out entirely. Format strings must stay compatible with
//! both backends: use `{}` only for primitives and `{:?}` for everything else.
#![allow(unused_macros)]

macro_rules! trace {
    ($s:literal $(, $x:expr)* $(,)?) => {{
        #[cfg(feature = "defmt")]
        ::defmt::trace!($s $(, $x)*);
        #[cfg(all(feature = "log", not(feature = "defmt")))]
        ::log::trace!($s $(, $x)*);
        #[cfg(not(any(feature = "defmt", feature = "log")))]
        let _ = ($( & $x ),*);
    }};
}

macro_rules! debug {
    ($s:literal $(, $x:expr)* $(,)?) => {{
        #[cfg(feature = "defmt")]
        ::defmt::debug!($s $(, $x)*);
        #[cfg(all(feature = "log", not(feature = "defmt")))]
        ::log::debug!($s $(, $x)*);
        #[cfg(not(any(feature = "defmt", feature = "log")))]
        let _ = ($( & $x ),*);
    }};
}

macro_rules! info {
    ($s:literal $(, $x:expr)* $(,)?) => {{
        #[cfg(feature = "defmt")]
        ::defmt::info!($s $(, $x)*);
        #[cfg(all(feature = "log", not(feature = "defmt")))]
        ::log::info!($s $(, $x)*);
        #[cfg(not(any(feature = "defmt", feature = "log")))]
        let _ = ($( & $x ),*);
    }};
}

macro_rules! warn {
    ($s:literal $(, $x:expr)* $(,)?) => {{
        #[cfg(feature = "defmt")]
        ::defmt::warn!($s $(, $x)*);
        #[cfg(all(feature = "log", not(feature = "defmt")))]
        ::log::warn!($s $(, $x)*);
        #[cfg(not(any(feature = "defmt", feature = "log")))]
        let _ = ($( & $x ),*);
    }};
}

macro_rules! error {
    ($s:literal $(, $x:expr)* $(,)?) => {{
        #[cfg(feature = "defmt")]
        ::defmt::error!($s $(, $x)*);
        #[cfg(all(feature = "log", not(feature = "defmt")))]
        ::log::error!($s $(, $x)*);
        #[cfg(not(any(feature = "defmt", feature = "log")))]
        let _ = ($( & $x ),*);
    }};
}
