import { ensureMicMonitor, getMicMonitorSnapshot } from "../api";
import { t } from "../i18n";
import { drawVadLevelGraph } from "./vad-level-graph";

const POLL_MS = 80;

let pollTimer: ReturnType<typeof setInterval> | undefined;
let monitorArmed = false;
let pollStatusTab = false;
let pollVoiceTab = false;

function updateStatusLevelDisplay(level: number): void {
  const el = document.querySelector<HTMLElement>("[data-status-level]");
  if (el) {
    el.textContent = `${Math.round(level)}%`;
  }
}

function currentThresholdMode(panel: HTMLElement): "auto" | "manual" {
  const checkbox = panel.querySelector<HTMLInputElement>("[data-vad-threshold-auto]");
  if (checkbox) {
    return checkbox.checked ? "auto" : "manual";
  }
  const hidden = panel.querySelector<HTMLInputElement>("[data-vad-threshold-mode-field]");
  return hidden?.value === "manual" ? "manual" : "auto";
}

function setThresholdDisplay(panel: HTMLElement, percent: number): void {
  const levelValue = panel.querySelector<HTMLElement>("[data-vad-level-value]");
  const clamped = Math.max(3, Math.min(80, percent));
  if (levelValue) {
    levelValue.textContent = `${clamped}%`;
  }
}

function updateThresholdLabel(
  panel: HTMLElement,
  mode: "auto" | "manual",
  effective: number,
  autoThreshold: number,
  manualThreshold: number,
): void {
  const label = panel.querySelector<HTMLElement>("[data-vad-threshold-label]");
  if (!label) {
    return;
  }
  label.textContent =
    mode === "auto"
      ? t("settings.vadThresholdAutoValue", { value: autoThreshold })
      : t("settings.vadThresholdManualValue", { value: manualThreshold });
  void effective;
}

async function armMicMonitor(): Promise<void> {
  if (monitorArmed) {
    return;
  }
  try {
    await ensureMicMonitor();
    monitorArmed = true;
  } catch {
    monitorArmed = false;
  }
}

function recomputeVadPollTimer(): void {
  const wantPoll = pollStatusTab || pollVoiceTab;
  if (wantPoll && pollTimer === undefined) {
    void armMicMonitor();
    void refreshVadGraphsFromSnapshot();
    pollTimer = setInterval(() => {
      void refreshVadGraphsFromSnapshot();
    }, POLL_MS);
  } else if (!wantPoll && pollTimer !== undefined) {
    clearInterval(pollTimer);
    pollTimer = undefined;
  }
}

/** Mic level graph on Status tab; Capture tab when the VAD panel is shown. */
export function syncVadGraphPolling(options: {
  statusTab: boolean;
  voiceTab: boolean;
}): void {
  pollStatusTab = options.statusTab;
  pollVoiceTab = options.voiceTab;
  recomputeVadPollTimer();
}

export function notifyVadMicDeviceChanged(): void {
  monitorArmed = false;
  void armMicMonitor();
}

export async function refreshVadGraphsFromSnapshot(): Promise<void> {
  const statusCanvas = document.querySelector<HTMLCanvasElement>(
    "[data-status-dashboard] [data-vad-graph]",
  );
  const voicePanel = document.querySelector<HTMLElement>("[data-vad-threshold-panel]");
  const statusActive = Boolean(statusCanvas);
  const voiceActive = voicePanel && !voicePanel.hidden;
  if (!statusActive && !voiceActive) {
    return;
  }

  let snapshot;
  try {
    snapshot = await getMicMonitorSnapshot();
  } catch {
    return;
  }

  if (statusActive && statusCanvas) {
    drawVadLevelGraph(
      statusCanvas,
      snapshot.history,
      snapshot.effective_threshold_percent,
      snapshot.level_percent,
    );
    updateStatusLevelDisplay(snapshot.level_percent);
    const speechIndicator = document.querySelector<HTMLElement>(
      "[data-status-dashboard] [data-vad-speech-indicator]",
    );
    if (speechIndicator) {
      speechIndicator.hidden = !snapshot.speech_active;
    }
  }

  if (voicePanel && voiceActive) {
    const speechIndicator = voicePanel.querySelector<HTMLElement>("[data-vad-speech-indicator]");
    if (speechIndicator) {
      speechIndicator.hidden = !snapshot.speech_active;
    }

    const mode = currentThresholdMode(voicePanel);
    if (mode === "auto") {
      const autoInput = voicePanel.querySelector<HTMLInputElement>("[data-vad-auto-threshold]");
      if (autoInput && Number(autoInput.value) !== snapshot.effective_threshold_percent) {
        autoInput.value = String(snapshot.effective_threshold_percent);
      }
      setThresholdDisplay(voicePanel, snapshot.effective_threshold_percent);
      updateThresholdLabel(
        voicePanel,
        "auto",
        snapshot.effective_threshold_percent,
        snapshot.effective_threshold_percent,
        Number(voicePanel.querySelector<HTMLInputElement>("[data-vad-threshold-slider]")?.value ?? 15),
      );
    }
  }

  document.querySelectorAll<HTMLCanvasElement>("[data-vad-graph]").forEach((canvas) => {
    drawVadLevelGraph(
      canvas,
      snapshot.history,
      snapshot.effective_threshold_percent,
      snapshot.level_percent,
    );
  });
}
