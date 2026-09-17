import {
  calibrateVadThreshold,
  ensureMicMonitor,
  getMicMonitorSnapshot,
  type VadThresholdMode,
} from "../api";
import { t } from "../i18n";

const POLL_MS = 80;

let pollTimer: ReturnType<typeof setInterval> | undefined;
let monitorArmed = false;
let calibrating = false;

export function renderVadThresholdPanel(
  mode: VadThresholdMode,
  manualThreshold: number,
  autoThreshold: number,
  pushToTalk: boolean,
): string {
  const hidden = pushToTalk ? "hidden" : "";
  const effective = mode === "auto" ? autoThreshold : manualThreshold;

  return `
    <section class="vad-threshold-panel continuous-only" data-vad-threshold-panel ${hidden}>
      <div class="vad-threshold-head">
        <h3 class="vad-threshold-title">${escapeHtml(t("settings.vadThreshold"))}</h3>
        <span class="vad-threshold-speech" data-vad-speech-indicator hidden>
          ${escapeHtml(t("settings.vadSpeechDetected"))}
        </span>
      </div>

      <div class="vad-threshold-mode">
        <label class="vad-threshold-mode-option">
          <input
            type="radio"
            name="vad_threshold_mode"
            value="auto"
            ${mode === "auto" ? "checked" : ""}
          />
          <span>${escapeHtml(t("settings.vadThresholdAuto"))}</span>
        </label>
        <label class="vad-threshold-mode-option">
          <input
            type="radio"
            name="vad_threshold_mode"
            value="manual"
            ${mode === "manual" ? "checked" : ""}
          />
          <span>${escapeHtml(t("settings.vadThresholdManual"))}</span>
        </label>
      </div>

      <div class="vad-threshold-graph-wrap">
        <canvas
          class="vad-threshold-graph"
          data-vad-graph
          width="560"
          height="120"
          aria-label="${escapeHtml(t("settings.vadThreshold"))}"
        ></canvas>
        <div
          class="vad-threshold-line"
          data-vad-threshold-line
          style="bottom: ${effective}%"
        ></div>
      </div>

      <div class="vad-threshold-controls">
        <div class="vad-threshold-value-row">
          <span data-vad-threshold-label>
            ${
              mode === "auto"
                ? escapeHtml(t("settings.vadThresholdAutoValue", { value: autoThreshold }))
                : escapeHtml(t("settings.vadThresholdManualValue", { value: manualThreshold }))
            }
          </span>
          <span class="vad-threshold-level" data-vad-level-value>${effective}%</span>
        </div>

        <input
          class="vad-threshold-slider"
          type="range"
          name="vad_voice_threshold_percent"
          min="3"
          max="80"
          step="1"
          value="${manualThreshold}"
          data-vad-threshold-slider
          ${mode === "manual" ? "" : "disabled"}
        />

        <div class="vad-threshold-actions">
          <button
            type="button"
            class="btn btn-secondary"
            data-vad-calibrate
            ${mode === "auto" ? "" : "hidden"}
          >
            ${escapeHtml(t("settings.vadCalibrate"))}
          </button>
          <p class="field-hint vad-threshold-hint">${escapeHtml(t("settings.vadThresholdHint"))}</p>
          <p class="field-hint">${escapeHtml(t("settings.vadAdvancedHint"))}</p>
        </div>
      </div>

      <input type="hidden" name="vad_auto_threshold_percent" value="${autoThreshold}" data-vad-auto-threshold />
    </section>
  `;
}

export function bindVadThresholdPanel(
  form: HTMLFormElement,
  onChange: () => void,
): void {
  const panel = form.querySelector<HTMLElement>("[data-vad-threshold-panel]");
  if (!panel) {
    return;
  }

  const slider = panel.querySelector<HTMLInputElement>("[data-vad-threshold-slider]");
  const calibrateButton = panel.querySelector<HTMLButtonElement>("[data-vad-calibrate]");
  const modeInputs = panel.querySelectorAll<HTMLInputElement>('input[name="vad_threshold_mode"]');

  modeInputs.forEach((input) => {
    input.addEventListener("change", () => {
      syncVadThresholdControls(panel);
      onChange();
    });
  });

  slider?.addEventListener("input", () => {
    updateThresholdLineFromSlider(panel);
  });

  slider?.addEventListener("change", () => {
    onChange();
  });

  calibrateButton?.addEventListener("click", () => {
    void runCalibration(form, panel);
  });
}

export function startVadThresholdMonitor(): void {
  stopVadThresholdMonitor();
  monitorArmed = false;
  void armMicMonitor();
  void refreshVadMonitor();
  pollTimer = setInterval(() => {
    void refreshVadMonitor();
  }, POLL_MS);
}

export function stopVadThresholdMonitor(): void {
  if (pollTimer !== undefined) {
    clearInterval(pollTimer);
    pollTimer = undefined;
  }
}

export function notifyVadMicDeviceChanged(): void {
  monitorArmed = false;
  void armMicMonitor();
}

function syncVadThresholdControls(panel: HTMLElement): void {
  const mode = currentThresholdMode(panel);
  const slider = panel.querySelector<HTMLInputElement>("[data-vad-threshold-slider]");
  const calibrateButton = panel.querySelector<HTMLButtonElement>("[data-vad-calibrate]");
  const autoThreshold = Number(
    panel.querySelector<HTMLInputElement>("[data-vad-auto-threshold]")?.value ?? 12,
  );

  if (slider) {
    slider.disabled = mode !== "manual";
  }

  if (calibrateButton) {
    calibrateButton.hidden = mode !== "auto";
    calibrateButton.disabled = calibrating;
    calibrateButton.textContent = calibrating
      ? t("settings.vadCalibrating")
      : t("settings.vadCalibrate");
  }

  const effective = mode === "auto" ? autoThreshold : Number(slider?.value ?? 15);
  setThresholdLine(panel, effective);
  updateThresholdLabel(panel, mode, effective, autoThreshold, Number(slider?.value ?? 15));
}

