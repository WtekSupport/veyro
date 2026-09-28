import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  downloadSeparationModel,
  EVENTS,
  getSeparationModelsStatus,
  getSettings,
  pickVoiceFiles,
  pickVocalSeparatorOutputDir,
  separateVocalFile,
  updateSettings,
  type SeparationModelDownloadProgress,
  type SeparationModelStatus,
  type VocalSeparatorProgressPayload,
  type VocalSeparatorProgressPhase,
  type VocalSeparatorResult,
} from "../api";
import {
  escapeHtml,
  fileNameFromPath,
  pathsMatch,
  runSequentialJobs,
} from "../lib/tool-file-queue";
import { ToolDecodeProgressSmoother } from "../lib/tool-decode-progress";
import {
  formatToolErrorForResultField,
  parseToolErrorRaw,
  toolErrorMessageKey,
} from "../lib/tool-error-display";
import { HF_ACCESS_TOKENS_URL } from "../lib/separation-hf-links";
import {
  modelDownloadProgressPercent,
  patchModelDownloadProgressDom,
  renderModelDownloadProgressBlock,
} from "../lib/model-download-progress";
import { t, type MessageKey } from "../i18n";
import { openUrl } from "@tauri-apps/plugin-opener";

type ModelPanelError =
  | { kind: "hfUnauthorized"; profile: SeparationModelStatus["profile"] }
  | { kind: "text"; message: string };

export type VocalSepJobStatus = "pending" | "processing" | "done" | "error";

export interface VocalSepJob {
  id: string;
  path: string;
  fileName: string;
  status: VocalSepJobStatus;
  processingPhase: VocalSeparatorProgressPhase | null;
  processingPercent: number | null;
  vocalsPath: string;
  instrumentalPath: string;
  warnings: string[];
  errorKey: string | null;
  errorRaw: string | null;
}

function stageLabel(phase: VocalSeparatorProgressPhase | null): string {
  switch (phase) {
    case "decoding":
      return t("tools.vocalSeparator.stage.decoding");
    case "separating":
      return t("tools.vocalSeparator.stage.separating");
    case "writing":
      return t("tools.vocalSeparator.stage.writing");
    case "done":
    case null:
      return t("tools.vocalSeparator.status.processing");
  }
}

function statusLabel(job: VocalSepJob, visiblePercent: number | null): string {
  switch (job.status) {
    case "pending":
      return t("tools.vocalSeparator.status.pending");
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
      return t("tools.vocalSeparator.status.done");
    case "error":
      return t("tools.vocalSeparator.status.error");
  }
}

function separationModelSpecLine(model: SeparationModelStatus): string {
  const modelSize = t("tools.vocalSeparator.modelSpecSize", {
    size: String(model.downloadSizeMb),
  });
  const ram = t("settings.sttSpecRam", {
    size: String(Math.round(model.ramMb / 1024)),
  });
  const vram =
    model.vramMb === null || model.vramMb <= 0
      ? t("settings.sttSpecVramNone")
      : t("settings.sttSpecVramValue", {
          size: String(Math.round(model.vramMb / 1024)),
        });
  return `${modelSize} · ${ram} · ${vram}`;
}

function activeSeparationModel(
  models: SeparationModelStatus[],
  profile: SeparationModelStatus["profile"],
): SeparationModelStatus | undefined {
  return models.find((model) => model.profile === profile);
}

function renderHfAccessHelp(model: SeparationModelStatus): string {
  return `
    <div class="vocal-sep-hf-help" role="alert">
      <p class="field-hint vocal-sep-models-error">${escapeHtml(t("tools.vocalSeparator.hfAccessIntro"))}</p>
      <ol class="vocal-sep-hf-steps">
        <li>${escapeHtml(t("tools.vocalSeparator.hfStepAcceptLicense"))}</li>
        <li>${escapeHtml(t("tools.vocalSeparator.hfStepCreateToken"))}</li>
        <li>${escapeHtml(t("tools.vocalSeparator.hfStepEnvToken"))}</li>
      </ol>
      <div class="vocal-sep-hf-help-actions">
        <button
          type="button"
          class="btn btn-secondary btn-compact"
          data-open-hf-model-page
          data-hf-url="${escapeHtml(model.hfModelPageUrl)}"
        >
          ${escapeHtml(t("tools.vocalSeparator.openHfModelPage"))}
        </button>
        <button type="button" class="btn btn-secondary btn-compact" data-open-hf-tokens>
          ${escapeHtml(t("tools.vocalSeparator.openHfAccessTokens"))}
        </button>
      </div>
    </div>
  `;
}

