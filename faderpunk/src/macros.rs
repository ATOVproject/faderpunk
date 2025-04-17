macro_rules! register_apps {
    ($($id:literal => $app_mod:ident),+ $(,)?) => {
        $(
            mod $app_mod;
        )*


        use embassy_futures::join::join;

        use config::Param;
        use crate::{CMD_CHANNEL, EVENT_PUBSUB};
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

        pub async fn run_app_by_id(
            app_id: usize,
            start_channel: usize,
        ) {
            match app_id {
                $(
                    $id => {
                        let sender = CMD_CHANNEL.sender();
                        let app = App::<{ $app_mod::CHANNELS }>::new(
                            app_id,
                            start_channel,
                            sender,
                            &EVENT_PUBSUB
                        );
                        let param_values = ParamStore::new($app_mod::default_params(), app_id, start_channel);
                        let storage = $app_mod::get_storage(app_id, start_channel);
                        let context = $app_mod::AppContext::new(&param_values, storage);
                        join($app_mod::run(app, &context), $app_mod::msg_loop(start_channel, &context)).await;
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

        pub fn get_storage(app_id: usize, start_channel: usize) -> AppStorage {
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
            pub fn new(app_id: usize, start_channel: usize) -> Self {
                let mut idx = 0;
                Self {
                    $(
                        $s_name: {
                            let current_idx = idx;
                            idx += 1;
                            $crate::storage::StorageSlot::<$s_slot_type>::new($s_initial_value, app_id, start_channel, idx)
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

        pub async fn msg_loop(start_channel: usize, ctx: &AppContext<'_>) {
             let param_sender = $crate::APP_STORAGE_EVENT.sender();
             loop {
                 match $crate::APP_STORAGE_CMDS[start_channel].wait().await {
                     $crate::AppStorageCmd::GetAllParams => {
                         let values: [config::Value; PARAMS] = ctx.params.values.get_all().await;
                         let vec = heapless::Vec::<_, { $crate::storage::APP_MAX_PARAMS }>::from_slice(&values)
                             .expect("Failed to create Vec from param values");
                         param_sender.send(vec).await;
                     }
                     $crate::AppStorageCmd::SetParamSlot(slot, value) => {
                         if slot < PARAMS {
                             ctx.params.values.set(slot, value).await;
                         }
                     }
                     $crate::AppStorageCmd::SaveScene => {
                        #[allow(unused)]
                        let scene = $crate::scene::get_global_scene();
                        $( ctx.storage.$s_name.save_to_scene(scene).await; )*
                     }
                 }
             }
        }
    };
}
