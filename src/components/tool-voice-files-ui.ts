import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  copyTextToClipboard,
  EVENTS,
  pickVoiceFiles,
  transcribeVoiceFile,
  type VoiceFileProgressPayload,
  type VoiceFileProgressPhase,
  type VoiceFileTranscriptionResult,
} from "../api";
import { iconCopy } from "./icons";
import { ToolDecodeProgressSmoother } from "../lib/tool-decode-progress";
import {
  formatToolErrorForResultField,
  toolErrorMessageKey,
} from "../lib/tool-error-display";
import { escapeHtml, fileNameFromPath, pathsMatch } from "../lib/tool-file-queue";
import { runToolWithSttLanguageRecovery } from "../lib/tool-stt-auto-recovery";
import { t, type MessageKey } from "../i18n";

export type VoiceFileJobStatus = "pending" | "processing" | "done" | "error";

export interface VoiceFileJob {
  id: string;
  path: string;
  fileName: string;
  status: VoiceFileJobStatus;
  processingPhase: VoiceFileProgressPhase | null;
  processingPercent: number | null;
  text: string;
  errorKey: string | null;
  /** Full invoke error (may include `key|technical` suffix). */
  errorRaw: string | null;
  rewriteFallback: boolean;
  aiRewriteApplied: boolean;
  gecGrammarOnly: boolean;
  infoMessageKey: string | null;
}

function resultFootnote(job: VoiceFileJob | null | undefined): string {
  if (!job || job.status !== "done") {
    return "";
  }
  if (job.rewriteFallback) {
    return t("tools.voiceFiles.rewriteFallback");
  }
  if (job.gecGrammarOnly) {
    return t("tools.voiceFiles.gecOptimizationHint");
  }
  if (job.aiRewriteApplied) {
    return t("tools.voiceFiles.aiRewriteDone");
  }
  return "";
}

function stageLabel(phase: VoiceFileProgressPhase | null): string {
  switch (phase) {
    case "decoding":
      return t("tools.voiceFiles.stage.decoding");
    case "transcribing":
      return t("tools.voiceFiles.stage.transcribing");
    case "text_cleanup":
      return t("tools.voiceFiles.stage.textCleanup");
    case "ai_rewrite":
      return t("tools.voiceFiles.stage.aiRewrite");
    case "done":
    case null:
      return t("tools.voiceFiles.status.processing");
  }
}

function statusLabel(job: VoiceFileJob, visiblePercent: number | null): string {
  switch (job.status) {
    case "pending":
      return t("tools.voiceFiles.status.pending");
    case "processing": {
      const stage = stageLabel(job.processingPhase);
      const percent =
        visiblePercent !== null && job.processingPhase === "decoding"
          ? visiblePercent
          : job.processingPercent;
      if (percent !== null && percent >= 0) {
        return `${stage} ${percent}%`;
      }
      return stage;
    }
    case "done":
      return t("tools.voiceFiles.status.done");
    case "error":
      return t("tools.voiceFiles.status.error");
  }
}

function renderQueueCompact(
  jobs: VoiceFileJob[],
  selectedId: string | null,
  visiblePercent: number | null,
): string {
  if (jobs.length === 0) {
    return "";
  }
  return `
    <ul class="voice-files-queue-compact" role="list">
      ${jobs
        .map((job) => {
          const selected = job.id === selectedId;
          const errorText =
            job.errorKey !== null
              ? escapeHtml(t(job.errorKey as MessageKey))
              : "";
          return `
            <li>
              <button
                type="button"
                class="voice-files-queue-chip voice-files-queue-chip--${job.status}${selected ? " voice-files-queue-chip--selected" : ""}"
                data-voice-file-id="${escapeHtml(job.id)}"
                aria-pressed="${selected}"
                title="${errorText || escapeHtml(job.fileName)}"
              >
                <span class="voice-files-queue-chip-name">${escapeHtml(job.fileName)}</span>
                <span class="voice-files-queue-chip-status">${escapeHtml(statusLabel(job, visiblePercent))}</span>
              </button>
            </li>
          `;
        })
        .join("")}
    </ul>
  `;
}

