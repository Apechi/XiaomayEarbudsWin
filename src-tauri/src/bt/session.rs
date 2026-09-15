// A connected buds session: owns the RFCOMM socket, performs the auth handshake,
// runs the read loop, handles commands, and emits state events.
//
// Threading: a dedicated reader thread blocks on DataReader.LoadAsync (never
// canceled — canceling a WinRT DataReader operation poisons it with
// RO_E_CLOSED) and forwards raw chunks over a channel. The session loop
// multiplexes between incoming chunks and control commands with recv_timeout.

use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use windows::core::{Error, HRESULT, Result};
use windows::Storage::Streams::{ByteOrder, DataReader, DataWriter, InputStreamOptions};

use super::discovery::connect_rfcomm;
use super::framing::{Message, MessageType, Opcode};
use super::protocol::{AncMode, DeviceState, Protocol};
use super::wait::{wait_op, wait_op_forever, DEFAULT_TIMEOUT};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum SessionEvent {
    Connected,
    Authenticated,
    StateUpdated { state: DeviceState },
    Disconnected { reason: String },
}

pub enum SessionCommand {
    SetAnc(AncMode),
    SetEqPreset(u8),
    SetEqCurve([i8; 10]),
    Disconnect,
}

const LOOP_TICK: Duration = Duration::from_millis(250);

pub fn run_session(
    address: u64,
    cmd_rx: Receiver<SessionCommand>,
    event_tx: Sender<SessionEvent>,
) {
    let result = run_session_inner(address, &cmd_rx, &event_tx);
    let reason = match result {
        Ok(()) => "closed".to_string(),
        Err(e) => format!("{e}"),
    };
    let _ = event_tx.send(SessionEvent::Disconnected { reason });
}

