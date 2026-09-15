import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  createIcons,
  Plus,
  X,
  ArrowLeft,
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
  Activity,
  Check,
  MousePointerClick,
  Copy,
  Layers,
  Timer,
  Sparkles,
  Link,
  Brain,
  PhoneCall,
  Waves,
  SlidersHorizontal,
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
  gestures: number[] | null;
  double_connection: boolean | null;
  adaptive_sound: boolean | null;
  auto_answer: boolean | null;
  adaptive_anc: boolean | null;
  customized_anc: boolean | null;
  effect_strength_anc: number | null;
  effect_strength_transparency: number | null;
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
      ArrowLeft,
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
      Activity,
      Check,
      MousePointerClick,
      Copy,
      Layers,
      Timer,
      Sparkles,
      Link,
      Brain,
      PhoneCall,
      Waves,
      SlidersHorizontal,
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
  if (connected) {
    // Only force the dashboard when we're coming from the picker/disconnected
    // state — never while the user is browsing a sub-page.
    if (currentPage === null) showPage("main");
  } else {
    currentPage = null;
    for (const p of ["main", "eq", "gestures", "effects"] as PageName[]) {
      $(`view-${p}`).classList.add("hidden");
    }
  }
  $("picker-view").classList.toggle("hidden", connected);
}

/* ------------------------------ router ------------------------------ */

type PageName = "main" | "eq" | "gestures" | "effects";

let currentPage: PageName | null = null;

function showPage(name: PageName) {
  currentPage = name;
  for (const p of ["main", "eq", "gestures", "effects"] as PageName[]) {
    $(`view-${p}`).classList.toggle("hidden", p !== name);
  }
  // Re-render icons in case the page contains icon placeholders.
  renderIcons();
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
      updateEqPreview();
    });
    inp.addEventListener("change", () => void sendEqCurve());
  });
  updateEqPreview();
}

const EQ_PRESET_NAMES: Record<number, string> = {
  21: "Balanced",
  6: "Treble",
  5: "Bass",
  1: "Voice",
  7: "Volume",
  10: "Custom",
};