export function renderVoiceFilesTool(
  jobs: VoiceFileJob[],
  selectedId: string | null,
  copyHint: string | null,
  visibleProgressPercent: number | null = null,
): string {
  const selected =
    jobs.find((job) => job.id === selectedId) ??
    (jobs.length > 0 ? jobs[jobs.length - 1] : null);
  const resultText = selected?.status === "done" ? selected.text : "";
  let displayText = resultText;
  if (selected?.status === "processing") {
    displayText = `${stageLabel(selected.processingPhase)} — ${selected.fileName}`;
  } else if (selected?.status === "pending") {
    displayText = `${t("tools.voiceFiles.status.pending")} ${selected.fileName}`;
  } else if (
    selected?.status === "done" &&
    !resultText.trim() &&
    selected.infoMessageKey
  ) {
    displayText = formatToolErrorForResultField(
      selected.infoMessageKey,
      "tools.voiceFiles.emptyResult",
    );
  } else if (selected?.status === "done" && !resultText.trim()) {
    displayText = formatToolErrorForResultField(
      "tools.voiceFiles.emptyResult",
      "tools.voiceFiles.emptyResult",
    );
  } else if (selected?.status === "error" && selected.errorRaw) {
    displayText = formatToolErrorForResultField(
      selected.errorRaw,
      "tools.voiceFiles.failed",
    );
  } else if (selected?.status === "error") {
    displayText = formatToolErrorForResultField(
      selected.errorKey ?? "tools.voiceFiles.failed",
      "tools.voiceFiles.failed",
    );
  }
  const processing = jobs.some((job) => job.status === "processing");
  const progressPercent =
    selected?.status === "processing"
      ? (visibleProgressPercent ?? selected.processingPercent)
      : null;
  const canCopy = Boolean(displayText.trim()) && selected?.status !== "pending";
  const copyTitle = copyHint ?? t("tools.voiceFiles.copy");
  const footnote = resultFootnote(selected);
  const footnoteClass = selected?.rewriteFallback
    ? "voice-files-fallback-hint"
    : "voice-files-stage-hint";

  return `
    <main class="voice-files-tool">
      <div
        class="voice-files-dropzone${processing ? " voice-files-dropzone--busy" : ""}"
        data-voice-files-dropzone
        tabindex="0"
        role="region"
        aria-label="${escapeHtml(t("tools.voiceFiles.dropHint"))}"
      >
        <p class="voice-files-dropzone-text">${escapeHtml(t("tools.voiceFiles.dropHint"))}</p>
        <button type="button" class="btn btn-secondary btn-compact" data-pick-voice-files ${processing ? "disabled" : ""}>
          ${escapeHtml(t("tools.voiceFiles.pickFiles"))}
        </button>
        ${renderQueueCompact(jobs, selectedId, visibleProgressPercent)}
      </div>

      ${
        progressPercent !== null
          ? `
        <div
          class="voice-files-progress"
          role="progressbar"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow="${progressPercent}"
          aria-label="${escapeHtml(stageLabel(selected?.processingPhase ?? null))}"
        >
          <div class="voice-files-progress-fill" style="width: ${progressPercent}%"></div>
        </div>
      `
          : ""
      }

      <div class="voice-files-text-stack" aria-live="polite">
        <div class="voice-files-text-wrap">
          <button
            type="button"
            class="voice-files-copy-overlay icon-btn"
            data-copy-voice-result
            ${canCopy ? "" : "disabled"}
            aria-label="${escapeHtml(copyTitle)}"
            title="${escapeHtml(copyTitle)}"
          >${iconCopy()}</button>
          <textarea
            class="voice-files-textarea${selected?.status === "error" ? " voice-files-textarea--error" : selected?.status === "done" && !resultText.trim() ? " voice-files-textarea--info" : ""}"
            readonly
            data-voice-result-text
            aria-label="${escapeHtml(t("tools.voiceFiles.resultTitle"))}"
          >${escapeHtml(displayText)}</textarea>
        </div>
        ${footnote ? `<p class="${footnoteClass}">${escapeHtml(footnote)}</p>` : ""}
      </div>
    </main>
  `;
}

