macro_rules! register_apps {
    ($($id:literal => $app_mod:ident),+ $(,)?) => {
        $(
            mod $app_mod;
        )*

        use embassy_sync::{
            blocking_mutex::raw::{NoopRawMutex},
            signal::Signal,
        };

        use config::{Layout, Param};
        use libfp::constants::GLOBAL_CHANNELS;
        use crate::{CMD_CHANNEL, EVENT_PUBSUB};
        use crate::app::App;
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

        pub const REGISTERED_APP_IDS: [usize; _APP_COUNT] = [$($id),*];

        // IDEA: Currently this doesn't need to be async
        pub async fn spawn_app_by_id(
            app_id: u8,
            start_channel: usize,
            spawner: Spawner,
            exit_signals: &'static [Signal<NoopRawMutex, bool>; 16]
        ) {
            match app_id {
                $(
                    $id => {
                        let app = App::<{ $app_mod::CHANNELS }>::new(
                            app_id,
                            start_channel,
                            CMD_CHANNEL.sender(),
                            &EVENT_PUBSUB
                        );

                        spawner.spawn($app_mod::wrapper(app, &exit_signals[start_channel])).unwrap();
                        // let param_values = Store::new($app_mod::default_params(), app_id, start_channel);
                        // let storage_values = Store::new($app_mod::initial_storage(), app_id, start_channel);
                        //
                        // // TODO: We might want to replace this with something better
                        // storage_values.load(None).await;
                        //
                        // let params = $app_mod::AppParams::new(&param_values);
                        // let storage = $app_mod::AppStorage::new(&storage_values);
                        // let context = $app_mod::AppContext::new(params, storage);
                        // let scene_signal: Signal<NoopRawMutex, u8> = Signal::new();
                        //
                        // let sender = CMD_CHANNEL.sender();
                        // let app = App::<{ $app_mod::CHANNELS }>::new(
                        //     app_id,
                        //     start_channel as usize,
                        //     sender,
                        //     &EVENT_PUBSUB,
                        // );
                        //
                        // join(
                        //     $app_mod::run(app, &context),
                        //     // TODO: We could just pass in the param_values and storage_values here
                        //     // (instead of context) so that we can pass the unborrowed context into
                        //     // the app start which makes for a nicer API
                        //     $app_mod::msg_loop(start_channel, &context, &scene_signal),
                        // ).await;
                    },
                )*
                _ => panic!("Unknown app ID: {}", app_id),
            }
        }

        pub fn get_layout_from_slice(slice: &[u8]) -> Layout {
            if slice.len() > GLOBAL_CHANNELS {
                panic!("Layout is too big");
            }
            let mut start_channel = 0;
            let mut layout: Layout = Layout::new();
            for &app_id in slice {
                let channels = get_channels(app_id);
                let last = start_channel + channels;
                if last > GLOBAL_CHANNELS {
                    break;
                }
                layout.push((app_id, start_channel, channels));
                start_channel += channels;
            }
            layout.set_last(start_channel);
            layout
        }

        fn get_channels(app_id: u8) -> usize {
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

// macro_rules! app_config {
//     (
//         config($app_name:expr, $app_desc:expr);
//         params( $( $p_name:ident => ($p_slot_type:ty, $p_default_param:expr, $p_config_param:expr) ),* $(,)? );
//         storage( $( $s_name:ident => ($s_slot_type:ty, $s_initial_value:expr) ),* $(,)? );
//     ) => {
//         pub const PARAMS: usize = 0 $(+ { let _ = stringify!($p_name); 1 })*;
//         pub const STORAGE: usize = 0 $(+ { let _ = stringify!($s_name); 1 })*;
//
//         pub static CONFIG: config::Config<PARAMS> = {
//             #[allow(unused_mut)]
//             let mut cfg = config::Config::new($app_name, $app_desc);
//             $( cfg = cfg.add_param($p_config_param); )*
//             cfg
//         };
//
//         pub fn default_params() -> [config::Value; PARAMS] {
//             [ $( ($p_default_param).into() ),* ]
//         }
//
//         pub fn initial_storage() -> [config::Value; STORAGE] {
//             [ $( ($s_initial_value).into() ),* ]
//         }
//
//         pub struct AppParams<'a> {
//             $(
//                 #[allow(dead_code)]
//                 pub $p_name: $crate::storage::ParamSlot<'a, $p_slot_type, PARAMS>,
//             )*
//             values: &'a $crate::storage::Store<PARAMS>,
//         }
//
//         impl<'a> AppParams<'a> {
//             #[allow(unused)]
//             pub fn new(values: &'a $crate::storage::Store<PARAMS>) -> Self {
//                 let mut idx = 0;
//                 Self {
//                     $(
//                         $p_name: {
//                             let current_idx = idx;
//                             idx += 1;
//                             $crate::storage::ParamSlot::<$p_slot_type, PARAMS>::new(values, current_idx)
//                         },
//                     )*
//                     values,
//                 }
//             }
//         }
//
//         pub struct AppStorage<'a> {
//             $(
//                 #[allow(dead_code)]
//                 pub $s_name: $crate::storage::StorageSlot<'a, $s_slot_type, STORAGE>,
//             )*
//         }
//
//         impl<'a> AppStorage<'a> {
//             #[allow(unused)]
//             pub fn new(values: &'a $crate::storage::Store<STORAGE>) -> Self {
//                 let mut idx = 0;
//                 Self {
//                     $(
//                         $s_name: {
//                             let current_idx = idx;
//                             idx += 1;
//                             $crate::storage::StorageSlot::<$s_slot_type, STORAGE>::new(values, current_idx)
//                         },
//                     )*
//                 }
//             }
//         }
//
//         pub struct AppContext<'a>{
//             #[allow(dead_code)]
//             params: AppParams<'a>,
//             #[allow(dead_code)]
//             storage: AppStorage<'a>,
//         }
//
//         impl<'a> AppContext<'a> {
//             pub fn new(params: AppParams<'a>, storage: AppStorage<'a>) -> Self {
//                 Self {
//                     params,
//                     storage,
//                 }
//             }
//         }
//
//         pub async fn msg_loop(app_start_channel: u8, ctx: &AppContext<'_>, scene_signal: &embassy_sync::signal::Signal<embassy_sync::blocking_mutex::raw::NoopRawMutex, u8>) {
//             let param_sender = $crate::storage::APP_CONFIGURE_EVENT.sender();
//             let mut app_storage_receiver = $crate::storage::APP_STORAGE_CMD_PUBSUB.subscriber().unwrap();
//             loop {
//                 match app_storage_receiver.next_message_pure().await {
//                     $crate::storage::AppStorageCmd::GetAllParams { start_channel } => {
//                         if app_start_channel != start_channel {
//                             continue;
//                         }
//                         let values: [config::Value; PARAMS] = ctx.params.values.get_all().await;
//                         let vec = heapless::Vec::<_, { $crate::storage::APP_MAX_PARAMS }>::from_slice(&values)
//                             .expect("Failed to create Vec from param values");
//                         param_sender.send(vec).await;
//                     }
//                     $crate::storage::AppStorageCmd::SetParamSlot{ start_channel, param_slot, value } => {
//                         if app_start_channel != start_channel {
//                             continue;
//                         }
//                         if (param_slot as usize) < PARAMS {
//                             ctx.params.values.set(param_slot as usize, value).await;
//                         }
//                     }
//                     #[allow(unused)]
//                     $crate::storage::AppStorageCmd::SaveScene { scene } => {
//                         $( ctx.storage.$s_name.save_to_scene(scene as u8).await; )*
//                     }
//                     #[allow(unused)]
//                     $crate::storage::AppStorageCmd::LoadScene => {
//                         let scene = $crate::scene::get_scene();
//                         $( ctx.storage.$s_name.load_from_scene(scene).await; )*
//                         scene_signal.signal(scene);
//                     }
//                 }
//             }
//         }
//     };
// }
