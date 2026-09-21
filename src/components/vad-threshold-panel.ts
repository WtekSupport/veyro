import {
  calibrateVadThreshold,
  type SileroModelDownloadProgress,
  type VadEngine,
  type VadThresholdMode,
} from "../api";
import { t, vadEngineShortLabel } from "../i18n";
import { getState, setSettings } from "../state";
import { notifyVadMicDeviceChanged } from "./vad-monitor";

let calibrating = false;

export { notifyVadMicDeviceChanged };

function formatDownloadProgress(progress: SileroModelDownloadProgress): string {
  if (progress.percent !== null && progress.percent >= 0) {
    return t("settings.downloadingWhisper", { percent: Math.round(progress.percent) });
  }
  const downloadedMb = (progress.downloaded / (1024 * 1024)).toFixed(1);
  return t("settings.downloadingWhisperUnknown", { downloaded: downloadedMb });
}

export function renderVadThresholdPanel(
  mode: VadThresholdMode,
  manualThreshold: number,
  autoThreshold: number,
  vadEngine: VadEngine = "silero",
  sileroAvailable = true,
  activeVadEngine?: string,
  sileroCompiled = false,
  sileroModelOnDisk = true,
  sileroVadDownload: SileroModelDownloadProgress | null = null,
  sileroVadDownloadPercent: number | null = null,
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
        <select
          name="vad_engine"
          class="select-input"
          aria-label="${escapeHtml(t("settings.vadEngine"))}"
        >
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

      ${renderSileroVadModelAction(
        sileroCompiled,
        sileroModelOnDisk,
        sileroVadDownload,
        sileroVadDownloadPercent,
      )}

      <label class="field checkbox vad-threshold-auto-check">
        <input type="checkbox" data-vad-threshold-auto ${mode === "auto" ? "checked" : ""} />
        <span>${escapeHtml(t("settings.vadThresholdAuto"))}</span>
      </label>
      <input
        type="hidden"
        name="vad_threshold_mode"
        value="${mode}"
        data-vad-threshold-mode-field
      />

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
        </div>
      </div>

      <input type="hidden" name="vad_auto_threshold_percent" value="${autoThreshold}" data-vad-auto-threshold />
    </section>
  `;
}

function renderSileroVadModelAction(
  sileroCompiled: boolean,
  sileroModelOnDisk: boolean,
  sileroVadDownload: SileroModelDownloadProgress | null,
  downloadProgressPercent: number | null,
): string {
  if (!sileroCompiled) {
    return "";
  }
  if (sileroModelOnDisk && !sileroVadDownload) {
    return "";
  }

  if (sileroVadDownload) {
    return `
      <div class="silero-vad-download" data-silero-vad-download>
        <div class="field-row">
          <span class="field-hint" data-silero-vad-download-hint>${escapeHtml(formatDownloadProgress(sileroVadDownload))}</span>
        </div>
        <div
          class="download-progress"
          data-silero-vad-download-progress
          role="progressbar"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow="${downloadProgressPercent ?? 0}"
        >
          <div
            class="download-progress-bar ${
              downloadProgressPercent === null ? "is-indeterminate" : ""
            }"
            data-silero-vad-download-bar
            style="${downloadProgressPercent === null ? "" : `width: ${downloadProgressPercent}%;`}"
          ></div>
        </div>
      </div>
    `;
  }

  return `
    <div class="field-row model-action" data-silero-vad-download-action>
      <button type="button" class="btn-secondary" data-download-silero-vad-model>
        ${escapeHtml(t("settings.downloadWhisperModel"))}
      </button>
    </div>
  `;
}

function vadDownloadPercent(progress: SileroModelDownloadProgress | null): number | null {
  if (progress?.percent === null || progress?.percent === undefined) {
    return null;
  }
  return Math.max(0, Math.min(100, Math.round(progress.percent)));
}

export function ensureSileroVadDownloadUi(progress: SileroModelDownloadProgress): void {
  if (!document.querySelector("[data-silero-vad-download]")) {
    const action = document.querySelector("[data-silero-vad-download-action]");
    if (action) {
      const percent = vadDownloadPercent(progress);
      action.outerHTML = `
      <div class="silero-vad-download" data-silero-vad-download>
        <div class="field-row">
          <span class="field-hint" data-silero-vad-download-hint>${escapeHtml(formatDownloadProgress(progress))}</span>
        </div>
        <div
          class="download-progress"
          data-silero-vad-download-progress
          role="progressbar"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow="${percent ?? 0}"
        >
          <div
            class="download-progress-bar ${percent === null ? "is-indeterminate" : ""}"
            data-silero-vad-download-bar
            style="${percent === null ? "" : `width: ${percent}%;`}"
          ></div>
        </div>
      </div>`;
    }
  }
  updateSileroVadDownloadUi(progress);
}

export function updateSileroVadDownloadUi(
  progress: SileroModelDownloadProgress | null,
): void {
  const panel = document.querySelector<HTMLElement>("[data-silero-vad-download]");
  if (!panel || !progress) {
    return;
  }

  const percent = vadDownloadPercent(progress);
  const hint = panel.querySelector<HTMLElement>("[data-silero-vad-download-hint]");
  if (hint) {
    hint.textContent = formatDownloadProgress(progress);
  }

  const bar = panel.querySelector<HTMLElement>("[data-silero-vad-download-bar]");
  const progressRoot = panel.querySelector<HTMLElement>("[data-silero-vad-download-progress]");
  if (!bar || !progressRoot) {
    return;
  }

  if (percent === null) {
    bar.style.width = "";
    bar.classList.add("is-indeterminate");
    progressRoot.setAttribute("aria-valuenow", "0");
    return;
  }

  bar.classList.remove("is-indeterminate");
  bar.style.width = `${percent}%`;
  progressRoot.setAttribute("aria-valuenow", String(percent));
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

  panel.querySelector<HTMLInputElement>("[data-vad-threshold-auto]")?.addEventListener("change", () => {
    syncVadThresholdModeField(panel);
    syncVadThresholdControls(panel);
    onChange();
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

function syncVadThresholdModeField(panel: HTMLElement): void {
  const hidden = panel.querySelector<HTMLInputElement>("[data-vad-threshold-mode-field]");
  const checkbox = panel.querySelector<HTMLInputElement>("[data-vad-threshold-auto]");
  if (hidden && checkbox) {
    hidden.value = checkbox.checked ? "auto" : "manual";
  }
}

function syncVadThresholdControls(panel: HTMLElement): void {
  syncVadThresholdModeField(panel);
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
  const checkbox = panel.querySelector<HTMLInputElement>("[data-vad-threshold-auto]");
  if (checkbox) {
    return checkbox.checked ? "auto" : "manual";
  }
  const hidden = panel.querySelector<HTMLInputElement>("[data-vad-threshold-mode-field]");
  return hidden?.value === "manual" ? "manual" : "auto";
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
    const current = getState().settings;
    if (current) {
      setSettings({
        ...current,
        vad_threshold_mode: "auto",
        vad_auto_threshold_percent: threshold,
      });
    } else {
      const autoInput = panel.querySelector<HTMLInputElement>("[data-vad-auto-threshold]");
      if (autoInput) {
        autoInput.value = String(threshold);
      }
      const autoCheckbox = panel.querySelector<HTMLInputElement>("[data-vad-threshold-auto]");
      if (autoCheckbox) {
        autoCheckbox.checked = true;
      }
      syncVadThresholdModeField(panel);
      syncVadThresholdControls(panel);
    }
  } catch {
    // Calibration errors surface via global error handling when invoked from save flow.
  } finally {
    calibrating = false;
    const livePanel = form.querySelector<HTMLElement>("[data-vad-threshold-panel]");
    if (livePanel) {
      syncVadThresholdControls(livePanel);
    }
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