let jobCounter = 0;

function nextJobId(): string {
  jobCounter += 1;
  return `vf-${jobCounter}`;
}

export function createVoiceFilesController(root: HTMLElement): {
  enqueuePaths: (paths: string[]) => void;
  dispose: () => void;
} {
  let jobs: VoiceFileJob[] = [];
  let selectedId: string | null = null;
  let copyHint: string | null = null;
  let queueRunning = false;
  let progressUnlisten: UnlistenFn | null = null;
  const decodeProgress = new ToolDecodeProgressSmoother();

  const onProgress = (payload: VoiceFileProgressPayload): void => {
    jobs = jobs.map((job) => {
      if (job.status !== "processing" || !pathsMatch(job.path, payload.path)) {
        return job;
      }
      const phaseChanged = payload.phase !== job.processingPhase;
      let processingPercent = job.processingPercent;
      if (payload.percent !== undefined) {
        processingPercent =
          typeof payload.percent === "number" && Number.isFinite(payload.percent)
            ? Math.min(100, Math.max(0, Math.round(payload.percent)))
            : null;
      } else if (phaseChanged) {
        processingPercent = 0;
      }
      return {
        ...job,
        processingPhase: payload.phase,
        processingPercent,
      };
    });
    const active = jobs.find(
      (job) => job.status === "processing" && pathsMatch(job.path, payload.path),
    );
    decodeProgress.sync(
      active?.processingPhase ?? null,
      active?.processingPercent ?? null,
    );
    paint();
  };

  const paint = (): void => {
    const selected =
      jobs.find((job) => job.id === selectedId) ??
      (jobs.length > 0 ? jobs[jobs.length - 1] : undefined);
    const visible = decodeProgress.displayPercent(
      selected?.processingPhase ?? null,
      selected?.processingPercent ?? null,
    );
    root.innerHTML = renderVoiceFilesTool(jobs, selectedId, copyHint, visible);
    bind();
  };

  decodeProgress.bindRepaint(paint);

  const bind = (): void => {
    root.querySelector<HTMLButtonElement>("[data-pick-voice-files]")?.addEventListener("click", () => {
      void pickAndEnqueue();
    });

    root.querySelector<HTMLButtonElement>("[data-copy-voice-result]")?.addEventListener("click", () => {
      void copyResult();
    });

    root.querySelectorAll<HTMLButtonElement>("[data-voice-file-id]").forEach((button) => {
      button.addEventListener("click", () => {
        selectedId = button.dataset.voiceFileId ?? null;
        copyHint = null;
        paint();
      });
    });

    const dropzone = root.querySelector<HTMLElement>("[data-voice-files-dropzone]");
    dropzone?.addEventListener("dragover", (event) => {
      event.preventDefault();
      dropzone.classList.add("voice-files-dropzone--hover");
    });
    dropzone?.addEventListener("dragleave", () => {
      dropzone.classList.remove("voice-files-dropzone--hover");
    });
    dropzone?.addEventListener("drop", (event) => {
      event.preventDefault();
      dropzone.classList.remove("voice-files-dropzone--hover");
      const transfer = event.dataTransfer;
      if (!transfer?.files?.length) {
        return;
      }
      const paths: string[] = [];
      for (const file of Array.from(transfer.files)) {
        const withPath = file as File & { path?: string };
        if (withPath.path) {
          paths.push(withPath.path);
        }
      }
      if (paths.length > 0) {
        enqueuePaths(paths);
      }
    });
  };

  const pickAndEnqueue = async (): Promise<void> => {
    const paths = await pickVoiceFiles();
    if (paths.length > 0) {
      enqueuePaths(paths);
    }
  };

  const copyResult = async (): Promise<void> => {
    const textarea = root.querySelector<HTMLTextAreaElement>("[data-voice-result-text]");
    const fromDom = textarea?.value.trim() ?? "";
    const selected =
      jobs.find((job) => job.id === selectedId) ??
      (jobs.length > 0 ? jobs[jobs.length - 1] : undefined);
    const payload =
      fromDom ||
      (selected?.status === "done" ? selected.text.trim() : "") ||
      (selected?.status === "error" && selected.errorRaw
        ? formatToolErrorForResultField(selected.errorRaw, "tools.voiceFiles.failed")
        : "");
    if (!payload) {
      return;
    }
    try {
      await copyTextToClipboard(payload);
      copyHint = t("tools.voiceFiles.copied");
    } catch {
      try {
        await navigator.clipboard.writeText(payload);
        copyHint = t("tools.voiceFiles.copied");
      } catch {
        copyHint = t("tools.voiceFiles.copyFailed");
      }
    }
    paint();
    window.setTimeout(() => {
      copyHint = null;
      paint();
    }, 1600);
  };

  const enqueuePaths = (paths: string[]): void => {
    const unique = paths.filter((path) => path.trim().length > 0);
    if (unique.length === 0) {
      return;
    }
    for (const path of unique) {
      const id = nextJobId();
      jobs.push({
        id,
        path,
        fileName: fileNameFromPath(path),
        status: "pending",
        processingPhase: null,
        processingPercent: null,
        text: "",
        errorKey: null,
        errorRaw: null,
        rewriteFallback: false,
        aiRewriteApplied: false,
        gecGrammarOnly: false,
        infoMessageKey: null,
      });
      if (!selectedId) {
        selectedId = id;
      }
    }
    paint();
    void runQueue();
  };

  const applyResult = (id: string, result: VoiceFileTranscriptionResult): void => {
    jobs = jobs.map((job) =>
      job.id === id
        ? {
            ...job,
            status: "done",
            text: result.text,
            rewriteFallback: result.rewriteFallback,
            aiRewriteApplied: result.aiRewriteApplied,
            gecGrammarOnly: result.gecGrammarOnly,
            processingPhase: null,
            processingPercent: null,
            fileName: result.fileName || job.fileName,
            infoMessageKey: result.infoMessageKey ?? null,
          }
        : job,
    );
    selectedId = id;
  };

  const applyError = (id: string, errorRaw: string): void => {
    const errorKey = toolErrorMessageKey(errorRaw, "tools.voiceFiles.failed");
    jobs = jobs.map((job) =>
      job.id === id
        ? {
            ...job,
            status: "error",
            errorKey,
            errorRaw,
            processingPhase: null,
            processingPercent: null,
          }
        : job,
    );
    selectedId = id;
  };

  const runQueue = async (): Promise<void> => {
    if (queueRunning) {
      return;
    }
    queueRunning = true;
    try {
      while (true) {
        const next = jobs.find((job) => job.status === "pending");
        if (!next) {
          break;
        }
        jobs = jobs.map((job) =>
          job.id === next.id
            ? {
                ...job,
                status: "processing",
                processingPhase: "decoding",
                processingPercent: 0,
              }
            : job,
        );
        selectedId = next.id;
        copyHint = null;
        decodeProgress.reset();
        paint();

        try {
          const result = await runToolWithSttLanguageRecovery(() =>
            transcribeVoiceFile(next.path),
          );
          applyResult(next.id, result);
        } catch (error) {
          const raw = error instanceof Error ? error.message : String(error);
          applyError(next.id, raw.trim() || "tools.voiceFiles.failed");
        }
        paint();
      }
    } finally {
      queueRunning = false;
    }
  };

  void listen<VoiceFileProgressPayload>(EVENTS.voiceFileProgress, (event) => {
    onProgress(event.payload);
  }).then((unlisten) => {
    progressUnlisten = unlisten;
  });

  paint();

  return {
    enqueuePaths,
    dispose: (): void => {
      void progressUnlisten?.();
      progressUnlisten = null;
      decodeProgress.dispose();
    },
  };
}
