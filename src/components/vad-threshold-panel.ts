import {
  calibrateVadThreshold,
  type VadEngine,
  type VadThresholdMode,
} from "../api";
import { t, vadEngineShortLabel } from "../i18n";
import { notifyVadMicDeviceChanged } from "./vad-monitor";

let calibrating = false;

export { notifyVadMicDeviceChanged };

export function renderVadThresholdPanel(
  mode: VadThresholdMode,
  manualThreshold: number,
  autoThreshold: number,
  vadEngine: VadEngine = "silero",
  sileroAvailable = true,
  activeVadEngine?: string,
): string {
  const effective = mode === "auto" ? autoThreshold : manualThreshold;
  const vadChipEngine = activeVadEngine ?? vadEngine;

  return `
    <section class="vad-threshold-panel" data-vad-threshold-panel>
      <div class="vad-threshold-head">
        <div class="vad-threshold-title-row">
          <h3 class="vad-threshold-title">${escapeHtml(t("settings.vadThreshold"))}</h3>
          ${renderVadActiveChip(vadChipEngine)}
        </div>
        <span class="vad-threshold-speech" data-vad-speech-indicator hidden>
          ${escapeHtml(t("settings.vadSpeechDetected"))}
        </span>
      </div>

      <label class="field">
        <span>${escapeHtml(t("settings.vadEngine"))}</span>
        <select name="vad_engine" class="select-input">
          <option value="silero" ${vadEngine === "silero" ? "selected" : ""} ${sileroAvailable ? "" : "disabled"}>
            ${escapeHtml(t("settings.vadEngineSilero"))}
          </option>
          <option value="webrtc" ${vadEngine === "webrtc" ? "selected" : ""}>
            ${escapeHtml(t("settings.vadEngineWebRtc"))}
          </option>
        </select>
        ${
          sileroAvailable
            ? ""
            : `<p class="field-hint">${escapeHtml(t("settings.vadEngineSileroUnavailable"))}</p>`
        }
      </label>

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
  panel.querySelector<HTMLSelectElement>('select[name="vad_engine"]')?.addEventListener("change", (event) => {
    const select = event.currentTarget as HTMLSelectElement;
    updateVadActiveChip(panel, select.value);
    onChange();
  });

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
  setThresholdDisplay(panel, effective);
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
  setThresholdDisplay(panel, value);
  const autoThreshold = Number(
    panel.querySelector<HTMLInputElement>("[data-vad-auto-threshold]")?.value ?? 12,
  );
  updateThresholdLabel(panel, "manual", value, autoThreshold, value);
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

function renderVadActiveChip(engine: string): string {
  const short = vadEngineShortLabel(engine);
  const title = t("settings.vadActiveChipTitle", { engine: short });
  return `<span class="vad-active-chip" data-vad-active-chip title="${escapeHtml(title)}" aria-label="${escapeHtml(title)}">${escapeHtml(short)}</span>`;
}

function updateVadActiveChip(panel: HTMLElement, engine: string): void {
  const chip = panel.querySelector<HTMLElement>("[data-vad-active-chip]");
  if (!chip) {
    return;
  }
  const short = vadEngineShortLabel(engine);
  const title = t("settings.vadActiveChipTitle", { engine: short });
  chip.textContent = short;
  chip.title = title;
  chip.setAttribute("aria-label", title);
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}