function renderModelPanelError(
  error: ModelPanelError,
  models: SeparationModelStatus[],
): string {
  if (error.kind === "hfUnauthorized") {
    const model =
      activeSeparationModel(models, error.profile) ??
      models.find((entry) => entry.profile === error.profile);
    if (model) {
      return renderHfAccessHelp(model);
    }
    return `<p class="field-hint vocal-sep-models-error" role="alert">${escapeHtml(
      t("tools.vocalSeparator.downloadUnauthorized"),
    )}</p>`;
  }
  return `<p class="field-hint vocal-sep-models-error" role="alert">${escapeHtml(error.message)}</p>`;
}

function renderModelsPanel(
  models: SeparationModelStatus[],
  activeProfile: SeparationModelStatus["profile"],
  downloadingProfile: string | null,
  modelDownload: SeparationModelDownloadProgress | null,
  downloadError: ModelPanelError | null,
): string {
  const active = activeSeparationModel(models, activeProfile) ?? models[0];
  if (!active) {
    return "";
  }
  const isDownloading = downloadingProfile === active.profile && modelDownload !== null;
  const downloadPercent = isDownloading ? modelDownloadProgressPercent(modelDownload) : null;

  const profileOptions = models
    .map((model) => {
      const key = `tools.vocalSeparator.profile.${model.profile}` as MessageKey;
      return `<option value="${escapeHtml(model.profile)}" ${
        model.profile === activeProfile ? "selected" : ""
      }>${escapeHtml(t(key))}</option>`;
    })
    .join("");

  let downloadBlock = "";
  if (isDownloading && modelDownload) {
    downloadBlock = renderModelDownloadProgressBlock(
      modelDownload,
      downloadPercent,
      `data-separation-model-download="${escapeHtml(active.profile)}"`,
    );
  } else if (!active.exists) {
    downloadBlock = `<button
        type="button"
        class="btn btn-secondary btn-compact"
        data-download-separation-model="${escapeHtml(active.profile)}"
        ${downloadingProfile !== null ? "disabled" : ""}
      >
        ${escapeHtml(t("tools.vocalSeparator.download"))}
      </button>`;
  }

  return `
    <section class="vocal-sep-models" aria-label="${escapeHtml(t("tools.vocalSeparator.modelsTitle"))}">
      <label class="field field-compact vocal-sep-profile-field">
        <span>${escapeHtml(t("tools.vocalSeparator.modelLabel"))}</span>
        <select data-vocal-sep-profile ${downloadingProfile !== null ? "disabled" : ""}>
          ${profileOptions}
        </select>
      </label>
      <p class="vocal-sep-model-spec">${escapeHtml(separationModelSpecLine(active))}</p>
      <div class="vocal-sep-model-actions">
        ${downloadBlock}
      </div>
      ${
        downloadError
          ? renderModelPanelError(downloadError, models)
          : ""
      }
    </section>
  `;
}

