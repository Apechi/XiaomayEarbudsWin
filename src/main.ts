import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  createIcons,
  Plus,
  X,
  ChevronRight,
  Ear,
  EarOff,
  Headphones,
  Zap,
  Hand,
  AudioLines,
  BluetoothOff,
  BluetoothSearching,
  LoaderCircle,
} from "lucide";

interface DiscoveredBuds {
  name: string;
  address: number;
  connected: boolean;
  model: string | null;
}

interface DeviceState {
  battery: {
    left: number | null;
    right: number | null;
    case: number | null;
    left_charging: boolean;
    right_charging: boolean;
    case_charging: boolean;
  };
  firmware: string | null;
  anc_mode: number | null;
  wearing_detection: boolean | null;
}

interface BudsEvent {
  event: "connected" | "authenticated" | "state_updated" | "disconnected";
  state?: DeviceState;
  reason?: string;
}

const $ = <T extends HTMLElement>(id: string): T =>
  document.getElementById(id) as T;

let currentAnc: number | null = null;
let currentFirmware: string | null = null;
let currentModel: string | null = null;
let scanning = false;

function renderIcons() {
  createIcons({
    icons: {
      Plus,
      X,
      ChevronRight,
      Ear,
      EarOff,
      Headphones,
      Zap,
      Hand,
      AudioLines,
      BluetoothOff,
      BluetoothSearching,
      LoaderCircle,
    },
  });
}

function showError(msg: string | null) {
  const el = $("error");
  if (msg) {
    el.textContent = msg;
    el.classList.remove("hidden");
  } else {
    el.classList.add("hidden");
  }
}

function setHeader(name: string, subtitle: string) {
  $("device-name").textContent = name;
  $("subtitle-text").textContent = subtitle;
}

function setView(connected: boolean) {
  $("connected-view").classList.toggle("hidden", !connected);
  $("picker-view").classList.toggle("hidden", connected);
}

function highlightAnc() {
  document.querySelectorAll<HTMLButtonElement>(".anc-btn").forEach((btn) => {
    const isActive = Number(btn.dataset.mode) === currentAnc;
    const wasActive = btn.classList.contains("active");
    btn.classList.toggle("active", isActive);

    // Bump the circle when it becomes the active mode.
    if (isActive && !wasActive) {
      const circle = btn.querySelector<HTMLElement>(".anc-circle");
      if (circle) {
        circle.classList.remove("bump");
        void circle.offsetWidth; // restart the animation
        circle.classList.add("bump");
        circle.addEventListener(
          "animationend",
          () => circle.classList.remove("bump"),
          { once: true },
        );
      }
    }
  });
}

function renderBattery(
  side: "left" | "right" | "case",
  level: number | null,
  charging: boolean,
) {
  const pct = $(`batt-${side}`);
  const batt = document.querySelector<HTMLElement>(`[data-batt="${side}"]`);
  if (!batt || !pct) return;

  // The case slot only appears once the buds report the case battery
  // (they're docked); animate in/out on visibility changes.
  if (side === "case") {
    const caseSlot = document.getElementById("case-slot");
    const caseExtras = document.querySelectorAll<HTMLElement>(".case-extra");
    if (caseSlot) {
      const show = level !== null;
      const isVisible = caseSlot.classList.contains("visible");
      const isLeaving = caseSlot.classList.contains("leaving");
      if (show && !isVisible) {
        caseSlot.classList.remove("leaving");
        caseSlot.classList.add("visible");
        caseExtras.forEach((el) => {
          el.classList.remove("leaving");
          el.classList.add("visible");
        });
      } else if (!show && isVisible && !isLeaving) {
        caseSlot.classList.add("leaving");
        caseExtras.forEach((el) => el.classList.add("leaving"));
        const done = () => {
          caseExtras.forEach((el) => el.classList.remove("visible", "leaving"));
        };
        caseSlot.addEventListener("animationend", done, { once: true });
        setTimeout(done, 500); // safety net
      }
    }
  }

  if (level === null) {
    pct.textContent = "–";
    pct.classList.add("unknown");
    batt.classList.remove("charging");
    const fill = batt.querySelector<HTMLElement>(".battery-fill");
    if (fill) fill.style.width = "0%";
    return;
  }
  pct.textContent = `${level}%`;
  pct.classList.remove("unknown");
  batt.classList.toggle("charging", charging);
  const fill = batt.querySelector<HTMLElement>(".battery-fill");
  if (fill) {
    fill.style.width = `${level}%`;
    fill.classList.toggle("low", level <= 20 && !charging);
  }
}

function renderHeroStatus() {
  let status = "Connected";
  if (currentFirmware) status += ` · v${currentFirmware}`;
  $("hero-status").textContent = status;
}

