// Diagnostic CLI: dumps Bluetooth Classic devices via Win32 radio cache.
// Run: cargo run --bin btscan

use app_lib::bt::discovery::scan_buds;

fn main() {
    match scan_buds() {
        Ok(devices) => {
            println!("buds found: {}", devices.len());
            for d in devices {
                println!(
                    "  name={:?} addr=0x{:012X} connected={}",
                    d.name, d.address, d.connected
                );
            }
        }
        Err(e) => println!("scan failed: {e}"),
    }
}