function updateEqPreview() {
  const line = $("eq-preview-line");
  if (!line) return;
  const inputs = Array.from(
    document.querySelectorAll<HTMLInputElement>("#eq-bands input"),
  );
  const pts = inputs
    .map((inp, i) => {
      const x = (i / (inputs.length - 1)) * 300;
      const y = 40 - (Number(inp.value) / 6) * 34;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
  line.setAttribute("points", pts);
}

function setEqStateFact(preset: number | null) {
  const el = $("eq-state-fact");
  if (!el) return;
  el.textContent =
    preset === null
      ? "Preset: —"
      : `Preset: ${EQ_PRESET_NAMES[preset] ?? `code ${preset}`}`;
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
  // The custom curve panel shows only for the Custom preset.
  $("eq-panel").classList.toggle("hidden", preset !== 10);
  setEqStateFact(preset);
  if (preset === 10) updateEqPreview();
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

/* ---------------------------- gestures ----------------------------- */

// Interaction type bytes (protocol constants).
const G_SINGLE = 4,
  G_DOUBLE = 1,
  G_TRIPLE = 2,
  G_LONG = 3;

// Action values for the Buds 8 family.
const TAP_ACTIONS: [number, string][] = [
  [8, "None"],
  [1, "Play / pause"],
  [2, "Previous track"],
  [3, "Next track"],
  [4, "Volume up"],
  [5, "Volume down"],
];
const LONG_ACTIONS: [number, string][] = [
  [8, "None"],
  [6, "Noise control"],
  [0, "Voice assistant"],
];

function fillGestureSelects() {
  document.querySelectorAll<HTMLSelectElement>(".gesture-select").forEach((sel) => {
    const interaction = Number(sel.closest<HTMLElement>(".gesture-tile")!.dataset.interaction);
    const actions = interaction === G_LONG ? LONG_ACTIONS : TAP_ACTIONS;
    sel.innerHTML = actions
      .map(([v, n]) => `<option value="${v}">${n}</option>`)
      .join("");
    sel.addEventListener("change", async () => {
      try {
        await invoke("set_gesture", {
          interaction,
          left: sel.dataset.side === "left",
          value: Number(sel.value),
        });
      } catch (e) {
        showError(String(e));
      }
    });
  });
}

function renderGestures(g: number[] | null) {
  if (!g || g.length !== 8) return;
  // single L/R, double L/R, triple L/R, long L/R
  const map: Record<number, number[]> = {
    [G_SINGLE]: [g[0], g[1]],
    [G_DOUBLE]: [g[2], g[3]],
    [G_TRIPLE]: [g[4], g[5]],
    [G_LONG]: [g[6], g[7]],
  };
  document.querySelectorAll<HTMLSelectElement>(".gesture-select").forEach((sel) => {
    const interaction = Number(sel.closest<HTMLElement>(".gesture-tile")!.dataset.interaction);
    const pair = map[interaction];
    if (pair) {
      sel.value = String(
        sel.dataset.side === "left" ? pair[0] : pair[1],
      );
    }
  });
}

/* -------------------------- audio effects --------------------------- */

// Toggle element id -> SET_CONFIG id (shared across the Buds 8 family;
// unsupported options are auto-disabled by the probe).
const EFFECT_TOGGLES: [string, number][] = [
  ["tg-double-connection", 0x04],
  ["tg-adaptive-sound", 0x29],
  ["tg-auto-answer", 0x03],
  ["tg-adaptive-anc", 0x25],
  ["tg-customized-anc", 0x3b],
];

function setToggle(id: string, value: boolean | null) {
  const el = document.getElementById(id) as HTMLInputElement | null;
  if (!el) return;
  if (value === null) {
    // Buds didn't answer for this config — not supported on this model.
    el.disabled = true;
    el.checked = false;
    el.closest(".effect-tile")?.classList.add("unsupported");
  } else {
    el.disabled = false;
    el.checked = value;
    el.closest(".effect-tile")?.classList.remove("unsupported");
  }
}

function setStrength(id: string, valId: string, value: number | null) {
  const inp = document.getElementById(id) as HTMLInputElement | null;
  const val = $(valId);
  if (!inp || !val) return;
  if (value === null) {
    inp.disabled = true;
    inp.value = "0";
    val.textContent = "–";
  } else {
    inp.disabled = false;
    inp.value = String(value);
    val.textContent = String(value);
  }
}

function renderEffects(state: DeviceState) {
  for (const [id, configId] of EFFECT_TOGGLES) {
    const value =
      configId === 0x04
        ? state.double_connection
        : configId === 0x29
          ? state.adaptive_sound
          : configId === 0x03
            ? state.auto_answer
            : configId === 0x25
              ? state.adaptive_anc
              : state.customized_anc;
    setToggle(id, value);
  }
  // Strength sliders only make sense while Customized ANC is enabled.
  const showStrength = state.customized_anc === true;
  $("strength-card").classList.toggle("hidden", !showStrength);
  if (showStrength) {
    setStrength("strength-anc", "strength-anc-val", state.effect_strength_anc);
    setStrength(
      "strength-transparency",
      "strength-transparency-val",
      state.effect_strength_transparency,
    );
  }
}

function wireEffects() {
  for (const [id, configId] of EFFECT_TOGGLES) {
    document.getElementById(id)?.addEventListener("change", async (e) => {
      const checked = (e.currentTarget as HTMLInputElement).checked;
      try {
        await invoke("set_bool_config", { configId, value: checked });
      } catch (err) {
        showError(String(err));
      }
    });
  }
  for (const [id, target, valId] of [
    ["strength-anc", 1, "strength-anc-val"],
    ["strength-transparency", 2, "strength-transparency-val"],
  ] as [string, number, string][]) {
    document.getElementById(id)?.addEventListener("change", async (e) => {
      const mode = Number((e.currentTarget as HTMLInputElement).value);
      $(valId).textContent = String(mode);
      try {
        await invoke("set_strength", { target, mode });
      } catch (err) {
        showError(String(err));
      }
    });
  }
}

function renderState(state: DeviceState) {
  renderBattery("left", state.battery.left, state.battery.left_charging);
  renderBattery("right", state.battery.right, state.battery.right_charging);
  renderBattery("case", state.battery.case, state.battery.case_charging);

  currentFirmware = state.firmware;
  renderHeroStatus();
  if (state.firmware) {
    $("eq-note").textContent = ""; // reserved; could show preset later
  }

  if (state.anc_mode !== null && state.anc_mode !== currentAnc) {
    currentAnc = state.anc_mode;
    highlightAnc();
  }
  if (state.eq_preset !== null && state.eq_preset !== undefined) {
    setEqChip(state.eq_preset);
  }
  if (state.gestures) {
    renderGestures(state.gestures);
  }
  renderEffects(state);
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

  // Page navigation.
  document.querySelectorAll<HTMLButtonElement>(".nav-eq").forEach((btn) => {
    btn.addEventListener("click", () => showPage("eq"));
  });
  document.querySelectorAll<HTMLButtonElement>(".nav-gestures").forEach((btn) => {
    btn.addEventListener("click", () => showPage("gestures"));
  });
  document.querySelectorAll<HTMLButtonElement>(".nav-effects").forEach((btn) => {
    btn.addEventListener("click", () => showPage("effects"));
  });
  document.querySelectorAll<HTMLButtonElement>(".back-btn").forEach((btn) => {
    btn.addEventListener("click", () => showPage("main"));
  });

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
  fillGestureSelects();
  wireEffects();

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
