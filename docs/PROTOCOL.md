# Redmi Buds / Xiaomi Earbuds Control Protocol (RedmiBuds family)

Source: Gadgetbridge (codeberg.org/Freeyourgadget/Gadgetbridge), `service/devices/redmibuds/`
Cross-checked with: MiBudsClient (Redmi Buds 6 Play, Python desktop client — same RFCOMM transport).
Applies to: Redmi Buds 3 Pro, 4 Active, 5 Pro, 6, 6 Active, 6 Play, 8 Active, 8 Lite(?).

## ⚠️ Transport is Bluetooth Classic RFCOMM, NOT BLE

- The buds expose a custom SPP-like RFCOMM service:
  **Service UUID `0000fd2d-0000-1000-8000-00805f9b34fb`** (Xiaomi's shared control UUID for the whole family).
- Connection: outgoing RFCOMM socket to the SDP service UUID (Gadgetbridge resolves via SDP; no pairing/bonding required — `BONDING_STYLE_NONE`).
- Windows implementation: WinRT `Windows.Devices.Bluetooth` — `BluetoothDevice.FromBluetoothAddressAsync()` → `GetRfcommServicesAsync()` / `GetRfcommServicesForIdAsync(RfcommServiceId.FromUuid(...))` → `RfcommDeviceService` → `StreamSocket`. Windows RFCOMM API supports connecting to **unpaired** devices.
- Discovery: device name over BT Classic, e.g. `REDMI Buds 8 Active` (per-model regex in coordinators). Buds 8 Lite expected: `REDMI Buds 8 Lite` (verify on device).

## Message framing (`protocol/Message.java`)

All fields big-endian. Messages can be piggybacked in one read → split on header occurrences.

```
FE DC BA            3-byte header
[type]              1 byte   message type (below)
[opcode]            1 byte
[payload_len]       2 bytes  = payload length + (2 if type is a response, else 1)
[00]                1 byte   ONLY when type is a RESPONSE (bit 0x40 clear)
[seq]               1 byte   sequence number, increments per sent message
[payload]           N bytes
EF                  1-byte trailer
```

Message types:
| Type | Code | Meaning |
|---|---|---|
| PHONE_REQUEST | 0xC4 | phone → buds request |
| RESPONSE | 0x04 | buds → phone response |
| EARBUDS_REQUEST | 0xC0 | buds → phone request |
| EARBUDS_NOTIFY | 0xC7 | buds → phone notification |

## Opcodes (`protocol/Opcode.java`)

| Opcode | Code | Purpose |
|---|---|---|
| GET_DEVICE_INFO | 0x02 | firmware, VID/PID, battery |
| ANC | 0x08 | set ambient sound control / ear detection |
| GET_DEVICE_RUN_INFO | 0x09 | current ANC mode, wear detection |
| REPORT_STATUS | 0x0E | async status updates (battery, ANC) — must be ACKed |
| AUTH_CHALLENGE | 0x50 | auth step 1 |
| AUTH_CONFIRM | 0x51 | auth step 2 |
| SET_CONFIG | 0xF2 | set gestures / EQ / toggles |
| GET_CONFIG | 0xF3 | read config |
| NOTIFY_CONFIG | 0xF4 | async config notify — must be ACKed |

## Authentication handshake

1. Phone → `PHONE_REQUEST / AUTH_CHALLENGE`, payload = `0x01` + 16 random bytes.
2. Buds → `RESPONSE / AUTH_CHALLENGE`, payload = `0x01` + 16-byte challenge.
   Phone computes response = SAFER+_variant.encrypt(SEQ, keySchedule(challenge)) and sends
   `RESPONSE / AUTH_CHALLENGE` with `0x01` + response. (Buds don't seem to verify strictly.)
3. Phone → `PHONE_REQUEST / AUTH_CONFIRM`, payload `{0x01, 0x00}`.
4. Buds → `RESPONSE / AUTH_CONFIRM`; phone replies `RESPONSE / AUTH_CONFIRM` with `{0x01}`.
5. Auth done → send:
   - `GET_DEVICE_INFO` payload `{FF FF FF FF}` (request all TLVs)
   - `GET_DEVICE_RUN_INFO` payload `{FF FF FF FF}`
   - `GET_CONFIG` for each known config id

The cipher is a custom Bluetooth SAFER+ (128-bit key, 8 rounds):
- `SEQ` (plaintext, fixed): `11 22 33 33 22 11 11 22 33 33 22 11 11 22 33 33`
- 16x16 integer COEFFICIENTS matrix (see `protocol/AuthData.java`)
- keySchedule: XOR `key[15] ^= 6`, rolling 17-byte register rotated left by 5 bits per round key, `keyI[i] = register[(keyIdx+i) % 17] + biasMatrix[keyIdx-1][i]` (byte arithmetic, mod 256)
- biasMatrix / expTab / logTab derived from `45^x mod 257` — all pure math, trivially portable to Rust.

## Payload formats

**Battery** (in GET_DEVICE_INFO response TLV `0x07`, 3 bytes; and REPORT_STATUS TLV `0x00`):
- byte0 = left, byte1 = right, byte2 = case (batteryIndex 1, 2, 0 in GB)
- `0xFF` = unknown; bit7 set = charging; low 7 bits = percentage.

**GET_DEVICE_INFO response** is a TLV stream: `[len][index][data...]`, walk with `i += len + 1`:
- `0x01` firmware (4 bytes, nibble-packed `A.B.C.D` per pair)
- `0x03` VID/PID (4 bytes)
- `0x07` battery (3 bytes, above)

**REPORT_STATUS / device update** TLV stream:
- `0x00` battery (3 bytes)
- `0x04` ANC mode (1 byte)
Every REPORT_STATUS must be ACKed: `RESPONSE / REPORT_STATUS` with empty payload and the **same seq number**.

**NOTIFY_CONFIG** TLV stream: `[len][?][index]`:
- `0x0B` sound control: byte3 = mode (0=off/normal, 1=ANC, 2=transparency), byte4 = strength
- `0x0C` wearing/case position bitmap
ACK with `RESPONSE / NOTIFY_CONFIG`, same seq, empty payload.

**ANC set**: `PHONE_REQUEST / ANC` payload `{0x02, 0x04, mode}` — mode 0/1/2.
**Ear detection set**: `PHONE_REQUEST / ANC` payload `{0x02, 0x06, value}`.

**SET_CONFIG** payload `{len_lo, len_hi, config_id, ...values}`; GET_CONFIG payload `{0x00, config_id}`.
Config ids (`devices/redmibuds/prefs/Configuration.java`): ADAPTIVE_ANC, GESTURES, LONG_GESTURES,
EAR_DETECTION, DOUBLE_CONNECTION, AUTO_ANSWER, ADAPTIVE_SOUND, EQ_PRESET, EQ_CURVE, EFFECT_STRENGTH.

## Per-model differences (device-profile abstraction)

All models share transport + framing + auth + base opcodes. Differences are confined to:
- device-name regex for discovery
- which config ids / payloads the model supports (e.g. 8 Active adds EQ preset + custom EQ + double connection + adaptive sound; 3 Pro has slightly different gesture response layout)
- response payload offsets (minor)

→ One Rust `Transport` (RFCOMM) + `Framing`/`Auth` + a `DeviceProfile` per model
(name pattern + supported feature table + config-id map). New models = new profile data.

## Windows notes

- WinRT RFCOMM from Rust: `windows` crate, features `Devices_Bluetooth`, `Devices_Bluetooth_Rfcomm`, `Networking_Sockets`, `Storage_Streams`.
- Get buds' BT address via `BluetoothDevice.GetBluetoothAddressAsync` after discovery
  (`DeviceInformation.FindAllAsync` with selector `System.Devices.Aep.ProtocolId:="{e0cbf06c-cd8b-4647-bb8b-4b09e449878e}"` OR just query `GetRfcommServicesAsync`).
- `StreamSocket` connects to `socket.ConnectionProtection = None` typical; read/write raw bytes (DataWriter/DataReader or manual buffer).
- No admin rights needed; user may need Bluetooth enabled obviously.

## Open items

- [ ] Buds 8 Lite: confirm exact advertised name + protocol match (likely same family; test on device)
- [ ] ANC mode byte mapping (0/1/2 order) confirm on device
- [ ] Whether 8 Lite supports ear-detection / EQ (Lite models are usually stripped)
