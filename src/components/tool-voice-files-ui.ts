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
  rewriteFallback: boolean;
  aiRewriteApplied: boolean;
  gecGrammarOnly: boolean;
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

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function fileNameFromPath(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  const segment = normalized.split("/").pop();
  return segment ?? path;
}

function pathsMatch(left: string, right: string): boolean {
  return left.replace(/\\/g, "/").toLowerCase() === right.replace(/\\/g, "/").toLowerCase();
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

function statusLabel(job: VoiceFileJob): string {
  switch (job.status) {
    case "pending":
      return t("tools.voiceFiles.status.pending");
    case "processing": {
      const stage = stageLabel(job.processingPhase);
      if (job.processingPercent !== null && job.processingPercent >= 0) {
        return `${stage} ${job.processingPercent}%`;
      }
      return stage;
    }
    case "done":
      return t("tools.voiceFiles.status.done");
    case "error":
      return t("tools.voiceFiles.status.error");
  }
}

function renderQueueCompact(jobs: VoiceFileJob[], selectedId: string | null): string {
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
                <span class="voice-files-queue-chip-status">${escapeHtml(statusLabel(job))}</span>
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
  } else if (selected?.status === "done" && !resultText.trim()) {
    displayText = t("tools.voiceFiles.emptyResult");
  }
  const processing = jobs.some((job) => job.status === "processing");
  const progressPercent =
    selected?.status === "processing" && selected.processingPercent !== null
      ? selected.processingPercent
      : null;
  const canCopy = Boolean(resultText.trim());
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
        ${renderQueueCompact(jobs, selectedId)}
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
            class="voice-files-textarea"
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
        processingPercent = null;
      }
      return {
        ...job,
        processingPhase: payload.phase,
        processingPercent,
      };
    });
    paint();
  };

  const paint = (): void => {
    root.innerHTML = renderVoiceFilesTool(jobs, selectedId, copyHint);
    bind();
  };

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
    const selected =
      jobs.find((job) => job.id === selectedId) ??
      (jobs.length > 0 ? jobs[jobs.length - 1] : undefined);
    if (!selected?.text.trim()) {
      return;
    }
    try {
      await copyTextToClipboard(selected.text);
      copyHint = t("tools.voiceFiles.copied");
    } catch {
      try {
        await navigator.clipboard.writeText(selected.text);
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
        rewriteFallback: false,
        aiRewriteApplied: false,
        gecGrammarOnly: false,
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
          }
        : job,
    );
    selectedId = id;
  };

  const applyError = (id: string, errorKey: string): void => {
    jobs = jobs.map((job) =>
      job.id === id
        ? {
            ...job,
            status: "error",
            errorKey,
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
        paint();

        try {
          const result = await transcribeVoiceFile(next.path);
          applyResult(next.id, result);
        } catch (error) {
          const raw = error instanceof Error ? error.message : String(error);
          const key = raw.startsWith("tools.") ? raw : "tools.voiceFiles.failed";
          applyError(next.id, key);
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
    },
  };
}