function renderState(state: DeviceState) {
  renderBattery("left", state.battery.left, state.battery.left_charging);
  renderBattery("right", state.battery.right, state.battery.right_charging);
  renderBattery("case", state.battery.case, state.battery.case_charging);

  currentFirmware = state.firmware;
  renderHeroStatus();

  if (state.anc_mode !== null && state.anc_mode !== currentAnc) {
    currentAnc = state.anc_mode;
    highlightAnc();
  }
}

function setConnected(model: string | null, subtitle = "Connected") {
  currentModel = model;
  setHeader(model ?? "Xiaomi Earbuds", subtitle);
  setView(true);
}

function setDisconnected() {
  currentAnc = null;
  currentFirmware = null;
  currentModel = null;
  setHeader("Xiaomi Earbuds", "Manage your earphones");
  setView(false);
}

/* ------------------------------ picker ------------------------------ */

function openPicker() {
  $("picker-overlay").classList.remove("hidden");
  void scan();
}

function closePicker() {
  $("picker-overlay").classList.add("hidden");
}

async function scan() {
  if (scanning) return;
  scanning = true;
  const btn = $("scan-btn") as HTMLButtonElement;
  btn.disabled = true;
  showError(null);

  const list = $("device-list");
  list.innerHTML = `
    <div class="picker-hint">
      <i data-lucide="loader-circle" class="spin"></i> Scanning…
    </div>`;
  renderIcons();

  try {
    const devices: DiscoveredBuds[] = await invoke("scan_buds");
    if (devices.length === 0) {
      list.innerHTML = `<div class="picker-hint">No buds found — wake them up and try again</div>`;
    } else {
      list.innerHTML = "";
      for (const d of devices) {
        const item = document.createElement("div");
        item.className = "device-item";
        item.innerHTML = `
          <span class="list-icon"><i data-lucide="bluetooth-searching"></i></span>
          <div class="device-name"></div>
          <button class="connect-btn">Connect</button>`;
        item.querySelector(".device-name")!.innerHTML =
          `<div>${d.name}</div><div class="device-sub">${d.model ?? "Unknown model"}${d.connected ? " · connected" : ""}</div>`;
        item.querySelector("button")!.addEventListener("click", async (e) => {
          const b = e.currentTarget as HTMLButtonElement;
          b.disabled = true;
          b.textContent = "…";
          showError(null);
          setHeader(d.name, "Connecting…");
          try {
            await invoke("connect_buds", { address: d.address, name: d.name });
            closePicker();
            void refreshState();
          } catch (err) {
            showError(String(err));
            b.disabled = false;
            b.textContent = "Connect";
          }
        });
        list.appendChild(item);
      }
      renderIcons();
    }
  } catch (e) {
    list.innerHTML = `<div class="picker-hint">Scan failed: ${String(e)}</div>`;
  } finally {
    btn.disabled = false;
    scanning = false;
  }
}

async function refreshState() {
  try {
    const s = await invoke<{
      connected: boolean;
      authenticated: boolean;
      model: string | null;
      state: DeviceState;
    }>("connection_state");
    if (s.authenticated) {
      setConnected(s.model);
      renderState(s.state);
    } else if (s.connected) {
      setConnected(s.model, "Connecting…");
    }
  } catch {
    /* ignore */
  }
}

window.addEventListener("DOMContentLoaded", () => {
  renderIcons();

  $("scan-btn").addEventListener("click", openPicker);
  $("empty-scan-btn").addEventListener("click", openPicker);
  $("picker-close").addEventListener("click", closePicker);
  document
    .querySelector(".overlay-backdrop")
    ?.addEventListener("click", closePicker);

  document.querySelectorAll<HTMLButtonElement>(".anc-btn").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const mode = Number(btn.dataset.mode);
      try {
        await invoke("set_anc", { mode });
        currentAnc = mode;
        highlightAnc();
      } catch (e) {
        showError(String(e));
      }
    });
  });

  $("disconnect-item").addEventListener("click", async () => {
    try {
      await invoke("disconnect_buds");
    } catch (e) {
      showError(String(e));
    }
  });

  listen<BudsEvent>("buds-event", async (ev) => {
    const e = ev.payload;
    switch (e.event) {
      case "connected":
        $("hero-status").textContent = "Connecting…";
        break;
      case "authenticated":
        // State (model/firmware/battery) arrives via state_updated right after.
        setConnected(currentModel);
        $("hero-status").textContent = "Connected";
        break;
      case "state_updated":
        if (e.state) renderState(e.state);
        break;
      case "disconnected":
        setDisconnected();
        if (e.reason && e.reason !== "requested" && e.reason !== "closed") {
          showError(`Disconnected: ${e.reason}`);
        }
        break;
    }
  });

  void refreshState();
  setInterval(refreshState, 4000);
});
