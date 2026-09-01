use core::cell::RefCell;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use libfp::{CombineMode, RouteDestination, RouteSource, RoutingConfig};
use portable_atomic::{AtomicBool, AtomicU16, Ordering};

use crate::tasks::max::MAX_VALUES_ADC;

pub static VIRTUAL_APP_OUTPUTS: [AtomicU16; 16] = [const { AtomicU16::new(0) }; 16];
pub static ROUTING_CONFIG: Mutex<CriticalSectionRawMutex, RefCell<RoutingConfig>> =
    Mutex::new(RefCell::new(RoutingConfig::new()));
pub static IS_ROUTING_ACTIVE: AtomicBool = AtomicBool::new(false);

fn notify_modulated_destinations(source_channel: usize) {
    let src_chan = source_channel as u8;
    ROUTING_CONFIG.lock(|cell| {
        let config = cell.borrow();
        for r in config.routes.iter().flatten() {
            if r.enabled
                && matches!(r.source, RouteSource::AppOutput { channel } if channel == src_chan)
            {
                match r.destination {
                    RouteDestination::AppFader { channel }
                    | RouteDestination::AppInput { channel } => {
                        let target_chan = channel as usize;
                        if !crate::tasks::buttons::is_shift_button_pressed()
                            && !crate::tasks::buttons::is_channel_button_pressed(target_chan)
                        {
                            crate::events::EVENT_PUBSUB
                                .immediate_publisher()
                                .publish_immediate(crate::events::InputEvent::FaderChange(
                                    target_chan,
                                ));
                        }
                    }
                    _ => {}
                }
            }
        }
    });
}

pub fn set_virtual_output(channel: usize, value: u16) {
    if channel < 16 {
        let old = VIRTUAL_APP_OUTPUTS[channel].swap(value, Ordering::Relaxed);
        if old != value && IS_ROUTING_ACTIVE.load(Ordering::Relaxed) {
            notify_modulated_destinations(channel);
        }
    }
}

pub fn get_virtual_output(channel: usize) -> u16 {
    if channel < 16 {
        VIRTUAL_APP_OUTPUTS[channel].load(Ordering::Relaxed)
    } else {
        0
    }
}

/// Evaluates a raw source value in counts (0..4095)
fn evaluate_source_value(source: RouteSource) -> u16 {
    match source {
        RouteSource::AppOutput { channel } | RouteSource::AppMidi { channel } => {
            get_virtual_output(channel as usize)
        }
        RouteSource::PhysicalAdc { channel } => {
            if (channel as usize) < MAX_VALUES_ADC.len() {
                MAX_VALUES_ADC[channel as usize].load(Ordering::Relaxed)
            } else {
                0
            }
        }
        RouteSource::Constant { value } => value.clamp(0, 4095),
    }
}

/// Applies gain attenuation percent (-200% to +200%) and offset to a raw sample
fn apply_attenuation_and_offset(raw: u16, attenuation_percent: i16, offset: i16) -> u16 {
    let scaled = (raw as i32 * attenuation_percent as i32) / 100 + offset as i32;
    scaled.clamp(0, 4095) as u16
}

/// Check if any active route is targeted to an input channel (synchronous)
pub fn is_input_routed_sync(channel: usize) -> bool {
    if !IS_ROUTING_ACTIVE.load(Ordering::Relaxed) {
        return false;
    }
    let target_chan = channel as u8;
    ROUTING_CONFIG.lock(|cell| {
        let config = cell.borrow();
        config.routes.iter().flatten().any(|r| {
            r.enabled
                && matches!(
                    r.destination,
                    RouteDestination::AppInput { channel } if channel == target_chan
                )
        })
    })
}

/// Returns the effective input value for `channel` (synchronous).
/// If no internal routes target this input, returns `physical_adc`.
pub fn get_input_value_sync(channel: usize, physical_adc: u16) -> u16 {
    if !IS_ROUTING_ACTIVE.load(Ordering::Relaxed) {
        return physical_adc;
    }

    let target_chan = channel as u8;
    ROUTING_CONFIG.lock(|cell| {
        let config = cell.borrow();
        let matching_routes: heapless::Vec<_, 32> = config
            .routes
            .iter()
            .flatten()
            .filter(|r| {
                r.enabled
                    && matches!(
                        r.destination,
                        RouteDestination::AppInput { channel } if channel == target_chan
                    )
            })
            .collect();

        if matching_routes.is_empty() {
            physical_adc
        } else {
            evaluate_matching_routes(&matching_routes, physical_adc)
        }
    })
}

/// Returns the effective DAC output value for `channel` (synchronous).
/// If no internal routes target this DAC, returns `default_dac`.
pub fn get_dac_value_sync(channel: usize, default_dac: u16) -> u16 {
    if !IS_ROUTING_ACTIVE.load(Ordering::Relaxed) {
        return default_dac;
    }

    let target_chan = channel as u8;
    ROUTING_CONFIG.lock(|cell| {
        let config = cell.borrow();
        let matching_routes: heapless::Vec<_, 32> = config
            .routes
            .iter()
            .flatten()
            .filter(|r| {
                r.enabled
                    && matches!(
                        r.destination,
                        RouteDestination::PhysicalDac { channel } if channel == target_chan
                    )
            })
            .collect();

        if matching_routes.is_empty() {
            default_dac
        } else {
            evaluate_matching_routes(&matching_routes, default_dac)
        }
    })
}

