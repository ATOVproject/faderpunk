use postcard_bindgen::PostcardBindings;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, PostcardBindings)]
pub enum SignalType {
    #[default]
    Cv,
    Gate,
    Midi,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, PostcardBindings)]
pub enum CombineMode {
    /// Saturation sum: clamp(A + B, 0, 4095)
    #[default]
    Sum,
    /// Average: (A + B) / 2
    Average,
    /// Highest voltage/value wins
    Max,
    /// Lowest voltage/value wins
    Min,
    /// Gate logic OR (High if any active)
    Or,
    /// Gate logic AND (High if all active)
    And,
    /// Gate logic XOR (High if odd count active)
    Xor,
    /// Direct override / replace
    Replace,
}

impl crate::ext::FromValue for CombineMode {
    fn from_value(val: crate::Value) -> Self {
        match val {
            crate::Value::Enum(idx) => match idx {
                0 => CombineMode::Sum,
                1 => CombineMode::Average,
                2 => CombineMode::Max,
                3 => CombineMode::Min,
                4 => CombineMode::Or,
                5 => CombineMode::And,
                6 => CombineMode::Xor,
                7 => CombineMode::Replace,
                _ => CombineMode::Sum,
            },
            _ => CombineMode::Sum,
        }
    }
}

impl From<CombineMode> for crate::Value {
    fn from(val: CombineMode) -> Self {
        crate::Value::Enum(val as usize)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, PostcardBindings)]
pub enum RouteSource {
    AppOutput { channel: u8 },
    AppMidi { channel: u8 },
    PhysicalAdc { channel: u8 },
    Constant { value: u16 },
}

impl Default for RouteSource {
    fn default() -> Self {
        Self::AppOutput { channel: 0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, PostcardBindings)]
pub enum RouteDestination {
    PhysicalDac { channel: u8 },
    AppInput { channel: u8 },
    AppFader { channel: u8 },
    AppLayer { channel: u8 },
}

impl Default for RouteDestination {
    fn default() -> Self {
        Self::PhysicalDac { channel: 0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, PostcardBindings)]
pub struct Route {
    pub source: RouteSource,
    pub destination: RouteDestination,
    pub mode: CombineMode,
    /// Gain multiplier percentage (-200% to +200%)
    pub attenuation_percent: i16,
    /// Constant value offset (-2048 to +2047)
    pub offset: i16,
    pub enabled: bool,
}

pub const MAX_ROUTES: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, PostcardBindings)]
pub struct RoutingConfig {
    pub routes: [Option<Route>; MAX_ROUTES],
}

impl RoutingConfig {
    pub const fn new() -> Self {
        Self {
            routes: [None; MAX_ROUTES],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcard::{from_bytes, to_slice};

    #[test]
    fn test_routing_config_postcard_roundtrip() {
        let mut config = RoutingConfig::new();
        config.routes[0] = Some(Route {
            source: RouteSource::AppOutput { channel: 1 },
            destination: RouteDestination::PhysicalDac { channel: 3 },
            mode: CombineMode::Sum,
            attenuation_percent: 100,
            offset: 0,
            enabled: true,
        });

        config.routes[1] = Some(Route {
            source: RouteSource::PhysicalAdc { channel: 0 },
            destination: RouteDestination::AppInput { channel: 2 },
            mode: CombineMode::Or,
            attenuation_percent: 50,
            offset: -100,
            enabled: false,
        });

        let mut buf = [0_u8; 512];
        let bytes = to_slice(&config, &mut buf).unwrap();
        let deserialized: RoutingConfig = from_bytes(bytes).unwrap();
        assert_eq!(config, deserialized);
    }
}
