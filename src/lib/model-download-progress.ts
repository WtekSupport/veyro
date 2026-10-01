import { t } from "../i18n";
import { escapeHtml } from "./html";

/** Progress payload shared by Whisper, LLM, Silero, and separation model downloads. */
export interface ModelDownloadProgressLike {
  downloaded: number;
  total: number | null;
  percent: number | null;
}

export function formatModelDownloadProgress(progress: ModelDownloadProgressLike): string {
  if (progress.percent !== null && progress.percent >= 0) {
    return t("settings.downloadingWhisper", { percent: Math.round(progress.percent) });
  }
  const downloadedMb = (progress.downloaded / (1024 * 1024)).toFixed(1);
  return t("settings.downloadingWhisperUnknown", { downloaded: downloadedMb });
}

export function modelDownloadProgressPercent(
  progress: ModelDownloadProgressLike | null,
): number | null {
  if (progress?.percent === null || progress?.percent === undefined) {
    return null;
  }
  return Math.max(0, Math.min(100, Math.round(progress.percent)));
}

export function renderModelDownloadProgressBlock(
  progress: ModelDownloadProgressLike,
  percent: number | null,
  dataAttr: string,
): string {
  const hint = formatModelDownloadProgress(progress);
  return `
    <div class="vocal-sep-model-download" ${dataAttr}>
      <span class="field-hint" data-model-download-hint>${escapeHtml(hint)}</span>
      <div
        class="download-progress"
        data-model-download-progress
        role="progressbar"
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow="${percent ?? 0}"
      >
        <div
          class="download-progress-bar ${percent === null ? "is-indeterminate" : ""}"
          data-model-download-bar
          style="${percent === null ? "" : `width: ${percent}%;`}"
        ></div>
      </div>
    </div>
  `;
}

export function patchModelDownloadProgressDom(
  scope: ParentNode,
  progress: ModelDownloadProgressLike,
  panelSelector: string,
): void {
  const panel = scope.querySelector<HTMLElement>(panelSelector);
  if (!panel) {
    return;
  }
  const percent = modelDownloadProgressPercent(progress);
  const hint = panel.querySelector<HTMLElement>("[data-model-download-hint]");
  if (hint) {
    hint.textContent = formatModelDownloadProgress(progress);
  }
  const bar = panel.querySelector<HTMLElement>("[data-model-download-bar]");
  const progressRoot = panel.querySelector<HTMLElement>("[data-model-download-progress]");
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
