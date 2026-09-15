// Device profiles: new buds models plug in as data, not code.
// Shared protocol family: transport/framing/auth are identical; profiles declare
// the discovery name pattern and which features the model supports.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Features {
    pub anc: bool,
    pub transparency: bool,
    pub ear_detection: bool,
    pub equalizer: bool,
    pub dual_connection: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceProfile {
    pub model: &'static str,
    pub name_pattern: &'static str,
    pub features: Features,
}

pub const PROFILES: &[DeviceProfile] = &[
    DeviceProfile {
        model: "Redmi Buds 8 Active",
        name_pattern: "REDMI Buds 8 Active",
        features: Features {
            anc: true,
            transparency: true,
            ear_detection: true,
            equalizer: true,
            dual_connection: true,
        },
    },
    DeviceProfile {
        model: "Redmi Buds 8 Lite",
        name_pattern: "REDMI Buds 8 Lite",
        // Same generation as 8 Active; feature set to be confirmed on device.
        features: Features {
            anc: true,
            transparency: true,
            ear_detection: false,
            equalizer: true,
            dual_connection: false,
        },
    },
    DeviceProfile {
        model: "Redmi Buds 6 Active",
        name_pattern: "REDMI Buds 6 Active",
        features: Features {
            anc: true,
            transparency: true,
            ear_detection: true,
            equalizer: false,
            dual_connection: false,
        },
    },
    DeviceProfile {
        model: "Redmi Buds 6",
        name_pattern: "REDMI Buds 6",
        features: Features {
            anc: true,
            transparency: true,
            ear_detection: true,
            equalizer: false,
            dual_connection: false,
        },
    },
    DeviceProfile {
        model: "Redmi Buds 6 Play",
        name_pattern: "REDMI Buds 6 Play",
        features: Features {
            anc: false,
            transparency: false,
            ear_detection: false,
            equalizer: false,
            dual_connection: false,
        },
    },
    DeviceProfile {
        model: "Redmi Buds 5 Pro",
        name_pattern: "REDMI Buds 5 Pro",
        features: Features {
            anc: true,
            transparency: true,
            ear_detection: true,
            equalizer: true,
            dual_connection: true,
        },
    },
    DeviceProfile {
        model: "Redmi Buds 4 Active",
        name_pattern: "REDMI Buds 4 Active",
        features: Features {
            anc: true,
            transparency: true,
            ear_detection: false,
            equalizer: false,
            dual_connection: false,
        },
    },
    DeviceProfile {
        model: "Redmi Buds 3 Pro",
        name_pattern: "REDMI Buds 3 Pro",
        features: Features {
            anc: true,
            transparency: true,
            ear_detection: true,
            equalizer: false,
            dual_connection: false,
        },
    },
];

/// Match an advertised device name to a profile. Falls back to a generic profile
/// for unknown REDMI/Xiaomi buds so the app still works with new models.
pub fn match_profile(device_name: &str) -> Option<&'static DeviceProfile> {
    let name_lower = device_name.to_lowercase();
    // Longer patterns first so "REDMI Buds 6 Pro" doesn't match "REDMI Buds 6".
    let mut best: Option<&DeviceProfile> = None;
    for p in PROFILES {
        if name_lower.contains(&p.name_pattern.to_lowercase()) {
            if best.map_or(true, |b| p.name_pattern.len() > b.name_pattern.len()) {
                best = Some(p);
            }
        }
    }
    best
}

pub fn generic_profile() -> DeviceProfile {
    DeviceProfile {
        model: "Unknown Xiaomi Buds",
        name_pattern: "",
        features: Features {
            anc: true,
            transparency: true,
            ear_detection: false,
            equalizer: false,
            dual_connection: false,
        },
    }
}
