// EQ preset brute-force scanner for Redmi Buds 8 Lite.
//
// Connects, authenticates, then sends EQ preset values one at a time with a
// pause between each. Listen with music playing and note which values change
// the sound. Incoming notifications are ACKed (un-ACKed notifications make
// the buds drop the connection).
//
// Usage: cargo run --example eqscan -- <BT_ADDRESS_HEX> [from] [to]
//   e.g.  cargo run --example eqscan -- 548450DB489F 0 30

use std::sync::mpsc;
use std::time::Duration;

use windows::Storage::Streams::{ByteOrder, DataReader, DataWriter, InputStreamOptions};

use app_lib::bt::discovery::connect_rfcomm;
use app_lib::bt::framing::{Message, MessageType, Opcode};
use app_lib::bt::protocol::Protocol;
use app_lib::bt::wait::{wait_op, wait_op_forever, DEFAULT_TIMEOUT};

fn send(writer: &DataWriter, msg: Message) -> bool {
    writer.WriteBytes(msg.encode().as_slice()).is_ok()
        && wait_op(writer.StoreAsync().unwrap(), DEFAULT_TIMEOUT).is_ok()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let addr_str = args
        .get(1)
        .unwrap_or_else(|| panic!("usage: eqscan <BT_ADDRESS_HEX> [from] [to]"))
        .trim_start_matches("0x")
        .to_string();
    let address = u64::from_str_radix(&addr_str, 16).expect("hex address");
    let from: u8 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let to: u8 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(30);

    let socket = connect_rfcomm(address).expect("connect");
    let output = socket.OutputStream().expect("output");
    let writer = DataWriter::CreateDataWriter(&output).expect("writer");
    writer.SetByteOrder(ByteOrder::BigEndian).ok();

    // Reader thread: blocking LoadAsync -> chunks over a channel (same
    // pattern as the app session; never canceled, so never poisoned).
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let input_ref =
        windows::core::AgileReference::new(&socket.InputStream().expect("input")).expect("agile");
    std::thread::spawn(move || {
        let input = input_ref.resolve().expect("resolve");
        let reader = DataReader::CreateDataReader(&input).expect("reader");
        reader.SetByteOrder(ByteOrder::BigEndian).ok();
        reader.SetInputStreamOptions(InputStreamOptions::Partial).ok();
        loop {
            let chunk = (|| -> Option<Vec<u8>> {
                let n = wait_op_forever(reader.LoadAsync(1024).ok()?).ok()? as usize;
                if n == 0 {
                    return None;
                }
                let mut bytes = vec![0u8; n];
                reader.ReadBytes(&mut bytes).ok()?;
                Some(bytes)
            })();
            match chunk {
                Some(bytes) => {
                    if tx.send(bytes).is_err() {
                        break;
                    }
                }
                None => break,
            }
        }
    });

    // ---- Auth handshake ----
    let mut proto = Protocol::new();
    send(&writer, proto.encode_start_authentication());
    let mut authenticated = false;
    while !authenticated {
        let bytes = match rx.recv_timeout(Duration::from_secs(8)) {
            Ok(b) => b,
            Err(_) => panic!("no auth reply from buds"),
        };
        for msg in Message::split_piggybacked(&bytes) {
            match msg.opcode {
                Opcode::AuthChallenge => {
                    if msg.msg_type == MessageType::Response {
                        send(&writer, proto.encode_auth_confirm());
                    } else if msg.payload.len() >= 17 {
                        let mut challenge = [0u8; 16];
                        challenge.copy_from_slice(&msg.payload[1..17]);
                        let seq = msg.sequence;
                        send(&writer, proto.encode_challenge_response(&challenge, seq));
                    }
                }
                Opcode::AuthConfirm => {
                    let seq = msg.sequence;
                    send(&writer, proto.encode_auth_confirm_response(seq));
                    if msg.msg_type == MessageType::Response {
                        authenticated = true;
                    }
                }
                _ => {}
            }
        }
    }
    println!("authenticated");

    // Ask for current EQ preset, then drain replies for a moment, ACKing
    // status notifications so the buds don't drop us.
    send(&writer, proto.encode_get_config(0x07));
    drain_and_ack(&rx, &writer, &mut proto, 1500);

    println!("scanning EQ preset values {from}..={to}, 3s each — listen and note changes!");
    for v in from..=to {
        println!(">>> preset value {v}");
        send(&writer, proto.encode_eq_preset(v));
        drain_and_ack(&rx, &writer, &mut proto, 2900);
    }
    println!("done");
}

/// Consume incoming messages for `ms`, ACKing status/config notifications.
fn drain_and_ack(
    rx: &mpsc::Receiver<Vec<u8>>,
    writer: &DataWriter,
    proto: &mut Protocol,
    ms: u64,
) {
    let deadline = std::time::Instant::now() + Duration::from_millis(ms);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match rx.recv_timeout(remaining) {
            Ok(bytes) => {
                for msg in Message::split_piggybacked(&bytes) {
                    match msg.opcode {
                        Opcode::ReportStatus => {
                            let seq = msg.sequence;
                            send(writer, proto.encode_status_ack(seq));
                        }
                        Opcode::NotifyConfig => {
                            let seq = msg.sequence;
                            send(writer, proto.encode_notify_config_ack(seq));
                        }
                        Opcode::GetConfig => {
                            println!(
                                "  GET_CONFIG reply: payload={:02x?}",
                                msg.payload
                            );
                        }
                        _ => {}
                    }
                }
            }
            Err(_) => break,
        }
    }
}
