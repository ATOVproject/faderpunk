macro_rules! register_apps {
    ($($id:literal => $app_mod:ident),+ $(,)?) => {
        $(
            mod $app_mod;
        )*


        use embassy_futures::join::join;
        use embassy_time::Timer;
        use embassy_sync::{
            blocking_mutex::raw::{NoopRawMutex},
            signal::Signal,
        };

        use config::Param;
        use crate::{CMD_CHANNEL, EVENT_PUBSUB, HardwareEvent};
        use crate::app::App;
        use crate::storage::ParamStore;

        const _APP_COUNT: usize = {
            let mut count = 0;
            $(
                // Use each ID to force expansion
                let _ = $id;
                count += 1;
            )*
            count
        };

        pub const REGISTERED_APP_IDS: [usize; _APP_COUNT] = [$($id),*];

        async fn wait_for_eeprom_refresh() {
            loop {
                let mut subscriber = EVENT_PUBSUB.subscriber().unwrap();
                if let HardwareEvent::EepromRefresh = subscriber.next_message_pure().await {
                    // Wait a bit for all serialization to be done
                    Timer::after_millis(10).await;
                    return;
                }
            }
        }

        pub async fn run_app_by_id(
            app_id: u8,
            start_channel: usize,
        ) {
            match app_id {
                $(
                    $id => {
                        let param_values = ParamStore::new($app_mod::default_params(), app_id, start_channel);
                        let storage = $app_mod::get_storage(app_id, start_channel);
                        let context = $app_mod::AppContext::new(&param_values, storage);
                        let scene_signal: Signal<NoopRawMutex, u8> = Signal::new();

                        let sender = CMD_CHANNEL.sender();
                        let app = App::<{ $app_mod::CHANNELS }>::new(
                            app_id,
                            start_channel as usize,
                            sender,
                            &EVENT_PUBSUB,
                            &scene_signal,
                        );

                        let app_start_fut = async {
                            wait_for_eeprom_refresh().await;
                            $app_mod::run(app, &context).await;
                        };

                        join(
                            app_start_fut,
                            $app_mod::msg_loop(start_channel, &context, &scene_signal),
                        ).await;
                    },
                )*
                _ => panic!("Unknown app ID: {}", app_id),
            }
        }

        pub fn get_channels(app_id: usize) -> usize {
            match app_id {
                $(
                    $id => $app_mod::CHANNELS,
                )*
                _ => panic!("Unknown app ID: {}", app_id),
            }
        }

        pub fn get_config(app_id: usize) -> (usize, &'static str, &'static str, &'static [Param]) {
            match app_id {
                $(
                    $id => {
                        $app_mod::CONFIG.get_meta()
                    },
                )*
                _ => panic!("Unknown app ID: {}", app_id),
            }
        }
    };
}

macro_rules! app_config {
    (
        config($app_name:expr, $app_desc:expr);
        params( $( $p_name:ident => ($p_slot_type:ty, $p_default_param:expr, $p_config_param:expr) ),* $(,)? );
        storage( $( $s_name:ident => ($s_slot_type:ty, $s_initial_value:expr) ),* $(,)? );
    ) => {
        pub const PARAMS: usize = 0 $(+ { let _ = stringify!($p_name); 1 })*;

        pub static CONFIG: config::Config<PARAMS> = {
            #[allow(unused_mut)]
            let mut cfg = config::Config::new($app_name, $app_desc);
            $( cfg = cfg.add_param($p_config_param); )*
            cfg
        };

        pub fn default_params() -> [config::Value; PARAMS] {
            [ $( ($p_default_param).into() ),* ]
        }

        pub fn get_storage(app_id: u8, start_channel: usize) -> AppStorage {
            AppStorage::new(app_id, start_channel)
        }

        pub struct AppParams<'a> {
            $(
                #[allow(dead_code)]
                pub $p_name: $crate::storage::ParamSlot<'a, $p_slot_type, PARAMS>,
            )*
            values: &'a $crate::storage::ParamStore<PARAMS>,
        }

        impl<'a> AppParams<'a> {
            #[allow(unused)]
            pub fn new(values: &'a $crate::storage::ParamStore<PARAMS>) -> Self {
                let mut idx = 0;
                Self {
                    $(
                        $p_name: {
                            let current_idx = idx;
                            idx += 1;
                            $crate::storage::ParamSlot::<$p_slot_type, PARAMS>::new(values, current_idx)
                        },
                    )*
                    values,
                }
            }
        }

        pub struct AppStorage {
            $(
                #[allow(dead_code)]
                pub $s_name: $crate::storage::StorageSlot<$s_slot_type>,
            )*
        }

        impl AppStorage {
            #[allow(unused)]
            pub fn new(app_id: u8, start_channel: usize) -> Self {
                let mut idx = 0;
                Self {
                    $(
                        $s_name: {
                            let current_idx = idx;
                            idx += 1;
                            $crate::storage::StorageSlot::<$s_slot_type>::new($s_initial_value, app_id, start_channel as u8, current_idx)
                        },
                    )*
                }
            }
        }

        pub struct AppContext<'a> {
            #[allow(dead_code)]
            params: AppParams<'a>,
            #[allow(dead_code)]
            storage: AppStorage,
        }

        impl<'a> AppContext<'a> {
            pub fn new(param_values: &'a $crate::storage::ParamStore<PARAMS>, storage: AppStorage) -> Self {
                let params = AppParams::new(param_values);
                Self {
                    params,
                    storage,
                }
            }
        }

        pub async fn msg_loop(app_start_channel: usize, ctx: &AppContext<'_>, scene_signal: &embassy_sync::signal::Signal<embassy_sync::blocking_mutex::raw::NoopRawMutex, u8>) {
            let param_sender = $crate::storage::APP_CONFIGURE_EVENT.sender();
            let mut app_storage_receiver = $crate::storage::APP_STORAGE_CMD_PUBSUB.subscriber().unwrap();
            loop {
                match app_storage_receiver.next_message_pure().await {
                    $crate::storage::AppStorageCmd::GetAllParams { start_channel } => {
                        if app_start_channel != start_channel as usize {
                            continue;
                        }
                        let values: [config::Value; PARAMS] = ctx.params.values.get_all().await;
                        let vec = heapless::Vec::<_, { $crate::storage::APP_MAX_PARAMS }>::from_slice(&values)
                            .expect("Failed to create Vec from param values");
                        param_sender.send(vec).await;
                    }
                    $crate::storage::AppStorageCmd::SetParamSlot{ start_channel, param_slot, value } => {
                        if app_start_channel != start_channel as usize {
                            continue;
                        }
                        if (param_slot as usize) < PARAMS {
                            ctx.params.values.set(param_slot as usize, value).await;
                        }
                    }
                    #[allow(unused)]
                    $crate::storage::AppStorageCmd::SaveScene { scene } => {
                        $( ctx.storage.$s_name.save_to_scene(scene as u8).await; )*
                    }
                    #[allow(unused)]
                    $crate::storage::AppStorageCmd::LoadScene => {
                        let scene = $crate::scene::get_scene();
                        $( ctx.storage.$s_name.load_from_scene(scene).await; )*
                        scene_signal.signal(scene);
                    }
                    #[allow(unused)]
                    $crate::storage::AppStorageCmd::ReadAppStorageSlot { key, data } => {
                        let key_raw: u16 = key.into();
                        // TODO: Use match for the storage key to prevent cloning
                        $( ctx.storage.$s_name.load(key, data.clone()).await; )*
                    }
                }
            }
        }
    };
}
