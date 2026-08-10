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
        use crate::{I2C_LEADER_CHANNEL, MAX_CHANNEL, APP_MIDI_CHANNEL};
        use crate::{app::App, events::EVENT_PUBSUB, tasks::midi::{MIDI_DIN_PUBSUB, MIDI_USB_PUBSUB}};
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

        pub fn set_external_apps(
            apps: &'static [$crate::registry::ExternalAppDescriptor],
        ) {
            for (index, app) in apps.iter().enumerate() {
                assert!(app.id != 0, "external app id 0 is reserved");
                assert!(
                    !REGISTERED_APP_IDS.contains(&app.id),
                    "external app id collides with a built-in app"
                );
                assert!(
                    (1..=16).contains(&app.channels),
                    "external app channel count must be 1..=16"
                );
                assert!(
                    !apps[..index].iter().any(|other| other.id == app.id),
                    "duplicate external app id"
                );
            }
            $crate::registry::set_external_apps(apps);
        }

        pub fn registered_app_ids() -> impl Iterator<Item = u8> {
            REGISTERED_APP_IDS
                .into_iter()
                .chain($crate::registry::external_apps().iter().map(|app| app.id))
        }

        pub fn app_count() -> usize {
            _APP_COUNT + $crate::registry::external_apps().len()
        }

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
                            APP_MIDI_CHANNEL.sender(),
                            &MIDI_DIN_PUBSUB,
                            &MIDI_USB_PUBSUB,
                        );

                        spawner.spawn($app_mod::wrapper(app, &exit_signals[start_channel])).unwrap();
                    },
                )*
                _ => {
                    if let Some(app) = $crate::registry::external_app(app_id) {
                        app.spawn(start_channel, layout_id, spawner, exit_signals);
                    }
                }
            }
        }

        pub fn get_channels(app_id: u8) -> Option<usize> {
            match app_id {
                $(
                    $id => Some($app_mod::CHANNELS),
                )*
                _ => $crate::registry::external_app(app_id).map(|app| app.channels),
            }
        }

        pub fn get_config(app_id: u8) -> Option<(u8, usize, ConfigMeta<'static>)> {
            match app_id {
                $(
                    $id => {
                        Some((app_id, $app_mod::CHANNELS, $app_mod::CONFIG.get_meta()))
                    },
                )*
                _ => $crate::registry::external_app(app_id)
                    .map(|app| (app.id, app.channels, app.config())),
            }
        }
    };
}
