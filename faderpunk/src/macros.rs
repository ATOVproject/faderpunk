macro_rules! register_apps {
    ($($id:literal => $app_mod:ident @ $bytes:literal),+ $(,)?) => {
        $(
            mod $app_mod;
        )*

        use embassy_sync::{
            blocking_mutex::raw::{NoopRawMutex},
            signal::Signal,
        };

        use libfp::ConfigMeta;
        use crate::{I2C_LEADER_PUBLISHER, MAX_CHANNEL, APP_MIDI_CHANNEL};
        use crate::{app::App, events::EVENT_PUBSUB, tasks::midi::{MIDI_DIN_PUBSUB, MIDI_USB_PUBSUB}};
        use embassy_executor::Spawner;
        use portable_atomic::{AtomicBool, AtomicU32, Ordering};

        const _APP_COUNT: usize = {
            let mut count = 0;
            $(
                // Use each ID to force expansion
                let _ = $id;
                count += 1;
            )*
            count
        };

        pub const REGISTERED_APP_IDS: [u8; _APP_COUNT] = [$($id),*];

        /// Per-app_id embassy-executor task pool cost in bytes. Generated
        /// from the same $id list as REGISTERED_APP_IDS, in the same macro
        /// expansion, so the two can never disagree on which app_ids exist
        /// — adding an app without a `@ <bytes>` literal is a macro parse
        /// error, not a silent gap in this table.
        ///
        /// The *value* can still go stale relative to the real, current
        /// size of that app's generated future (grows/shrinks whenever the
        /// app's own `run()` state does) — nothing in the type system
        /// catches that, only re-measuring does. See
        /// `layout_exceeds_arena_budget`'s doc comment for how these were
        /// measured and how to re-measure them.
        ///
        /// While developing: any `u32` compiles, so use `40_000` as a
        /// placeholder (safely above every current entry) rather than
        /// guessing low or leaving `0` — a low placeholder quietly reopens
        /// this bug even during your own dev loop. Measure for real before
        /// the app ships.
        pub const BUILTIN_POOL_BYTES: [(u8, u32); _APP_COUNT] = [$(($id, $bytes)),*];

        /// Must track `faderpunk/Cargo.toml`'s `task-arena-size-*` feature
        /// exactly — nothing enforces that automatically today.
        const TASK_ARENA_SIZE: u32 = 393_216;
        /// Fixed system tasks (buttons, leds, fram, i2c, max, midi, clock,
        /// global_config, transport, the two executor entry tasks) always
        /// spawned once at boot — measured the same way as
        /// BUILTIN_POOL_BYTES (readelf --debug-dump=info on a release
        /// build, TaskPool<F, N> DW_AT_byte_size).
        const SYSTEM_TASK_BYTES: u32 = 30_456;
        /// run_fpapp's pool (fpapp_runtime::__run_fpapp_task, pool_size
        /// GLOBAL_CHANNELS) — one allocation, on the first FPApp spawn
        /// ever, any slot or app. Measured the same way; notably larger
        /// than an earlier ~181 KiB estimate that turns out to have been
        /// stale.
        const FPAPP_POOL_BYTES: u32 = 206_720;
        /// Arena::alloc pads each allocation up to the type's alignment
        /// (next_multiple_of), so the raw sum of measured sizes slightly
        /// under-reports true consumption. All 28 measured types (27
        /// built-ins + run_fpapp) align to 8 bytes, so worst case is
        /// 7 bytes x 28 types = 196 B; rounded up generously.
        const SAFETY_MARGIN_BYTES: u32 = 512;

        /// Bit i set = built-in app_id i has had a spawn attempted since
        /// boot — i.e. its embassy-executor task pool has already been
        /// carved out of the arena. "Attempted" and "pool exists" are the
        /// same event regardless of whether the spawn then succeeds, since
        /// the allocation happens while evaluating the wrapper(...)
        /// argument, before Spawner::spawn is entered.
        ///
        /// Deliberately RAM-only, never persisted: must reset to 0 every
        /// boot, mirroring the arena static itself. Compare
        /// fpapps::QUARANTINED_SLOTS, which mirrors into persisted
        /// RuntimeState — this one must not.
        static SPAWNED_BUILTIN_TYPES: AtomicU32 = AtomicU32::new(0);
        /// Same idea, one bit: has any FPApp (any slot, any app) been
        /// spawned this boot yet. The run_fpapp pool is a single one-time
        /// cost, not per-type, but it's exactly the same "first spawn
        /// ever" hazard as a built-in type and must count in the same
        /// budget check.
        static FPAPP_POOL_SPAWNED: AtomicBool = AtomicBool::new(false);

        /// True if spawning `layout` as-is could panic inside
        /// embassy-executor's Arena::alloc — the combined cost of every
        /// type (built-in or first-ever FPApp) not yet spawned this boot
        /// would exceed what's left of the task arena.
        ///
        /// Only ever call from `ConfigMsgIn::SetLayout`
        /// (tasks/configure.rs) — never from `LayoutManager::spawn_layout`
        /// itself: every type is unseen at boot by construction (this
        /// bitmask starts at 0 every boot), so a check inside
        /// `spawn_layout` would see "unseen type" on the very first call
        /// and reboot into the identical boot state, looping forever.
        /// `SetLayout` is the one point where "layout wants an unseen
        /// type" and "we haven't committed to spawning it yet" both hold.
        ///
        /// Recomputed from the bitmask/flag each call rather than a
        /// separately maintained running counter, so there's one source of
        /// truth, not two that can drift. Every other path that can reach
        /// a spawn (boot's initial send, respawn_all, spawn_one for V/Oct
        /// eviction-restore) only ever re-spawns types whose bit/flag is
        /// already set, so none of them can add to `used` between this
        /// scan and a reboot — no lock needed.
        ///
        /// See the register_apps! docs above for how BUILTIN_POOL_BYTES,
        /// SYSTEM_TASK_BYTES and FPAPP_POOL_BYTES are measured and how to
        /// keep them current.
        pub fn layout_exceeds_arena_budget(layout: &libfp::Layout) -> bool {
            let spawned = SPAWNED_BUILTIN_TYPES.load(Ordering::Relaxed);
            let fpapp_spawned = FPAPP_POOL_SPAWNED.load(Ordering::Relaxed);

            let used = SYSTEM_TASK_BYTES
                + BUILTIN_POOL_BYTES
                    .iter()
                    .filter(|(id, _)| spawned & (1 << id) != 0)
                    .map(|(_, bytes)| bytes)
                    .sum::<u32>()
                + if fpapp_spawned { FPAPP_POOL_BYTES } else { 0 };

            let needs_fpapp_pool =
                !fpapp_spawned && layout.iter().any(|(id, ..)| id >= 100);
            let needed: u32 = BUILTIN_POOL_BYTES
                .iter()
                .filter(|(id, _)| {
                    layout.iter().any(|(lid, ..)| lid == *id) && spawned & (1 << id) == 0
                })
                .map(|(_, bytes)| bytes)
                .sum::<u32>()
                + if needs_fpapp_pool { FPAPP_POOL_BYTES } else { 0 };

            // SAFETY_MARGIN_BYTES absorbs alignment padding Arena::alloc
            // adds that this sum doesn't account for. Comparing against
            // "arena minus margin" rather than adding the margin to
            // `needed`, so it's spent once regardless of how many types
            // are being newly spawned this call.
            used + needed > TASK_ARENA_SIZE.saturating_sub(SAFETY_MARGIN_BYTES)
        }

        /// Returns whether the app actually started. A spawn fails when the
        /// task pool for that app is exhausted; the caller must not record the
        /// channel as occupied in that case, or the layout would claim an app
        /// that is not running and no later respawn would correct it.
        #[must_use]
        pub fn spawn_app_by_id(
            app_id: u8,
            start_channel: usize,
            layout_id: u8,
            spawner: Spawner,
            exit_signals: &'static [Signal<NoopRawMutex, bool>; 16],
            completion_signals: &'static [Signal<NoopRawMutex, ()>; 16],
        ) -> bool {
            match app_id {
                $(
                    $id => {
                        const _: () = assert!($id < 32, "built-in app_id exceeds u32 bitmask width — widen SPAWNED_BUILTIN_TYPES");
                        // Set unconditionally, before the spawn attempt: the
                        // arena cost is paid on attempt (Arena::alloc runs
                        // while evaluating the wrapper(...) argument below,
                        // before spawner.spawn is even entered), regardless
                        // of whether the spawn then succeeds. Gating on
                        // success would make a type whose pool exists but
                        // is momentarily full of instances read as
                        // never-spawned, causing a pointless reboot later.
                        SPAWNED_BUILTIN_TYPES.fetch_or(1 << $id, Ordering::Relaxed);

                        let app = App::<{ $app_mod::CHANNELS }>::new(
                            app_id,
                            start_channel,
                            layout_id,
                            &EVENT_PUBSUB,
                            I2C_LEADER_PUBLISHER,
                            MAX_CHANNEL.sender(),
                            APP_MIDI_CHANNEL.sender(),
                            &MIDI_DIN_PUBSUB,
                            &MIDI_USB_PUBSUB,
                        );

                        completion_signals[start_channel].reset();
                        if spawner
                            .spawn($app_mod::wrapper(
                                app,
                                &exit_signals[start_channel],
                                &completion_signals[start_channel],
                            ))
                            .is_err()
                        {
                            defmt::warn!(
                                "no free task slot for app {} on channel {}",
                                app_id,
                                start_channel
                            );
                            return false;
                        }
                    },
                )*
                _ => {
                    if let Some(descriptor) = crate::fpapps::runtime_descriptor(app_id) {
                        // Same reasoning as SPAWNED_BUILTIN_TYPES above: the
                        // arena cost is paid on attempt, not success.
                        FPAPP_POOL_SPAWNED.store(true, Ordering::Relaxed);
                        completion_signals[start_channel].reset();
                        if spawner
                            .spawn(crate::fpapp_runtime::run_fpapp(
                                descriptor,
                                start_channel,
                                layout_id,
                                &exit_signals[start_channel],
                                &completion_signals[start_channel],
                            ))
                            .is_err()
                        {
                            defmt::warn!(
                                "no free task slot for installable app {} on channel {}",
                                app_id,
                                start_channel
                            );
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
            }
            true
        }

        pub fn get_channels(app_id: u8) -> Option<usize> {
            match app_id {
                $(
                    $id => Some($app_mod::CHANNELS),
                )*
                _ => crate::fpapps::get_channels(app_id),
            }
        }

        pub fn get_config<'a, F: libfp::fpapp_store::SlotFlash>(
            app_id: u8,
            store: &'a libfp::fpapp_store::SlotStore<F>,
        ) -> Option<(u8, usize, ConfigMeta<'a>)> {
            match app_id {
                $(
                    $id => {
                        Some((app_id, $app_mod::CHANNELS, $app_mod::CONFIG.get_meta()))
                    },
                )*
                _ => crate::fpapps::get_config(app_id, store),
            }
        }
    };
}
