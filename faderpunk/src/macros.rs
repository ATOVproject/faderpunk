macro_rules! register_apps {
    ($($id:literal => $app_mod:ident),+ $(,)?) => {
        $(
            mod $app_mod;
        )*

        use embassy_sync::{
            blocking_mutex::raw::{NoopRawMutex},
            signal::Signal,
        };

        use libfp::ConfigMeta;
        use crate::{I2C_LEADER_CHANNEL, MAX_CHANNEL, MIDI_CHANNEL};
        use crate::{app::App, events::EVENT_PUBSUB};
        use embassy_executor::Spawner;

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

        pub fn spawn_app_by_id(
            app_id: u8,
            start_channel: usize,
            layout_id: u8,
            spawner: Spawner,
            exit_signals: &'static [Signal<NoopRawMutex, bool>; 16]
        ) {
            match app_id {
                $(
                    $id => {
                        let app = App::<{ $app_mod::CHANNELS }>::new(
                            app_id,
                            start_channel,
                            layout_id,
                            &EVENT_PUBSUB,
                            I2C_LEADER_CHANNEL.sender(),
                            MAX_CHANNEL.sender(),
                            MIDI_CHANNEL.sender(),
                        );

                        spawner.spawn($app_mod::wrapper(app, &exit_signals[start_channel])).unwrap();
                    },
                )*
                _ => {
                    // Do nothing if app_id isn't valid
                }
            }
        }

        pub fn get_channels(app_id: u8) -> Option<usize> {
            match app_id {
                $(
                    $id => Some($app_mod::CHANNELS),
                )*
                _ => None,
            }
        }

        pub fn get_config(app_id: u8) -> Option<(u8, usize, ConfigMeta<'static>)> {
            match app_id {
                $(
                    $id => {
                        Some((app_id, $app_mod::CHANNELS, $app_mod::CONFIG.get_meta()))
                    },
                )*
                _ => None
            }
        }
    };
}
