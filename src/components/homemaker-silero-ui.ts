import type {
  Diagnostics,
  SileroModelDownloadProgress,
  SileroTeModelStatus,
  SileroVadModelStatus,
} from "../api";
import { t } from "../i18n";

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function downloadPercent(progress: SileroModelDownloadProgress | null): number | null {
  return progress?.percent ?? null;
}

function formatDownloadProgress(progress: SileroModelDownloadProgress): string {
  if (progress.percent !== null && progress.percent >= 0) {
    return t("settings.downloadingWhisper", { percent: Math.round(progress.percent) });
  }
  const downloadedMb = (progress.downloaded / (1024 * 1024)).toFixed(1);
  return t("settings.downloadingWhisperUnknown", { downloaded: downloadedMb });
}

function renderSileroDownloadBlock(
  buttonLabelKey: "homemaker.downloadImproveRecognition" | "homemaker.downloadImprovePunctuation",
  buttonSelector: string,
  actionSelector: string,
  downloadRootSelector: string,
  hintSelector: string,
  progressSelector: string,
  barSelector: string,
  needsDownload: boolean,
  progress: SileroModelDownloadProgress | null,
): string {
  if (!needsDownload && !progress) {
    return "";
  }

  const percent = downloadPercent(progress);

  if (progress) {
    return `
      <div class="homemaker-silero-download" ${downloadRootSelector}>
        <p class="field-hint" ${hintSelector}>${escapeHtml(formatDownloadProgress(progress))}</p>
        <div
          class="download-progress"
          ${progressSelector}
          role="progressbar"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow="${percent ?? 0}"
        >
          <div
            class="download-progress-bar ${percent === null ? "is-indeterminate" : ""}"
            ${barSelector}
            style="${percent === null ? "" : `width: ${percent}%;`}"
          ></div>
        </div>
      </div>
    `;
  }

  return `
    <div class="homemaker-silero-action" ${actionSelector}>
      <button type="button" class="btn btn-primary btn-compact" ${buttonSelector}>
        ${escapeHtml(t(buttonLabelKey))}
      </button>
    </div>
  `;
}

/** Standard mode: Silero VAD / TE downloads (any storage & text processing mode). */
export function renderHomemakerSileroDownloads(
  diagnostics: Diagnostics | null,
  sileroTeModel: SileroTeModelStatus | null,
  sileroVadModel: SileroVadModelStatus | null,
  sileroTeModelDownload: SileroModelDownloadProgress | null,
  sileroVadModelDownload: SileroModelDownloadProgress | null,
): string {
  const sileroVadOnDisk = sileroVadModel?.exists === true;
  const sileroTeReady = sileroTeModel?.exists === true;
  const sileroVadCompiled = diagnostics?.vad_silero_compiled !== false;
  const sileroTeCompiled =
    diagnostics?.silero_te_compiled === true || (sileroTeModel?.size_mb ?? 0) > 0;

  const vad = renderSileroDownloadBlock(
    "homemaker.downloadImproveRecognition",
    "data-download-silero-vad-model",
    "data-silero-vad-download-action",
    "data-silero-vad-download",
    "data-silero-vad-download-hint",
    "data-silero-vad-download-progress",
    "data-silero-vad-download-bar",
    sileroVadCompiled && !sileroVadOnDisk,
    sileroVadModelDownload,
  );

  const te = renderSileroDownloadBlock(
    "homemaker.downloadImprovePunctuation",
    "data-download-silero-te-model",
    "data-silero-te-download-action",
    "data-silero-te-download",
    "data-silero-te-download-hint",
    "data-silero-te-download-progress",
    "data-silero-te-download-bar",
    sileroTeCompiled && !sileroTeReady,
    sileroTeModelDownload,
  );

  if (!vad && !te) {
    return "";
  }

  return `
    <div class="homemaker-card" data-homemaker-silero>
      <div class="homemaker-silero-actions">
        ${vad}
        ${te}
      </div>
    </div>
  `;
}