function currentThresholdMode(panel: HTMLElement): VadThresholdMode {
  const checked = panel.querySelector<HTMLInputElement>(
    'input[name="vad_threshold_mode"]:checked',
  );
  return checked?.value === "manual" ? "manual" : "auto";
}

function updateThresholdLineFromSlider(panel: HTMLElement): void {
  const slider = panel.querySelector<HTMLInputElement>("[data-vad-threshold-slider]");
  if (!slider) {
    return;
  }
  const value = Number(slider.value);
  setThresholdLine(panel, value);
  const autoThreshold = Number(
    panel.querySelector<HTMLInputElement>("[data-vad-auto-threshold]")?.value ?? 12,
  );
  updateThresholdLabel(panel, "manual", value, autoThreshold, value);
}

function setThresholdLine(panel: HTMLElement, percent: number): void {
  const line = panel.querySelector<HTMLElement>("[data-vad-threshold-line]");
  const levelValue = panel.querySelector<HTMLElement>("[data-vad-level-value]");
  const clamped = Math.max(3, Math.min(80, percent));
  if (line) {
    line.style.bottom = `${clamped}%`;
  }
  if (levelValue) {
    levelValue.textContent = `${clamped}%`;
  }
}

function updateThresholdLabel(
  panel: HTMLElement,
  mode: VadThresholdMode,
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

async function runCalibration(form: HTMLFormElement, panel: HTMLElement): Promise<void> {
  if (calibrating) {
    return;
  }

  calibrating = true;
  syncVadThresholdControls(panel);

  try {
    const threshold = await calibrateVadThreshold();
    const autoInput = panel.querySelector<HTMLInputElement>("[data-vad-auto-threshold]");
    if (autoInput) {
      autoInput.value = String(threshold);
    }
    const autoRadio = panel.querySelector<HTMLInputElement>(
      'input[name="vad_threshold_mode"][value="auto"]',
    );
    if (autoRadio) {
      autoRadio.checked = true;
    }
    syncVadThresholdControls(panel);

    const { getSettings } = await import("../api");
    const { setSettings } = await import("../state");
    setSettings(await getSettings());
  } catch {
    // Calibration errors surface via global error handling when invoked from save flow.
  } finally {
    calibrating = false;
    syncVadThresholdControls(panel);
    void form.dispatchEvent(new Event("change", { bubbles: true }));
  }
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

async function refreshVadMonitor(): Promise<void> {
  const panel = document.querySelector<HTMLElement>("[data-vad-threshold-panel]");
  if (!panel || panel.hidden) {
    return;
  }

  let snapshot;
  try {
    snapshot = await getMicMonitorSnapshot();
  } catch {
    return;
  }

  drawGraph(panel, snapshot.history, snapshot.effective_threshold_percent, snapshot.level_percent);

  const speechIndicator = panel.querySelector<HTMLElement>("[data-vad-speech-indicator]");
  if (speechIndicator) {
    speechIndicator.hidden = !snapshot.speech_active;
  }

  const mode = currentThresholdMode(panel);
  if (mode === "auto") {
    const autoInput = panel.querySelector<HTMLInputElement>("[data-vad-auto-threshold]");
    if (autoInput && Number(autoInput.value) !== snapshot.effective_threshold_percent) {
      autoInput.value = String(snapshot.effective_threshold_percent);
    }
    setThresholdLine(panel, snapshot.effective_threshold_percent);
    updateThresholdLabel(
      panel,
      "auto",
      snapshot.effective_threshold_percent,
      snapshot.effective_threshold_percent,
      Number(panel.querySelector<HTMLInputElement>("[data-vad-threshold-slider]")?.value ?? 15),
    );
  }
}

function drawGraph(
  panel: HTMLElement,
  history: number[],
  threshold: number,
  currentLevel: number,
): void {
  const canvas = panel.querySelector<HTMLCanvasElement>("[data-vad-graph]");
  if (!canvas) {
    return;
  }

  const ctx = canvas.getContext("2d");
  if (!ctx) {
    return;
  }

  const width = canvas.width;
  const height = canvas.height;
  ctx.clearRect(0, 0, width, height);

  ctx.fillStyle = "rgba(255, 255, 255, 0.04)";
  ctx.fillRect(0, height * (1 - threshold / 100), width, height * (threshold / 100));

  const points =
    history.length > 0
      ? history
      : Array.from({ length: 2 }, () => currentLevel);

  ctx.strokeStyle = "rgba(96, 165, 250, 0.9)";
  ctx.lineWidth = 2;
  ctx.beginPath();

  points.forEach((level, index) => {
    const x = (index / Math.max(points.length - 1, 1)) * (width - 8) + 4;
    const y = height - (Math.max(0, Math.min(100, level)) / 100) * (height - 8) - 4;
    if (index === 0) {
      ctx.moveTo(x, y);
    } else {
      ctx.lineTo(x, y);
    }
  });
  ctx.stroke();

  ctx.strokeStyle = "rgba(250, 204, 21, 0.85)";
  ctx.setLineDash([4, 4]);
  ctx.beginPath();
  const thresholdY = height - (threshold / 100) * (height - 8) - 4;
  ctx.moveTo(0, thresholdY);
  ctx.lineTo(width, thresholdY);
  ctx.stroke();
  ctx.setLineDash([]);
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}
