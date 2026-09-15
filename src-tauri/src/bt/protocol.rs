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
}

pub struct Protocol {
    sequence: u8,
}

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
