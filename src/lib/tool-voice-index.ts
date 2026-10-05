import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  EVENTS,
  getVoiceQueue,
  listVoiceHistory,
  type VoiceFileProgressPhase,
  type VoiceHistoryEntry,
  type VoiceJob,
} from "../api";
import { pathsMatch } from "./tool-file-queue";
import { t } from "../i18n";

export type VoiceIndexEntry = {
  id: string;
  path: string;
  fileName: string;
  status: string;
  text: string;
  errorKey: string | null;
  processing: boolean;
  updatedAtMs: number;
  contentSha256: string | null;
};

export const VOICE_INDEX_SELECTED_STORAGE_KEY = "veyro.voiceIndex.selectedId";
export const VOICE_INDEX_SELECTION_EVENT = "veyro:voice-index-selection";

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

/** Merge persisted history (index) with live queue jobs — real paths + live status. */
export function buildVoiceIndex(
  history: VoiceHistoryEntry[],
  jobs: VoiceJob[],
): VoiceIndexEntry[] {
  const updatedAt = new Map<string, number>();
  for (const entry of history) {
    updatedAt.set(entry.id, entry.updatedAtMs);
  }

  const byId = new Map<string, VoiceIndexEntry>();
  for (const entry of history) {
    byId.set(entry.id, {
      id: entry.id,
      path: entry.path,
      fileName: entry.fileName,
      status: entry.status,
      text: entry.text,
      errorKey: entry.errorKey ?? null,
      processing: entry.status === "processing",
      updatedAtMs: entry.updatedAtMs,
      contentSha256: entry.contentSha256 ?? null,
    });
  }
  for (const job of jobs) {
    const prev = byId.get(job.id);
    byId.set(job.id, {
      id: job.id,
      path: job.path,
      fileName: job.fileName,
      status: job.status,
      text: job.text || prev?.text || "",
      errorKey: job.errorKey ?? prev?.errorKey ?? null,
      processing: job.status === "processing",
      updatedAtMs: Math.max(
        prev?.updatedAtMs ?? 0,
        updatedAt.get(job.id) ?? 0,
      ),
      contentSha256: prev?.contentSha256 ?? null,
    });
  }
  const seenHash = new Set<string>();
  const keptPaths: string[] = [];
  return Array.from(byId.values())
    .sort((a, b) => b.updatedAtMs - a.updatedAtMs)
    .filter((entry) => {
      if (keptPaths.some((path) => pathsMatch(path, entry.path))) {
        return false;
      }
      if (entry.contentSha256) {
        if (seenHash.has(entry.contentSha256)) {
          return false;
        }
        seenHash.add(entry.contentSha256);
      }
      keptPaths.push(entry.path);
      return true;
    });
}

export function voiceIndexStatusLabel(
  status: string,
  phase: VoiceFileProgressPhase | null,
  percent: number | null,
): string {
  switch (status) {
    case "pending":
      return t("tools.voiceFiles.status.pending");
    case "processing": {
      const stage = stageLabel(phase);
      if (percent !== null && percent >= 0) {
        return `${stage} ${percent}%`;
      }
      return stage;
    }
    case "done":
      return t("tools.voiceFiles.status.done");
    case "speech_unrecognized":
      return t("tools.voiceWatch.status.speechUnrecognized");
    case "too_long":
      return t("tools.voiceWatch.status.tooLong");
    case "not_audio":
      return t("tools.voiceWatch.status.notAudio");
    case "skipped":
      return t("tools.voiceWatch.status.skipped");
    case "indexed":
      return t("tools.voiceFiles.status.indexed");
    case "error":
      return t("tools.voiceFiles.status.error");
    default:
      return status;
  }
}

export function getVoiceIndexSelection(): string | null {
  try {
    return localStorage.getItem(VOICE_INDEX_SELECTED_STORAGE_KEY);
  } catch {
    return null;
  }
}

export function setVoiceIndexSelection(id: string | null): void {
  try {
    if (id) {
      localStorage.setItem(VOICE_INDEX_SELECTED_STORAGE_KEY, id);
    } else {
      localStorage.removeItem(VOICE_INDEX_SELECTED_STORAGE_KEY);
    }
  } catch {
    /* ignore */
  }
  window.dispatchEvent(
    new CustomEvent(VOICE_INDEX_SELECTION_EVENT, { detail: id }),
  );
}

export async function fetchVoiceIndex(): Promise<{
  history: VoiceHistoryEntry[];
  jobs: VoiceJob[];
  index: VoiceIndexEntry[];
}> {
  const [history, queue] = await Promise.all([
    listVoiceHistory(),
    getVoiceQueue(),
  ]);
  const jobs = queue.jobs;
  return { history, jobs, index: buildVoiceIndex(history, jobs) };
}

export function subscribeVoiceIndex(
  onChange: () => void,
  onSelection?: (id: string | null) => void,
): () => void {
  const unlisteners: UnlistenFn[] = [];
  const selectionHandler = (event: Event) => {
    const detail = (event as CustomEvent<string | null>).detail ?? null;
    onSelection?.(detail);
  };
  const storageHandler = (event: StorageEvent) => {
    if (event.key === VOICE_INDEX_SELECTED_STORAGE_KEY) {
      onSelection?.(event.newValue);
    }
  };

  window.addEventListener(VOICE_INDEX_SELECTION_EVENT, selectionHandler);
  window.addEventListener("storage", storageHandler);

  void (async () => {
    unlisteners.push(
      await listen(EVENTS.voiceQueueChanged, () => {
        onChange();
      }),
    );
    unlisteners.push(
      await listen(EVENTS.voiceHistoryChanged, () => {
        onChange();
      }),
    );
  })();

  return () => {
    window.removeEventListener(VOICE_INDEX_SELECTION_EVENT, selectionHandler);
    window.removeEventListener("storage", storageHandler);
    for (const unlisten of unlisteners) {
      void unlisten();
    }
  };
}
