<div align="center">

# Xiaomi Earbuds

**A Windows desktop companion for Redmi / Xiaomi Buds — battery, noise control & live sync, straight from the desktop.**

[![Release](https://img.shields.io/badge/release-v0.0.1-2f6ff2)](https://github.com/Apechi/XiomayEarbudsWin/releases)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-0078d4)](#)
[![Built with](https://img.shields.io/badge/built%20with-Tauri%202%20%C2%B7%20Rust-ffc131)](#)

</div>

---

**Xiaomi Earbuds** brings the companion-app experience of Redmi / Xiaomi TWS earbuds to Windows.
It speaks the earbuds' own Bluetooth Classic control protocol (RFCOMM) directly — no pairing
rituals, no phone app, no admin rights — and gives you live battery, noise-control switching
and device status in a native desktop window.

> **Tested end-to-end on Redmi Buds 8 Lite**, and built to work across the Redmi Buds
> 3 Pro — 8 series protocol family.

## Features

- **Live battery** — Left / Right / Case, updated in real time as the buds report state
  (the case slot appears automatically while the buds are docked)
- **Noise control** — Off / Noise cancellation / Transparency, two-way synced:
  switch it from the app *or* by long-pressing the buds themselves
- **Instant discovery** — scans the Windows Bluetooth radio cache, so buds appear
  in under a second (no 60-second BLE-style enumeration)
- **No pairing required** — authenticates with the buds' own challenge/response handshake
- **Multi-model support** — device profiles for the Redmi Buds family, with a generic
  fallback so unknown future models still work for battery + ANC
- **Auto-reconnect** — remembers your buds and reconnects on app start
- **Tray-friendly** — close to tray, keep managing your buds in the background
- **Lightweight** — a single ~8 MB binary, powered by Tauri 2 + Rust

## Install

Grab the latest installer from the
[**Releases**](https://github.com/Apechi/XiomayEarbudsWin/releases) page:

| File | Type |
|---|---|
| `Xiaomi.Earbuds_x.x.x_x64-setup.exe` | NSIS installer (recommended) |
| `Xiaomi.Earbuds_x.x.x_x64_en-US.msi` | MSI package |

> **Requirements:** Windows 10/11 with Bluetooth, and your buds must have been
> connected (or at least seen) by Windows at least once. If they don't show up,
> wake them out of the case and rescan.

## Build from source

```bash
git clone https://github.com/Apechi/XiomayEarbudsWin.git
cd XiomayEarbudsWin
npm install

# dev (requires the Rust toolchain + MSVC Build Tools)
npm run tauri dev

# release installers
npm run tauri build
```

## How it works

The buds expose a custom RFCOMM service (`0000fd2d-0000-1000-8000-00805f9b34fb`)
over Bluetooth Classic — the same channel the official Android app uses. This project:

1. **Discovers** buds via the Win32 Bluetooth radio cache
2. **Connects** over RFCOMM using the WinRT `RfcommDeviceService` + `StreamSocket` APIs
3. **Authenticates** with a custom SAFER+ challenge/response handshake
4. **Speaks** a length-framed binary protocol (`FE DC BA … EF`) with TLV payloads for
   battery, firmware, ANC and configuration — with the proper ACKs so the buds
   don't drop the session

The protocol notes live in [`docs/PROTOCOL.md`](docs/PROTOCOL.md) and are based on the
outstanding community reverse-engineering done by
[**Gadgetbridge**](https://codeberg.org/Freeyourgadget/Gadgetbridge)
(AGPL — this project is a clean-room Rust reimplementation of the protocol ideas).

## Supported devices

| Device | Status |
|---|---|
| Redmi Buds 8 Lite | Fully tested |
| Redmi Buds 8 Active | Same protocol family |
| Redmi Buds 6 / 6 Active / 6 Play | Profiled |
| Redmi Buds 5 Pro / 4 Active / 3 Pro | Profiled |
| Unknown Redmi / Xiaomi buds | Generic profile (battery + ANC) |

## Disclaimer

This is a **community project** and is **not affiliated with, endorsed by, or connected to
Xiaomi Corporation or Redmi in any way.** "Xiaomi", "Redmi" and "Mi" are trademarks of their
respective owners and are used here solely to identify device compatibility
(nominative use). No Xiaomi software, firmware, or proprietary code is included or
distributed in this repository.

Product imagery is used for identification purposes only; see credits below.

## Credits

- [**Gadgetbridge**](https://codeberg.org/Freeyourgadget/Gadgetbridge) — the protocol
  reverse-engineering that made this possible
- [**Lucide**](https://lucide.dev) — UI icons
- [**Tauri**](https://tauri.app) — app framework
- Buds product image courtesy of [LMT e-shop](https://www.mmedia.lv/), used for device
  identification only. All product names, logos and images are property of their
  respective owners.

## License

MIT — see [LICENSE](LICENSE).
