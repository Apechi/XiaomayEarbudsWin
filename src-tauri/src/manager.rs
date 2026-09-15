// Connection manager: owns the active session thread and bridges it to Tauri.

use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serde::Serialize;

use crate::bt::discovery;
use crate::bt::profiles;
use crate::bt::protocol::{AncMode, DeviceState};
use crate::bt::session::{run_session, SessionCommand, SessionEvent};

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredBuds {
    pub name: String,
    pub address: u64,
    pub connected: bool,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnState {
    pub connected: bool,
    pub authenticated: bool,
    pub model: Option<String>,
    pub state: DeviceState,
}

impl Default for ConnState {
    fn default() -> Self {
        ConnState {
            connected: false,
            authenticated: false,
            model: None,
            state: DeviceState::default(),
        }
    }
}

pub struct ConnectionManager {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    cmd_tx: Option<Sender<SessionCommand>>,
    thread: Option<JoinHandle<()>>,
    address: Option<u64>,
    conn: ConnState,
}

impl ConnectionManager {
    pub fn new() -> Self {
        ConnectionManager {
            inner: Arc::new(Mutex::new(Inner {
                cmd_tx: None,
                thread: None,
                address: None,
                conn: ConnState::default(),
            })),
        }
    }

    pub fn scan(&self) -> Result<Vec<DiscoveredBuds>, String> {
        let devices = discovery::scan_buds().map_err(|e| e.to_string())?;
        Ok(devices
            .into_iter()
            .map(|d| DiscoveredBuds {
                model: profiles::match_profile(&d.name).map(|p| p.model.to_string()),
                name: d.name,
                address: d.address,
                connected: d.connected,
            })
            .collect())
    }

    /// Spawn the session thread for a device and start pumping events.
    ///
    /// Returns Ok(true) when a new session was started, Ok(false) when the
    /// device is already the active connection (no-op).
    pub fn connect(
        &self,
        address: u64,
        name: &str,
        on_event: impl Fn(SessionEvent) + Send + 'static,
    ) -> Result<bool, String> {
        let model = profiles::match_profile(name)
            .map(|p| p.model.to_string())
            .unwrap_or_else(|| name.to_string());
        let mut inner = self.inner.lock().unwrap();

        // Already connected to this exact device? Nothing to do.
        if inner.cmd_tx.is_some() && inner.address == Some(address) {
            return Ok(false);
        }

        // Connecting to a different device: tear down the current session.
        if let Some(tx) = inner.cmd_tx.take() {
            let _ = tx.send(SessionCommand::Disconnect);
            inner.address = None;
            // Give the old session a moment to release the socket.
            drop(inner);
            std::thread::sleep(std::time::Duration::from_millis(400));
            inner = self.inner.lock().unwrap();
        }

        let (cmd_tx, cmd_rx) = channel::<SessionCommand>();
        let (event_tx, event_rx) = channel::<SessionEvent>();

        let handle = std::thread::spawn(move || {
            run_session(address, cmd_rx, event_tx.clone());
        });

        // Event pump thread: forward to the callback + track state.
        let inner_arc = Arc::clone(&self.inner);
        std::thread::spawn(move || {
            while let Ok(event) = event_rx.recv() {
                match &event {
                    SessionEvent::Connected => {
                        if let Ok(mut i) = inner_arc.lock() {
                            i.conn.connected = true;
                            i.conn.authenticated = false;
                        }
                    }
                    SessionEvent::Authenticated => {
                        if let Ok(mut i) = inner_arc.lock() {
                            i.conn.authenticated = true;
                            i.conn.model = Some(model.clone());
                        }
                    }
                    SessionEvent::StateUpdated { state } => {
                        if let Ok(mut i) = inner_arc.lock() {
                            i.conn.state = state.clone();
                        }
                    }
                    SessionEvent::Disconnected { .. } => {
                        if let Ok(mut i) = inner_arc.lock() {
                            i.conn = ConnState::default();
                            i.cmd_tx = None;
                            i.address = None;
                        }
                        on_event(event.clone());
                        break;
                    }
                }
                on_event(event.clone());
            }
            // Make sure the session thread is joined-ish; it exits on its own.
            let _ = handle;
            // Note: `handle` moved here; session thread terminates when its
            // channel closes or the socket errors out.
        });

        inner.cmd_tx = Some(cmd_tx);
        inner.address = Some(address);
        inner.thread = None; // managed by pump; avoid storing non-Send handle
        Ok(true)
    }

    pub fn set_anc(&self, mode: AncMode) -> Result<(), String> {
        let inner = self.inner.lock().unwrap();
        match &inner.cmd_tx {
            Some(tx) => tx.send(SessionCommand::SetAnc(mode)).map_err(|e| e.to_string()),
            None => Err("not connected".into()),
        }
    }

    pub fn set_eq_preset(&self, preset: u8) -> Result<(), String> {
        let inner = self.inner.lock().unwrap();
        match &inner.cmd_tx {
            Some(tx) => tx
                .send(SessionCommand::SetEqPreset(preset))
                .map_err(|e| e.to_string()),
            None => Err("not connected".into()),
        }
    }

    pub fn set_eq_curve(&self, bands: [i8; 10]) -> Result<(), String> {
        let inner = self.inner.lock().unwrap();
        match &inner.cmd_tx {
            Some(tx) => tx
                .send(SessionCommand::SetEqCurve(bands))
                .map_err(|e| e.to_string()),
            None => Err("not connected".into()),
        }
    }

    pub fn disconnect(&self) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap();
        if let Some(tx) = inner.cmd_tx.take() {
            let _ = tx.send(SessionCommand::Disconnect);
        }
        inner.address = None;
        Ok(())
    }

    pub fn state(&self) -> ConnState {
        self.inner.lock().unwrap().conn.clone()
    }
}
