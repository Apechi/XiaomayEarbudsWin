// Discovery of Xiaomi/Redmi buds over Bluetooth Classic.
//
// Scan: Win32 Bluetooth* API (BluetoothFindFirstDevice) — reads the radio's device
// cache instantly. (WinRT AEP enumeration also works but took 60+s on the test
// machine, so it's not used for scanning.)
//
// Connect: WinRT RfcommDeviceService + StreamSocket (works with unpaired devices).

use serde::Serialize;
use windows::core::Result;

use windows::Devices::Bluetooth::BluetoothCacheMode;
use windows::Devices::Bluetooth::BluetoothDevice;
use windows::Devices::Bluetooth::Rfcomm::{RfcommDeviceService, RfcommServiceId};
use windows::Networking::Sockets::{SocketProtectionLevel, StreamSocket};
use windows::Win32::Devices::Bluetooth::{
    BluetoothFindDeviceClose, BluetoothFindFirstDevice, BluetoothFindFirstRadio,
    BluetoothFindRadioClose, BluetoothFindNextDevice, BLUETOOTH_DEVICE_INFO,
    BLUETOOTH_DEVICE_SEARCH_PARAMS, BLUETOOTH_FIND_RADIO_PARAMS,
};
use windows::Win32::Foundation::HANDLE;

use super::wait::{wait_action, wait_op, DEFAULT_TIMEOUT};

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredDevice {
    pub name: String,
    pub address: u64,
    pub connected: bool,
}

pub const XIAOMI_CTRL_SERVICE: windows::core::GUID =
    windows::core::GUID::from_u128(0x0000fd2d_0000_1000_8000_00805f9b34fb);

/// Enumerate Bluetooth Classic devices from the radio cache and return those
/// whose name looks like Xiaomi/Redmi buds.
pub fn scan_buds() -> Result<Vec<DiscoveredDevice>> {
    let mut devices = Vec::new();

    unsafe {
        // Grab the first BT radio; a single radio is the normal case.
        let mut radio = HANDLE::default();
        let radio_params = BLUETOOTH_FIND_RADIO_PARAMS {
            dwSize: std::mem::size_of::<BLUETOOTH_FIND_RADIO_PARAMS>() as u32,
        };
        let radio_find = BluetoothFindFirstRadio(&radio_params, &mut radio)?;
        if radio_find.is_invalid() {
            return Err(windows::core::Error::new(
                windows::core::HRESULT(-1),
                "no Bluetooth radio found",
            ));
        }

        let search = BLUETOOTH_DEVICE_SEARCH_PARAMS {
            dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32,
            fReturnAuthenticated: true.into(),
            fReturnRemembered: true.into(),
            fReturnUnknown: true.into(),
            fReturnConnected: true.into(),
            fIssueInquiry: false.into(), // cache only — instant; live inquiry comes later
            cTimeoutMultiplier: 0,
            hRadio: radio,
        };

        let mut info = BLUETOOTH_DEVICE_INFO {
            dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32,
            ..Default::default()
        };

        let find = BluetoothFindFirstDevice(&search, &mut info)?;
        if !find.is_invalid() {
            loop {
                let name = utf16_to_string(&info.szName);
                if is_buds_name(&name) {
                    let address = info.Address.Anonymous.ullLong;
                    devices.push(DiscoveredDevice {
                        name,
                        address,
                        connected: info.fConnected.as_bool(),
                    });
                }
                if !BluetoothFindNextDevice(find, &mut info).is_ok() {
                    break;
                }
            }
            let _ = BluetoothFindDeviceClose(find);
        }

        let _ = BluetoothFindRadioClose(radio_find);
    }

    Ok(devices)
}

fn utf16_to_string(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

fn is_buds_name(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("buds") || n.contains("redmi") || n.contains("xiaomi")
}

/// Open an RFCOMM StreamSocket to the Xiaomi control service (0000fd2d-...).
/// Retries briefly on WSAEADDRINUSE in case the previous session's socket is
/// still being released.
pub fn connect_rfcomm(address: u64) -> Result<StreamSocket> {
    const ADDR_IN_USE: i32 = 0x8007_2740u32 as i32;
    let mut attempt = 0;
    loop {
        match connect_rfcomm_once(address) {
            Ok(s) => return Ok(s),
            Err(e) if e.code().0 == ADDR_IN_USE && attempt < 6 => {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
            Err(e) => return Err(e),
        }
    }
}

fn connect_rfcomm_once(address: u64) -> Result<StreamSocket> {
    let device = wait_op(
        BluetoothDevice::FromBluetoothAddressAsync(address)?,
        DEFAULT_TIMEOUT,
    )?;
    let service_id = RfcommServiceId::FromUuid(XIAOMI_CTRL_SERVICE)?;
    let result = wait_op(
        device.GetRfcommServicesForIdWithCacheModeAsync(&service_id, BluetoothCacheMode::Uncached)?,
        DEFAULT_TIMEOUT,
    )?;
    let services = result.Services()?;
    if services.Size()? == 0 {
        return Err(windows::core::Error::new(
            windows::core::HRESULT(-1),
            "Xiaomi RFCOMM service (0000fd2d-...) not found on device",
        ));
    }
    let service: RfcommDeviceService = services.GetAt(0)?;

    let socket = StreamSocket::new()?;
    let host = service.ConnectionHostName()?;
    let name = service.ConnectionServiceName()?;
    wait_action(
        socket.ConnectWithProtectionLevelAsync(&host, &name, SocketProtectionLevel::PlainSocket)?,
        DEFAULT_TIMEOUT,
    )?;

    Ok(socket)
}