function renderQueue(jobs: VocalSepJob[], selectedId: string | null, visiblePercent: number | null): string {
  if (jobs.length === 0) {
    return "";
  }
  return `
    <ul class="voice-files-queue-compact" role="list">
      ${jobs
        .map((job) => {
          const selected = job.id === selectedId;
          return `
            <li>
              <button
                type="button"
                class="voice-files-queue-chip voice-files-queue-chip--${job.status}${selected ? " voice-files-queue-chip--selected" : ""}"
                data-vocal-sep-id="${escapeHtml(job.id)}"
                aria-pressed="${selected}"
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

function stemDownloadIcon(): string {
  return `<svg class="vocal-sep-stem-download-icon" width="18" height="18" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path fill="currentColor" d="M12 3a1 1 0 0 1 1 1v9.59l2.3-2.3a1 1 0 1 1 1.4 1.42l-4 4a1 1 0 0 1-1.4 0l-4-4a1 1 0 1 1 1.4-1.42l2.3 2.3V4a1 1 0 0 1 1-1Zm-7 14a1 1 0 0 1 1 1v2h12v-2a1 1 0 1 1 2 0v3a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-3a1 1 0 0 1 1-1Z"/></svg>`;
}

function renderStemPlayerRow(path: string, label: string): string {
  if (!path.trim()) {
    return "";
  }
  const src = convertFileSrc(path);
  const fileName = fileNameFromPath(path);
  const downloadLabel = t("tools.vocalSeparator.downloadStem");
  return `
    <div class="vocal-sep-stem-row">
      <span class="vocal-sep-stem-label">${escapeHtml(label)}</span>
      <audio class="vocal-sep-stem-audio" controls preload="metadata" src="${escapeHtml(src)}"></audio>
      <a
        class="vocal-sep-stem-download btn btn-secondary btn-icon"
        href="${escapeHtml(src)}"
        download="${escapeHtml(fileName)}"
        title="${escapeHtml(downloadLabel)}"
        aria-label="${escapeHtml(downloadLabel)}"
      >${stemDownloadIcon()}</a>
    </div>
  `;
}

function renderResultPanel(selected: VocalSepJob | null, resultText: string): string {
  if (selected?.status === "done") {
    const warnings =
      selected.warnings.length > 0
        ? `<ul class="vocal-sep-result-warnings">${selected.warnings
            .map(
              (key) =>
                `<li>${escapeHtml(t(key as MessageKey))}</li>`,
            )
            .join("")}</ul>`
        : "";
    return `
      <div class="vocal-sep-result-panel" data-vocal-sep-result>
        ${renderStemPlayerRow(selected.vocalsPath, t("tools.vocalSeparator.outputVocals"))}
        ${renderStemPlayerRow(selected.instrumentalPath, t("tools.vocalSeparator.outputInstrumental"))}
        ${warnings}
      </div>
    `;
  }
  if (!resultText) {
    return `<div class="vocal-sep-result-panel vocal-sep-result-panel--empty" data-vocal-sep-result></div>`;
  }
  return `
    <div class="vocal-sep-result-panel vocal-sep-result-panel--message" data-vocal-sep-result role="status">
      <p class="vocal-sep-result-message">${escapeHtml(resultText)}</p>
    </div>
  `;
}

function renderTool(
  jobs: VocalSepJob[],
  selectedId: string | null,
  models: SeparationModelStatus[],
  activeProfile: SeparationModelStatus["profile"],
  downloadingProfile: string | null,
  modelDownload: SeparationModelDownloadProgress | null,
  modelDownloadError: ModelPanelError | null,
  visiblePercent: number | null,
): string {
  const selected =
    jobs.find((job) => job.id === selectedId) ??
    (jobs.length > 0 ? jobs[jobs.length - 1] : null);
  let resultText = "";
  if (selected?.status === "error" && selected.errorRaw) {
    resultText = formatToolErrorForResultField(selected.errorRaw, "tools.vocalSeparator.failed");
  } else if (selected?.status === "processing") {
    resultText = stageLabel(selected.processingPhase);
  }

  const processing = jobs.some((job) => job.status === "processing");
  const progressPercent =
    selected?.status === "processing" && selected.processingPhase !== "done"
      ? (visiblePercent ?? selected.processingPercent)
      : null;

  return `
    <main class="voice-files-tool vocal-sep-tool">
      ${renderModelsPanel(
        models,
        activeProfile,
        downloadingProfile,
        modelDownload,
        modelDownloadError,
      )}
      <div
        class="voice-files-dropzone${processing ? " voice-files-dropzone--busy" : ""}"
        data-vocal-sep-dropzone
        tabindex="0"
      >
        <p class="voice-files-dropzone-text">${escapeHtml(t("tools.vocalSeparator.dropHint"))}</p>
        <button type="button" class="btn btn-secondary btn-compact" data-pick-vocal-sep ${processing ? "disabled" : ""}>
          ${escapeHtml(t("tools.vocalSeparator.pickFiles"))}
        </button>
        ${renderQueue(jobs, selectedId, visiblePercent)}
      </div>
      ${
        progressPercent !== null
          ? `<div class="voice-files-progress" role="progressbar" aria-valuenow="${progressPercent}">
              <div class="voice-files-progress-fill" style="width: ${progressPercent}%"></div>
            </div>`
          : ""
      }
      ${renderResultPanel(selected, resultText)}
    </main>
  `;
}

let jobCounter = 0;

function nextJobId(): string {
  jobCounter += 1;
  return `vs-${jobCounter}`;
}

export function createVocalSeparatorController(root: HTMLElement): {
  enqueuePaths: (paths: string[]) => void;
  dispose: () => void;
} {
  let jobs: VocalSepJob[] = [];
  let selectedId: string | null = null;
  let models: SeparationModelStatus[] = [];
  let activeProfile: SeparationModelStatus["profile"] = "quality";
  let downloadingProfile: string | null = null;
  let modelDownload: SeparationModelDownloadProgress | null = null;
  let modelDownloadError: ModelPanelError | null = null;
  let queueRunning = false;
  let progressUnlisten: UnlistenFn | null = null;
  let downloadUnlisten: UnlistenFn | null = null;
  const decodeProgress = new ToolDecodeProgressSmoother();

  const persistProfile = async (
    profile: SeparationModelStatus["profile"],
  ): Promise<void> => {
    try {
      await updateSettings({ vocal_separator_profile: profile });
    } catch {
      // Keep local selection if settings update fails.
    }
  };

  const reconcileActiveProfile = (): void => {
    if (activeSeparationModel(models, activeProfile)?.exists) {
      return;
    }
    const ready = models.find((model) => model.exists);
    if (ready) {
      activeProfile = ready.profile;
      void persistProfile(activeProfile);
    }
  };

  const refreshModels = async (): Promise<void> => {
    try {
      models = await getSeparationModelsStatus();
      const selected = models.find((model) => model.selected);
      if (selected) {
        activeProfile = selected.profile;
      }
      reconcileActiveProfile();
    } catch {
      models = [];
    }
    paint();
  };

  const onProgress = (payload: VocalSeparatorProgressPayload): void => {
    jobs = jobs.map((job) => {
      if (!pathsMatch(job.path, payload.path)) {
        return job;
      }
      if (job.status !== "processing" && payload.phase !== "done") {
        return job;
      }
      if (payload.phase === "done" && payload.vocalsPath && payload.instrumentalPath) {
        return {
          ...job,
          status: "done",
          processingPhase: "done",
          processingPercent: 100,
          vocalsPath: payload.vocalsPath,
          instrumentalPath: payload.instrumentalPath,
        };
      }
      if (job.status !== "processing") {
        return job;
      }
      return {
        ...job,
        processingPhase: payload.phase,
        processingPercent:
          payload.percent !== undefined && Number.isFinite(payload.percent)
            ? Math.min(100, Math.max(0, Math.round(payload.percent)))
            : job.processingPercent,
      };
    });
    const active = jobs.find(
      (job) => job.status === "processing" && pathsMatch(job.path, payload.path),
    );
    if (payload.phase === "done" || !active) {
      decodeProgress.stop();
    } else {
      decodeProgress.sync(active.processingPhase ?? null, active.processingPercent ?? null);
    }
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
    root.innerHTML = renderTool(
      jobs,
      selectedId,
      models,
      activeProfile,
      downloadingProfile,
      modelDownload,
      modelDownloadError,
      visible,
    );
    bind();
  };

  decodeProgress.bindRepaint(paint);

  const bind = (): void => {
    root.querySelector<HTMLButtonElement>("[data-pick-vocal-sep]")?.addEventListener("click", () => {
      void pickAndEnqueue();
    });
    root.querySelectorAll<HTMLButtonElement>("[data-vocal-sep-id]").forEach((button) => {
      button.addEventListener("click", () => {
        selectedId = button.dataset.vocalSepId ?? null;
        paint();
      });
    });
    root.querySelector<HTMLSelectElement>("[data-vocal-sep-profile]")?.addEventListener("change", (event) => {
      const select = event.currentTarget as HTMLSelectElement;
      const profile = select.value as SeparationModelStatus["profile"];
      if (profile !== "quality" && profile !== "fast" && profile !== "legacy") {
        return;
      }
      activeProfile = profile;
      modelDownloadError = null;
      void persistProfile(activeProfile);
      paint();
    });
    root.querySelectorAll<HTMLButtonElement>("[data-download-separation-model]").forEach((button) => {
      button.addEventListener("click", () => {
        const profile = button.dataset.downloadSeparationModel;
        if (!profile) {
          return;
        }
        void downloadModel(profile);
      });
    });
    root.querySelector<HTMLButtonElement>("[data-open-hf-model-page]")?.addEventListener("click", (event) => {
      const button = event.currentTarget as HTMLButtonElement;
      const url = button.dataset.hfUrl?.trim();
      if (url) {
        void openUrl(url);
      }
    });
    root.querySelector<HTMLButtonElement>("[data-open-hf-tokens]")?.addEventListener("click", () => {
      void openUrl(HF_ACCESS_TOKENS_URL);
    });
    root.querySelector<HTMLButtonElement>("[data-pick-output-dir]")?.addEventListener("click", () => {
      void pickVocalSeparatorOutputDir();
    });
  };

  const downloadModel = async (profile: string): Promise<void> => {
    downloadingProfile = profile;
    modelDownloadError = null;
    modelDownload = {
      profile: profile as SeparationModelDownloadProgress["profile"],
      downloaded: 0,
      total: null,
      percent: null,
    };
    paint();
    try {
      await downloadSeparationModel(profile as SeparationModelStatus["profile"]);
      activeProfile = profile as SeparationModelStatus["profile"];
      await persistProfile(activeProfile);
      await refreshModels();
    } catch (error) {
      const raw = error instanceof Error ? error.message : String(error);
      const parsed = parseToolErrorRaw(raw);
      if (parsed.key === "tools.vocalSeparator.downloadUnauthorized") {
        modelDownloadError = {
          kind: "hfUnauthorized",
          profile: profile as SeparationModelStatus["profile"],
        };
      } else {
        modelDownloadError = {
          kind: "text",
          message: formatToolErrorForResultField(raw, "tools.vocalSeparator.downloadFailed"),
        };
      }
      console.error("download separation model", error);
    } finally {
      downloadingProfile = null;
      modelDownload = null;
      paint();
    }
  };

  const processJob = async (job: VocalSepJob): Promise<void> => {
    const model = activeSeparationModel(models, activeProfile);
    if (!model?.exists) {
      jobs = jobs.map((entry) =>
        entry.id === job.id
          ? {
              ...entry,
              status: "error",
              errorRaw: "tools.vocalSeparator.modelMissing",
              errorKey: "tools.vocalSeparator.modelMissing",
            }
          : entry,
      );
      paint();
      return;
    }

    jobs = jobs.map((entry) =>
      entry.id === job.id
        ? { ...entry, status: "processing", processingPhase: "decoding", processingPercent: 0 }
        : entry,
    );
    paint();
    try {
      const result: VocalSeparatorResult = await separateVocalFile(job.path, {
        profile: activeProfile,
      });
      jobs = jobs.map((entry) => {
        if (entry.id !== job.id) {
          return entry;
        }
        // Prefer paths already applied by the Done progress event.
        if (entry.status === "done" && entry.vocalsPath && entry.instrumentalPath) {
          return {
            ...entry,
            warnings: result.warnings ?? entry.warnings,
          };
        }
        return {
          ...entry,
          status: "done",
          processingPhase: "done",
          processingPercent: 100,
          vocalsPath: result.vocalsPath,
          instrumentalPath: result.instrumentalPath,
          warnings: result.warnings ?? [],
        };
      });
      decodeProgress.stop();
    } catch (error) {
      const raw = error instanceof Error ? error.message : String(error);
      jobs = jobs.map((entry) =>
        entry.id === job.id && entry.status !== "done"
          ? {
              ...entry,
              status: "error",
              errorRaw: raw,
              errorKey: toolErrorMessageKey(raw, "tools.vocalSeparator.failed"),
            }
          : entry,
      );
      decodeProgress.stop();
    }
    paint();
  };

  const drainQueue = async (): Promise<void> => {
    if (queueRunning) {
      return;
    }
    queueRunning = true;
    try {
      await runSequentialJobs(
        jobs.filter((job) => job.status === "pending"),
        processJob,
      );
    } finally {
      queueRunning = false;
    }
  };

  const enqueuePaths = (paths: string[]): void => {
    const normalized = paths
      .map((path) => path.trim())
      .filter((path) => path.length > 0);

    let nextJobs = [...jobs];
    let lastNewId: string | null = null;

    for (const path of normalized) {
      const existing = nextJobs.find((job) => pathsMatch(job.path, path));
      if (existing) {
        if (existing.status === "pending" || existing.status === "processing") {
          continue;
        }
        Object.assign(existing, {
          status: "pending" as const,
          processingPhase: null,
          processingPercent: null,
          vocalsPath: "",
          instrumentalPath: "",
          warnings: [],
          errorKey: null,
          errorRaw: null,
        });
        lastNewId = existing.id;
        continue;
      }
      const id = nextJobId();
      nextJobs.push({
        id,
        path,
        fileName: fileNameFromPath(path),
        status: "pending",
        processingPhase: null,
        processingPercent: null,
        vocalsPath: "",
        instrumentalPath: "",
        warnings: [],
        errorKey: null,
        errorRaw: null,
      });
      lastNewId = id;
    }

    if (lastNewId === null) {
      return;
    }
    const model = activeSeparationModel(models, activeProfile);
    if (!model?.exists) {
      modelDownloadError = {
        kind: "text",
        message: t("tools.vocalSeparator.modelMissing"),
      };
    }
    jobs = nextJobs;
    selectedId = lastNewId ?? selectedId;
    paint();
    void drainQueue();
  };

  const pickAndEnqueue = async (): Promise<void> => {
    const paths = await pickVoiceFiles();
    if (paths.length > 0) {
      enqueuePaths(paths);
    }
  };

  void listen(EVENTS.vocalSeparatorProgress, (event) => {
    onProgress(event.payload as VocalSeparatorProgressPayload);
  }).then((unlisten) => {
    progressUnlisten = unlisten;
  });

  void listen(EVENTS.separationModelDownloadProgress, (event) => {
    const payload = event.payload as SeparationModelDownloadProgress;
    if (downloadingProfile !== payload.profile) {
      return;
    }
    modelDownload = payload;
    patchModelDownloadProgressDom(
      root,
      payload,
      `[data-separation-model-download="${payload.profile}"]`,
    );
  }).then((unlisten) => {
    downloadUnlisten = unlisten;
  });

  void (async () => {
    try {
      const settings = await getSettings();
      if (settings.vocal_separator_profile) {
        activeProfile = settings.vocal_separator_profile;
      }
    } catch {
      // Defaults remain.
    }
    await refreshModels();
  })();

  return {
    enqueuePaths,
    dispose: () => {
      void progressUnlisten?.();
      void downloadUnlisten?.();
      decodeProgress.dispose();
    },
  };
}
