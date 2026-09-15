// High-level command encoders + response payload decoders.
// Mirrors Gadgetbridge RedmiBuds{,8Active}Protocol.

use super::auth;
use super::framing::{Message, MessageType, Opcode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AncMode {
    Off = 0x00,
    NoiseCancelling = 0x01,
    Transparency = 0x02,
}

impl AncMode {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(AncMode::Off),
            0x01 => Some(AncMode::NoiseCancelling),
            0x02 => Some(AncMode::Transparency),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct BatteryState {
    pub left: Option<u8>,
    pub right: Option<u8>,
    pub case: Option<u8>,
    pub left_charging: bool,
    pub right_charging: bool,
    pub case_charging: bool,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct DeviceState {
    pub battery: BatteryState,
    pub firmware: Option<String>,
    pub anc_mode: Option<u8>,
    pub wearing_detection: Option<bool>,
    pub eq_preset: Option<u8>,
    /// single L/R, double L/R, triple L/R, long L/R
    pub gestures: Option<[u8; 8]>,
    pub double_connection: Option<bool>,
    pub adaptive_sound: Option<bool>,
    pub auto_answer: Option<bool>,
    pub adaptive_anc: Option<bool>,
    pub customized_anc: Option<bool>,
    /// EFFECT_STRENGTH: (anc_target, mode) — target 1=ANC, 2=transparency
    pub effect_strength_anc: Option<u8>,
    pub effect_strength_transparency: Option<u8>,
}

pub struct Protocol {
    sequence: u8,
}

// EQ preset codes for the Buds 8 family (8 Active / 8 Lite).
pub const EQ_PRESET_BALANCED: u8 = 21;
pub const EQ_PRESET_TREBLE: u8 = 6;
pub const EQ_PRESET_BASS: u8 = 5;
pub const EQ_PRESET_VOICE: u8 = 1;
pub const EQ_PRESET_VOLUME: u8 = 7;
pub const EQ_PRESET_CUSTOM: u8 = 10;

// The 10 custom-EQ band frequencies in Hz (62 Hz .. 16 kHz).
pub const EQ_BAND_FREQS: [u32; 10] = [62, 125, 250, 500, 1000, 2000, 4000, 8000, 12000, 16000];

impl Protocol {
    pub fn new() -> Self {
        Protocol { sequence: 0 }
    }

    fn next_seq(&mut self) -> u8 {
        let s = self.sequence;
        self.sequence = self.sequence.wrapping_add(1);
        s
    }

    /// Auth step 1: phone sends 0x01 + 16 random bytes.
    pub fn encode_start_authentication(&mut self) -> Message {
        let mut rnd = [0u8; 16];
        getrandom(&mut rnd);
        let mut payload = vec![0x01];
        payload.extend_from_slice(&rnd);
        Message::new(
            MessageType::PhoneRequest,
            Opcode::AuthChallenge,
            self.next_seq(),
            payload,
        )
    }

    /// Auth step 2: respond to the buds' challenge with the SAFER+ answer.
    pub fn encode_challenge_response(&mut self, challenge: &[u8; 16], seq: u8) -> Message {
        let response = auth::compute_challenge_response(challenge);
        let mut payload = vec![0x01];
        payload.extend_from_slice(&response);
        Message::new(MessageType::Response, Opcode::AuthChallenge, seq, payload)
    }

    /// Auth step 3: confirm.
    pub fn encode_auth_confirm(&mut self) -> Message {
        Message::new(
            MessageType::PhoneRequest,
            Opcode::AuthConfirm,
            self.next_seq(),
            vec![0x01, 0x00],
        )
    }

    pub fn encode_auth_confirm_response(&mut self, seq: u8) -> Message {
        Message::new(MessageType::Response, Opcode::AuthConfirm, seq, vec![0x01])
    }

    pub fn encode_get_device_info(&mut self) -> Message {
        Message::new(
            MessageType::PhoneRequest,
            Opcode::GetDeviceInfo,
            self.next_seq(),
            vec![0xFF, 0xFF, 0xFF, 0xFF],
        )
    }

    pub fn encode_get_device_run_info(&mut self) -> Message {
        Message::new(
            MessageType::PhoneRequest,
            Opcode::GetDeviceRunInfo,
            self.next_seq(),
            vec![0xFF, 0xFF, 0xFF, 0xFF],
        )
    }

    pub fn encode_set_anc(&mut self, mode: AncMode) -> Message {
        Message::new(
            MessageType::PhoneRequest,
            Opcode::Anc,
            self.next_seq(),
            vec![0x02, 0x04, mode as u8],
        )
    }

    /// SET_CONFIG for a single integer value (EQ preset etc.):
    /// {len_lo, len_hi, config_id, value}.
    pub fn encode_set_integer_config(&mut self, config_id: u8, value: u8) -> Message {
        Message::new(
            MessageType::PhoneRequest,
            Opcode::SetConfig,
            self.next_seq(),
            vec![0x03, 0x00, config_id, value],
        )
    }

    /// GET_CONFIG for a single config id: {0x00, config_id}.
    pub fn encode_get_config(&mut self, config_id: u8) -> Message {
        Message::new(
            MessageType::PhoneRequest,
            Opcode::GetConfig,
            self.next_seq(),
            vec![0x00, config_id],
        )
    }

    pub fn encode_eq_preset(&mut self, preset: u8) -> Message {
        self.encode_set_integer_config(0x07, preset) // EQ_PRESET
    }

    // ---- Gestures -------------------------------------------------------
    // SET_CONFIG {len, 0x02, interaction, left, right}; 0xFF = untouched side.
    // Interaction type bytes (Gadgetbridge Gestures.InteractionType).
    pub const GESTURE_SINGLE: u8 = 0x04;
    pub const GESTURE_DOUBLE: u8 = 0x01;
    pub const GESTURE_TRIPLE: u8 = 0x02;
    pub const GESTURE_LONG: u8 = 0x03;

    // Action values for the Buds 8 family (single/double/triple taps):
    pub const GESTURE_NONE: u8 = 8;
    pub const GESTURE_PLAY_PAUSE: u8 = 1;
    pub const GESTURE_PREV: u8 = 2;
    pub const GESTURE_NEXT: u8 = 3;
    pub const GESTURE_VOL_UP: u8 = 4;
    pub const GESTURE_VOL_DOWN: u8 = 5;
    // Long-press adds: Voice assistant = 0, Noise-control switch = 6
    // (observed as the factory default on Buds 8 Lite).
    pub const GESTURE_VOICE_ASSISTANT: u8 = 0;
    pub const GESTURE_ANC_SWITCH: u8 = 6;

    pub fn encode_set_gesture(&mut self, interaction: u8, left: bool, value: u8) -> Message {
        let (l, r) = if left {
            (value, 0xFF)
        } else {
            (0xFF, value)
        };
        Message::new(
            MessageType::PhoneRequest,
            Opcode::SetConfig,
            self.next_seq(),
            vec![0x05, 0x00, 0x02, interaction, l, r],
        )
    }

    /// SET_CONFIG {04 00 0b target mode} — effect strength (1=ANC, 2=transparency).
    pub fn encode_set_strength(&mut self, target: u8, mode: u8) -> Message {
        Message::new(
            MessageType::PhoneRequest,
            Opcode::SetConfig,
            self.next_seq(),
            vec![0x04, 0x00, 0x0b, target, mode],
        )
    }

    /// SET_CONFIG custom 10-band EQ curve (EQ_CURVE = 0x37).
    /// Each band: 3-byte big-endian frequency prefix + gain byte.
    pub fn encode_eq_curve(&mut self, bands: &[i8; 10]) -> Message {
        let freq_prefixes: [[u8; 2]; 10] = [
            [0x00, 0x3E], // 62
            [0x00, 0x7D], // 125
            [0x00, 0xFA], // 250
            [0x01, 0xF4], // 500
            [0x03, 0xE8], // 1000
            [0x07, 0xE0], // 2000
            [0x0F, 0xA0], // 4000
            [0x1F, 0x40], // 8000
            [0x2E, 0xE0], // 12000
            [0x3E, 0x80], // 16000
        ];
        let mut payload = vec![0x24, 0x00, 0x37, 0x05, 0x01, 0x01, 0x0A];
        for (i, &gain) in bands.iter().enumerate() {
            payload.extend_from_slice(&freq_prefixes[i]);
            payload.push(gain as u8);
        }
        Message::new(
            MessageType::PhoneRequest,
            Opcode::SetConfig,
            self.next_seq(),
            payload,
        )
    }

    pub fn encode_status_ack(&mut self, seq: u8) -> Message {
        Message::new(MessageType::Response, Opcode::ReportStatus, seq, vec![])
    }

    pub fn encode_notify_config_ack(&mut self, seq: u8) -> Message {
        Message::new(MessageType::Response, Opcode::NotifyConfig, seq, vec![])
    }

    /// Decode a GET_DEVICE_INFO payload (TLV stream: [len][index][data..]).
    pub fn decode_device_info(payload: &[u8], state: &mut DeviceState) {
        let mut i = 0usize;
        let mut fw = [0u8; 4];
        while i + 1 < payload.len() {
            let len = payload[i] as usize;
            let index = payload[i + 1];
            match index {
                0x01 if i + 2 + 4 <= payload.len() => {
                    fw.copy_from_slice(&payload[i + 2..i + 6]);
                    state.firmware = Some(format!(
                        "{}.{}.{}.{}",
                        (fw[0] >> 4) & 0xF,
                        fw[0] & 0xF,
                        (fw[1] >> 4) & 0xF,
                        fw[1] & 0xF
                    ));
                }
                0x07 if i + 2 + 3 <= payload.len() => {
                    parse_battery(&payload[i + 2..i + 5], state);
                }
                _ => {}
            }
            i += len + 1;
        }
    }

    /// Decode a REPORT_STATUS payload (TLV stream), returns true if it carried data.
    pub fn decode_device_update(payload: &[u8], state: &mut DeviceState) -> bool {
        let mut i = 0usize;
        let mut got = false;
        while i + 1 < payload.len() {
            let len = payload[i] as usize;
            let index = payload[i + 1];
            match index {
                0x00 if i + 2 + 3 <= payload.len() => {
                    parse_battery(&payload[i + 2..i + 5], state);
                    got = true;
                }
                0x04 if i + 2 < payload.len() => {
                    state.anc_mode = Some(payload[i + 2]);
                    got = true;
                }
                _ => {}
            }
            i += len + 1;
        }
        got
    }

    /// Decode a GET_DEVICE_RUN_INFO payload (current ANC mode etc.).
    pub fn decode_run_info(payload: &[u8], state: &mut DeviceState) {
        let mut i = 0usize;
        while i + 1 < payload.len() {
            let len = payload[i] as usize;
            let index = payload[i + 1];
            match index {
                0x09 if i + 2 < payload.len() => state.anc_mode = Some(payload[i + 2]),
                0x0A if i + 2 < payload.len() => {
                    state.wearing_detection = Some(payload[i + 2] == 0x00)
                }
                _ => {}
            }
            i += len + 1;
        }
    }

    /// Decode a GET_CONFIG response payload ({?, ?, config_id, values...}).
    pub fn decode_config(payload: &[u8], state: &mut DeviceState) {
        if payload.len() < 4 {
            return;
        }
        match payload[2] {
            0x07 => state.eq_preset = Some(payload[3]), // EQ_PRESET
            0x04 => state.double_connection = Some(payload[3] == 1),
            0x29 => state.adaptive_sound = Some(payload[3] == 1),
            0x03 => state.auto_answer = Some(payload[3] == 1),
            0x25 => state.adaptive_anc = Some(payload[3] == 1),
            0x3b => state.customized_anc = Some(payload[3] == 1),
            0x0b => {
                // {len,00,0b,target,mode}
                if payload.len() >= 5 {
                    match payload[3] {
                        1 => state.effect_strength_anc = Some(payload[4]),
                        2 => state.effect_strength_transparency = Some(payload[4]),
                        _ => {}
                    }
                }
            }
            0x02 => {
                // GESTURES: groups of (interaction_type, left, right) starting
                // at index 3. Type bytes: 04=single, 01=double, 02=triple,
                // 03=long. Order observed on Buds 8 Lite: 01, 02, 03, 04.
                let mut single = [0u8; 2];
                let mut double = [0u8; 2];
                let mut triple = [0u8; 2];
                let mut long = [0u8; 2];
                let mut g = [0u8; 8];
                let mut i = 3;
                while i + 2 < payload.len() {
                    let (l, r) = (payload[i + 1], payload[i + 2]);
                    match payload[i] {
                        0x04 => single = [l, r],
                        0x01 => double = [l, r],
                        0x02 => triple = [l, r],
                        0x03 => long = [l, r],
                        _ => {}
                    }
                    i += 3;
                }
                g[0..2].copy_from_slice(&single);
                g[2..4].copy_from_slice(&double);
                g[4..6].copy_from_slice(&triple);
                g[6..8].copy_from_slice(&long);
                state.gestures = Some(g);
            }
            _ => {}
        }
    }

    /// Decode a NOTIFY_CONFIG payload (async ANC/strength notify).
    pub fn decode_notify_config(payload: &[u8], state: &mut DeviceState) {
        let mut i = 0usize;
        while i + 1 < payload.len() {
            let len = payload[i] as usize;
            let index = payload[i + 2];
            match index {
                0x0B if i + 4 < payload.len() => state.anc_mode = Some(payload[i + 3]),
                _ => {}
            }
            i += len + 1;
        }
    }
}

fn parse_battery(data: &[u8], state: &mut DeviceState) {
    // Order: left, right, case. 0xFF = unknown; bit7 = charging.
    let parse = |v: u8| -> (Option<u8>, bool) {
        if v == 0xFF {
            (None, false)
        } else {
            (Some(v & 0x7F), v & 0x80 != 0)
        }
    };
    (state.battery.left, state.battery.left_charging) = parse(data[0]);
    (state.battery.right, state.battery.right_charging) = parse(data[1]);
    (state.battery.case, state.battery.case_charging) = parse(data[2]);
}

fn getrandom(buf: &mut [u8]) {
    use rand::RngCore;
    rand::thread_rng().fill_bytes(buf);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_parsing() {
        let mut st = DeviceState::default();
        parse_battery(&[0x64, 0xFF, 0x80 | 0x32], &mut st);
        assert_eq!(st.battery.left, Some(100));
        assert_eq!(st.battery.right, None);
        assert_eq!(st.battery.case, Some(50));
        assert!(st.battery.case_charging);
    }

    #[test]
    fn device_info_tlv() {
        let mut st = DeviceState::default();
        // len=5, index=1, 4 fw bytes | len=4, index=7, 3 battery bytes
        let payload = [5u8, 0x01, 0x30, 0x06, 0x00, 0x00, 4, 0x07, 0x64, 0x64, 0xFF];
        Protocol::decode_device_info(&payload, &mut st);
        assert_eq!(st.firmware.as_deref(), Some("3.0.0.6"));
        assert_eq!(st.battery.left, Some(100));
        assert_eq!(st.battery.right, Some(100));
    }
}