/// Returns the CV fader modulation offset for `channel` (-2048..2047) (synchronous).
pub fn get_fader_offset_sync(channel: usize) -> i16 {
    if !IS_ROUTING_ACTIVE.load(Ordering::Relaxed) {
        return 0;
    }

    if crate::tasks::buttons::is_shift_button_pressed()
        || crate::tasks::buttons::is_channel_button_pressed(channel)
    {
        return 0;
    }

    let target_chan = channel as u8;
    ROUTING_CONFIG.lock(|cell| {
        let config = cell.borrow();
        let matching_routes: heapless::Vec<_, 32> = config
            .routes
            .iter()
            .flatten()
            .filter(|r| {
                r.enabled
                    && matches!(
                        r.destination,
                        RouteDestination::AppFader { channel } if channel == target_chan
                    )
            })
            .collect();

        if matching_routes.is_empty() {
            0
        } else {
            let val = evaluate_matching_routes(&matching_routes, 2048);
            (val as i32 - 2048).clamp(-2048, 2047) as i16
        }
    })
}

/// Check if any active route is targeted to an input channel
#[allow(dead_code)]
pub async fn is_input_routed(channel: usize) -> bool {
    is_input_routed_sync(channel)
}

/// Returns the effective input value for `channel`.
/// If no internal routes target this input, returns `physical_adc`.
#[allow(dead_code)]
pub async fn get_input_value(channel: usize, physical_adc: u16) -> u16 {
    get_input_value_sync(channel, physical_adc)
}

/// Returns the effective DAC output value for `channel`.
/// If no internal routes target this DAC, returns `default_dac`.
#[allow(dead_code)]
pub async fn get_dac_value(channel: usize, default_dac: u16) -> u16 {
    get_dac_value_sync(channel, default_dac)
}

/// Returns the CV fader modulation offset for `channel` (-2048..2047)
#[allow(dead_code)]
pub async fn get_fader_offset(channel: usize) -> i16 {
    get_fader_offset_sync(channel)
}

/// Combines multiple active routes for a single target destination
fn evaluate_matching_routes(routes: &[&libfp::Route], default_val: u16) -> u16 {
    if routes.is_empty() {
        return default_val;
    }

    let mode = routes[0].mode;

    match mode {
        CombineMode::Replace => {
            let last_route = routes.last().unwrap();
            let src = evaluate_source_value(last_route.source);
            apply_attenuation_and_offset(src, last_route.attenuation_percent, last_route.offset)
        }
        CombineMode::Sum => {
            let mut sum: i32 = 0;
            for r in routes {
                let src = evaluate_source_value(r.source);
                let val = apply_attenuation_and_offset(src, r.attenuation_percent, r.offset);
                sum += val as i32;
            }
            sum.clamp(0, 4095) as u16
        }
        CombineMode::Average => {
            let mut sum: i32 = 0;
            for r in routes {
                let src = evaluate_source_value(r.source);
                let val = apply_attenuation_and_offset(src, r.attenuation_percent, r.offset);
                sum += val as i32;
            }
            (sum / routes.len() as i32).clamp(0, 4095) as u16
        }
        CombineMode::Max => {
            let mut max_val: u16 = 0;
            for r in routes {
                let src = evaluate_source_value(r.source);
                let val = apply_attenuation_and_offset(src, r.attenuation_percent, r.offset);
                if val > max_val {
                    max_val = val;
                }
            }
            max_val
        }
        CombineMode::Min => {
            let mut min_val: u16 = 4095;
            for r in routes {
                let src = evaluate_source_value(r.source);
                let val = apply_attenuation_and_offset(src, r.attenuation_percent, r.offset);
                if val < min_val {
                    min_val = val;
                }
            }
            min_val
        }
        CombineMode::Or => {
            let mut any_high = false;
            for r in routes {
                let src = evaluate_source_value(r.source);
                if src > 2000 {
                    any_high = true;
                    break;
                }
            }
            if any_high {
                4095
            } else {
                0
            }
        }
        CombineMode::And => {
            let mut all_high = true;
            for r in routes {
                let src = evaluate_source_value(r.source);
                if src <= 2000 {
                    all_high = false;
                    break;
                }
            }
            if all_high {
                4095
            } else {
                0
            }
        }
        CombineMode::Xor => {
            let mut high_count: usize = 0;
            for r in routes {
                let src = evaluate_source_value(r.source);
                if src > 2000 {
                    high_count += 1;
                }
            }
            if high_count % 2 == 1 {
                4095
            } else {
                0
            }
        }
    }
}

/// Updates the active routing configuration in memory
pub async fn set_routing_config(new_config: RoutingConfig) {
    ROUTING_CONFIG.lock(|cell| {
        let has_routes = new_config.routes.iter().flatten().any(|r| r.enabled);
        IS_ROUTING_ACTIVE.store(has_routes, Ordering::Relaxed);
        *cell.borrow_mut() = new_config;

        for r in new_config.routes.iter().flatten() {
            if r.enabled {
                match r.destination {
                    RouteDestination::AppFader { channel }
                    | RouteDestination::AppInput { channel } => {
                        crate::events::EVENT_PUBSUB
                            .immediate_publisher()
                            .publish_immediate(crate::events::InputEvent::FaderChange(
                                channel as usize,
                            ));
                    }
                    _ => {}
                }
            }
        }
    });
}

/// Gets a copy of the active routing configuration
pub async fn get_routing_config() -> RoutingConfig {
    ROUTING_CONFIG.lock(|cell| *cell.borrow())
}
