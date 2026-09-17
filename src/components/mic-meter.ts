import { ensureMicMonitor, getMicLevel } from "../api";
import { t } from "../i18n";
const POLL_MS = 80;
let pollTimer: ReturnType<typeof setInterval> | undefined;
let monitorArmed = false;

export function renderMicMeter(inline = false): string {
  if (inline) {
    return `
      <div class="mic-meter mic-meter--inline" data-mic-meter>
        <div class="mic-meter-track" aria-hidden="true">
          <div class="mic-meter-fill" data-mic-meter-fill style="width: 0%"></div>
        </div>
        <div class="mic-meter-foot">
          <span class="mic-meter-value" data-mic-meter-value>0%</span>
        </div>
      </div>
    `;
  }

  return `
    <div class="mic-meter" data-mic-meter>
      <div class="mic-meter-head">
        <span class="mic-meter-label">${escapeHtml(t("mic.inputLevel"))}</span>
        <span class="mic-meter-value" data-mic-meter-value>0%</span>
      </div>
      <div class="mic-meter-track" aria-hidden="true">
        <div class="mic-meter-fill" data-mic-meter-fill style="width: 0%"></div>
      </div>
      <p class="mic-meter-hint" data-mic-meter-hint>${escapeHtml(t("mic.speakToTest"))}</p>
    </div>
  `;
}

export function startMicLevelMonitor(): void {
  stopMicLevelMonitor();
  monitorArmed = false;
  void armMicMonitor();
  void refreshMicLevel();
  pollTimer = setInterval(() => {
    void refreshMicLevel();
  }, POLL_MS);
}

export function notifyMicDeviceChanged(): void {
  monitorArmed = false;
  void armMicMonitor();
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

export function stopMicLevelMonitor(): void {
  if (pollTimer !== undefined) {
    clearInterval(pollTimer);
    pollTimer = undefined;
  }
}

async function refreshMicLevel(): Promise<void> {
  const meters = document.querySelectorAll<HTMLElement>("[data-mic-meter]");
  if (meters.length === 0) {
    return;
  }

  let level = 0;
  try {
    level = await getMicLevel();
  } catch {
    level = 0;
  }

  for (const meter of Array.from(meters)) {
    const fill = meter.querySelector("[data-mic-meter-fill]") as HTMLElement | null;
    const value = meter.querySelector("[data-mic-meter-value]") as HTMLElement | null;
    if (fill) {
      fill.style.width = `${level}%`;
      fill.dataset.level =
        level >= 45 ? "hot" : level >= 12 ? "warm" : level >= 3 ? "low" : "silent";
    }
    if (value) {
      value.textContent = `${level}%`;
    }
  }
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}
