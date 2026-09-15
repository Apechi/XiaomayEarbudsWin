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
  eq_preset: number | null;
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

const EQ_BANDS = [62, 125, 250, 500, 1000, 2000, 4000, 8000, 12000, 16000];

function bandLabel(hz: number): string {
  return hz >= 1000 ? `${hz / 1000}k` : `${hz}`;
}

function buildEqBands() {
  const wrap = $("eq-bands");
  wrap.innerHTML = "";
  EQ_BANDS.forEach((hz, i) => {
    const cell = document.createElement("div");
    cell.className = "eq-band";
    cell.innerHTML = `
      <span class="band-val" id="eq-val-${i}">0</span>
      <input type="range" min="-6" max="6" step="1" value="0" data-band="${i}" />
      <span class="band-freq">${bandLabel(hz)}</span>`;
    wrap.appendChild(cell);
  });
  wrap.querySelectorAll<HTMLInputElement>("input").forEach((inp) => {
    inp.addEventListener("input", () => {
      const v = $(`eq-val-${inp.dataset.band}`);
      if (v) v.textContent = inp.value;
    });
    inp.addEventListener("change", () => void sendEqCurve());
  });
}

async function sendEqCurve() {
  const bands = EQ_BANDS.map((_, i) => {
    const inp = document.querySelector<HTMLInputElement>(
      `input[data-band="${i}"]`,
    );
    return Number(inp?.value ?? 0);
  });
  try {
    await invoke("set_eq_curve", { bands });
    setEqChip(10); // custom
  } catch (e) {
    showError(String(e));
  }
}

function setEqChip(preset: number | null) {
  document.querySelectorAll<HTMLButtonElement>(".chip").forEach((chip) => {
    chip.classList.toggle(
      "active",
      preset !== null && Number(chip.dataset.preset) === preset,
    );
  });
  // The Custom panel is a toggle: open only for the Custom preset.
  $("eq-panel").classList.toggle("hidden", preset !== 10);
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
  if (state.eq_preset !== null && state.eq_preset !== undefined) {
    setEqChip(state.eq_preset);
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

  document.querySelectorAll<HTMLButtonElement>(".chip").forEach((chip) => {
    chip.addEventListener("click", async () => {
      const preset = Number(chip.dataset.preset);
      // Clicking the already-active Custom chip just toggles the panel.
      if (preset === 10 && chip.classList.contains("active")) {
        $("eq-panel").classList.toggle("hidden");
        return;
      }
      try {
        await invoke("set_eq_preset", { preset });
        setEqChip(preset);
      } catch (e) {
        showError(String(e));
      }
    });
  });

  buildEqBands();

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
