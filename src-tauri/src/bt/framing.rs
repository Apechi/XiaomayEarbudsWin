// Wire framing for the RedmiBuds control protocol.
// Header FE DC BA, type, opcode, payload_len (BE, u16), [00 if response], seq, payload, trailer EF.
// See research/PROTOCOL.md.

pub const MESSAGE_HEADER: [u8; 3] = [0xFE, 0xDC, 0xBA];
pub const MESSAGE_TRAILER: u8 = 0xEF;

pub const TYPE_PHONE_REQUEST: u8 = 0xC4;
pub const TYPE_RESPONSE: u8 = 0x04;
pub const TYPE_EARBUDS_REQUEST: u8 = 0xC0;
pub const TYPE_EARBUDS_NOTIFY: u8 = 0xC7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    PhoneRequest,
    Response,
    EarbudsRequest,
    EarbudsNotify,
    Unknown(u8),
}

impl MessageType {
    pub fn code(self) -> u8 {
        match self {
            MessageType::PhoneRequest => TYPE_PHONE_REQUEST,
            MessageType::Response => TYPE_RESPONSE,
            MessageType::EarbudsRequest => TYPE_EARBUDS_REQUEST,
            MessageType::EarbudsNotify => TYPE_EARBUDS_NOTIFY,
            MessageType::Unknown(c) => c,
        }
    }

    pub fn from_code(code: u8) -> Self {
        match code {
            TYPE_PHONE_REQUEST => MessageType::PhoneRequest,
            TYPE_RESPONSE => MessageType::Response,
            TYPE_EARBUDS_REQUEST => MessageType::EarbudsRequest,
            TYPE_EARBUDS_NOTIFY => MessageType::EarbudsNotify,
            c => MessageType::Unknown(c),
        }
    }

    /// Request types carry the 0x40 bit and have a shorter (no 0x00 pad) header.
    pub fn is_request(self) -> bool {
        self.code() & 0x40 != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    GetDeviceInfo = 0x02,
    Anc = 0x08,
    GetDeviceRunInfo = 0x09,
    ReportStatus = 0x0E,
    AuthChallenge = 0x50,
    AuthConfirm = 0x51,
    SetConfig = 0xF2,
    GetConfig = 0xF3,
    NotifyConfig = 0xF4,
    Unknown = 0xFF,
}

impl Opcode {
    pub fn from_code(code: u8) -> Self {
        match code {
            0x02 => Opcode::GetDeviceInfo,
            0x08 => Opcode::Anc,
            0x09 => Opcode::GetDeviceRunInfo,
            0x0E => Opcode::ReportStatus,
            0x50 => Opcode::AuthChallenge,
            0x51 => Opcode::AuthConfirm,
            0xF2 => Opcode::SetConfig,
            0xF3 => Opcode::GetConfig,
            0xF4 => Opcode::NotifyConfig,
            _ => Opcode::Unknown,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub msg_type: MessageType,
    pub opcode: Opcode,
    pub opcode_raw: u8,
    pub sequence: u8,
    pub payload: Vec<u8>,
}

impl Message {
    pub fn new(msg_type: MessageType, opcode: Opcode, sequence: u8, payload: Vec<u8>) -> Self {
        Message {
            msg_type,
            opcode,
            opcode_raw: opcode as u8,
            sequence,
            payload,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let size_add: usize = if self.msg_type.is_request() { 1 } else { 2 };
        let payload_len = self.payload.len() + size_add;

        let mut buf = Vec::with_capacity(payload_len + 8);
        buf.extend_from_slice(&MESSAGE_HEADER);
        buf.push(self.msg_type.code());
        buf.push(self.opcode_raw);
        buf.push((payload_len >> 8) as u8);
        buf.push((payload_len & 0xFF) as u8);
        if !self.msg_type.is_request() {
            buf.push(0x00);
        }
        buf.push(self.sequence);
        buf.extend_from_slice(&self.payload);
        buf.push(MESSAGE_TRAILER);
        buf
    }

    /// Parse a single framed message from raw bytes (must include header + trailer).
    pub fn from_bytes(bytes: &[u8]) -> Option<Message> {
        if bytes.len() < 7 || bytes[..3] != MESSAGE_HEADER {
            return None;
        }
        let msg_type = MessageType::from_code(bytes[3]);
        let opcode_raw = bytes[4];
        let payload_offset = 3 + if msg_type.is_request() { 5 } else { 6 };
        if bytes.len() < payload_offset + 1 {
            return None;
        }
        let sequence = bytes[payload_offset - 1];
        let payload = bytes[payload_offset..bytes.len() - 1].to_vec();
        Some(Message {
            msg_type,
            opcode: Opcode::from_code(opcode_raw),
            opcode_raw,
            sequence,
            payload,
        })
    }

    /// One notification can piggyback several framed messages; split on headers.
    pub fn split_piggybacked(input: &[u8]) -> Vec<Message> {
        let mut starts: Vec<usize> = Vec::new();
        if input.len() > 3 {
            for i in 0..=(input.len() - 3) {
                if input[i..i + 3] == MESSAGE_HEADER {
                    starts.push(i);
                }
            }
        }
        let mut messages = Vec::new();
        for (n, &start) in starts.iter().enumerate() {
            let end = if n + 1 < starts.len() { starts[n + 1] } else { input.len() };
            if let Some(m) = Message::from_bytes(&input[start..end]) {
                messages.push(m);
            }
        }
        messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_request() {
        let m = Message::new(MessageType::PhoneRequest, Opcode::Anc, 7, vec![0x02, 0x04, 0x01]);
        let bytes = m.encode();
        let parsed = Message::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.msg_type, MessageType::PhoneRequest);
        assert_eq!(parsed.opcode_raw, 0x08);
        assert_eq!(parsed.sequence, 7);
        assert_eq!(parsed.payload, vec![0x02, 0x04, 0x01]);
    }

    #[test]
    fn roundtrip_response() {
        let m = Message::new(MessageType::Response, Opcode::AuthConfirm, 3, vec![0x01]);
        let bytes = m.encode();
        let parsed = Message::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.msg_type, MessageType::Response);
        assert_eq!(parsed.payload, vec![0x01]);
    }
}