fn run_session_inner(
    address: u64,
    cmd_rx: &Receiver<SessionCommand>,
    event_tx: &Sender<SessionEvent>,
) -> Result<()> {
    let socket = connect_rfcomm(address)?;

    let output = socket.OutputStream()?;
    let writer = DataWriter::CreateDataWriter(&output)?;
    writer.SetByteOrder(ByteOrder::BigEndian)?;

    let _ = event_tx.send(SessionEvent::Connected);

    // Reader thread: blocking reads -> raw byte chunks.
    let (chunk_tx, chunk_rx) = std::sync::mpsc::channel::<Result<Vec<u8>>>();
    let input_ref = windows::core::AgileReference::new(&socket.InputStream()?)?;
    std::thread::spawn(move || {
        let input = match input_ref.resolve() {
            Ok(i) => i,
            Err(e) => {
                let _ = chunk_tx.send(Err(e));
                return;
            }
        };
        let reader = match DataReader::CreateDataReader(&input) {
            Ok(r) => r,
            Err(e) => {
                let _ = chunk_tx.send(Err(e));
                return;
            }
        };
        let _ = reader.SetByteOrder(ByteOrder::BigEndian);
        // Partial: complete as soon as ANY bytes are available, not 1024.
        let _ = reader.SetInputStreamOptions(InputStreamOptions::Partial);
        loop {
            let chunk = (|| -> Result<Vec<u8>> {
                // Block indefinitely — idling is normal on this socket. The
                // op completes when data arrives or fails when the socket
                // closes. Never cancel: canceling poisons the DataReader.
                let n = wait_op_forever(reader.LoadAsync(1024)?)? as usize;
                if n == 0 {
                    return Err(Error::new(HRESULT(-4), "socket closed by device"));
                }
                let mut bytes = vec![0u8; n];
                reader.ReadBytes(&mut bytes)?;
                Ok(bytes)
            })();
            let closed = chunk.is_err();
            if chunk_tx.send(chunk).is_err() || closed {
                break;
            }
        }
    });

    // ---- Auth handshake -------------------------------------------------
    let mut proto = Protocol::new();
    let mut state = DeviceState::default();

    send(&writer, &proto.encode_start_authentication().encode())?;

    let mut authenticated = false;
    let mut auth_deadline = 0u32; // ticks of LOOP_TICK; 8s ≈ 32 ticks
    while !authenticated {
        auth_deadline += 1;
        if auth_deadline > 32 {
            return Err(Error::new(HRESULT(-2), "authentication timed out"));
        }
        if !handle_commands(cmd_rx, &mut proto, &writer, event_tx) {
            return Ok(());
        }
        match chunk_rx.recv_timeout(LOOP_TICK) {
            Ok(Ok(bytes)) => {
                for msg in Message::split_piggybacked(&bytes) {
                    if process_message(msg, &mut proto, &mut state, &writer, event_tx)? {
                        authenticated = true;
                    }
                }
            }
            Ok(Err(e)) => return Err(e),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(Error::new(HRESULT(-3), "reader thread died"));
            }
        }
    }

    let _ = event_tx.send(SessionEvent::Authenticated);

    // ---- Main loop -------------------------------------------------------
    loop {
        if !handle_commands(cmd_rx, &mut proto, &writer, event_tx) {
            break;
        }
        match chunk_rx.recv_timeout(LOOP_TICK) {
            Ok(Ok(bytes)) => {
                for msg in Message::split_piggybacked(&bytes) {
                    process_message(msg, &mut proto, &mut state, &writer, event_tx)?;
                }
            }
            Ok(Err(e)) => return Err(e),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    socket.Close()?;
    Ok(())
}

/// Handle pending commands; returns false if a disconnect was requested.
fn handle_commands(
    cmd_rx: &Receiver<SessionCommand>,
    proto: &mut Protocol,
    writer: &DataWriter,
    event_tx: &Sender<SessionEvent>,
) -> bool {
    while let Ok(cmd) = cmd_rx.try_recv() {
        match cmd {
            SessionCommand::Disconnect => {
                let _ = event_tx.send(SessionEvent::Disconnected {
                    reason: "requested".into(),
                });
                return false;
            }
            SessionCommand::SetAnc(mode) => {
                let _ = send(writer, &proto.encode_set_anc(mode).encode());
            }
            SessionCommand::SetEqPreset(preset) => {
                let _ = send(writer, &proto.encode_eq_preset(preset).encode());
            }
            SessionCommand::SetEqCurve(bands) => {
                let _ = send(writer, &proto.encode_eq_curve(&bands).encode());
            }
        }
    }
    true
}

/// Returns true when the message completes authentication.
fn process_message(
    msg: Message,
    proto: &mut Protocol,
    state: &mut DeviceState,
    writer: &DataWriter,
    event_tx: &Sender<SessionEvent>,
) -> Result<bool> {
    let mut authenticated = false;
    match msg.opcode {
        Opcode::AuthChallenge => {
            if msg.msg_type == MessageType::Response {
                // Buds confirmed our challenge -> send AUTH_CONFIRM.
                send(writer, &proto.encode_auth_confirm().encode())?;
            } else {
                // Buds challenge us -> compute response (payload: 0x01 + challenge).
                if msg.payload.len() >= 17 {
                    let mut challenge = [0u8; 16];
                    challenge.copy_from_slice(&msg.payload[1..17]);
                    let seq = msg.sequence;
                    send(
                        writer,
                        &proto.encode_challenge_response(&challenge, seq).encode(),
                    )?;
                }
            }
        }
        Opcode::AuthConfirm => {
            if msg.msg_type == MessageType::Response {
                // First auth step done -> final confirmation + info requests.
                send(writer, &proto.encode_auth_confirm_response(msg.sequence).encode())?;
                send(writer, &proto.encode_get_device_info().encode())?;
                send(writer, &proto.encode_get_device_run_info().encode())?;
                authenticated = true;
            } else {
                send(writer, &proto.encode_auth_confirm_response(msg.sequence).encode())?;
            }
        }
        Opcode::GetDeviceInfo => {
            Protocol::decode_device_info(&msg.payload, state);
            let _ = event_tx.send(SessionEvent::StateUpdated { state: state.clone() });
        }
        Opcode::GetDeviceRunInfo => {
            Protocol::decode_run_info(&msg.payload, state);
            let _ = event_tx.send(SessionEvent::StateUpdated { state: state.clone() });
        }
        Opcode::ReportStatus => {
            Protocol::decode_device_update(&msg.payload, state);
            let seq = msg.sequence;
            send(writer, &proto.encode_status_ack(seq).encode())?;
            let _ = event_tx.send(SessionEvent::StateUpdated { state: state.clone() });
        }
        Opcode::GetConfig => {
            Protocol::decode_config(&msg.payload, state);
            let _ = event_tx.send(SessionEvent::StateUpdated { state: state.clone() });
        }
        Opcode::NotifyConfig => {
            Protocol::decode_notify_config(&msg.payload, state);
            let seq = msg.sequence;
            send(writer, &proto.encode_notify_config_ack(seq).encode())?;
            let _ = event_tx.send(SessionEvent::StateUpdated { state: state.clone() });
        }
        _ => {}
    }
    Ok(authenticated)
}

fn send(writer: &DataWriter, bytes: &[u8]) -> Result<()> {
    writer.WriteBytes(bytes)?;
    wait_op(writer.StoreAsync()?, DEFAULT_TIMEOUT)?;
    Ok(())
}
