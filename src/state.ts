import { syncSettingsTabUi } from "./components/settings";
import { patchLiveStatusUi } from "./components/status";

import type {
  ActivityLogEntry,
  AiSkillInfo,
  AppSettings,
  DiagnosticsSnapshot,
  ErrorPayload,
  HomemakerLocalSetup,
  StatusSnapshot,
  LlmModelDownloadProgress,
  LlmModelInfo,
  LlmModelStatus,
  TranscriptionLanguageInfo,
  WhisperModelDownloadProgress,
  LocalSttModelInfo,
  WhisperModelStatus,
} from "./api";

export type SettingsTab = "status" | "voice" | "advanced";

export interface UiState {
  status: StatusSnapshot | null;
  settings: AppSettings | null;
  devices: string[];
  diagnostics: DiagnosticsSnapshot | null;
  hasApiKey: boolean;
  lastError: ErrorPayload | null;
  activityLog: ActivityLogEntry[];
  whisperModel: WhisperModelStatus | null;
  whisperModels: LocalSttModelInfo[];
  whisperModelsDir: string;
  dictionaryPath: string;
  aiSkills: AiSkillInfo[];
  whisperModelDownload: WhisperModelDownloadProgress | null;
  llmModel: LlmModelStatus | null;
  llmModels: LlmModelInfo[];
  llmModelsDir: string;
  llmModelDownload: LlmModelDownloadProgress | null;
  loading: boolean;
  activeTab: SettingsTab;
  homemakerLocalSetup: HomemakerLocalSetup | null;
  homemakerHotkeyPresets: string[];
  homemakerConfigLoading: boolean;
  transcriptionLanguages: TranscriptionLanguageInfo[];
  partialTranscript: string | null;
}

type Listener = (state: UiState) => void;

const initialState: UiState = {
  status: null,
  settings: null,
  devices: [],
  diagnostics: null,
  hasApiKey: false,
  lastError: null,
  activityLog: [],
  whisperModel: null,
  whisperModels: [],
  whisperModelsDir: "",
  dictionaryPath: "",
  aiSkills: [],
  whisperModelDownload: null,
  llmModel: null,
  llmModels: [],
  llmModelsDir: "",
  llmModelDownload: null,
  loading: true,
  activeTab: "status",
  homemakerLocalSetup: null,
  homemakerHotkeyPresets: [],
  homemakerConfigLoading: false,
  transcriptionLanguages: [],
  partialTranscript: null,
};

let state: UiState = { ...initialState };
const listeners = new Set<Listener>();

export function getState(): UiState {
  return state;
}

export function subscribe(listener: Listener): () => void {
  listeners.add(listener);
  listener(state);
  return () => listeners.delete(listener);
}

function emit(): void {
  for (const listener of listeners) {
    listener(state);
  }
}

export function patchState(
  partial: Partial<UiState>,
  options?: { render?: boolean },
): void {
  state = { ...state, ...partial };
  if (options?.render === false) {
    return;
  }
  emit();
}

export function setStatus(status: StatusSnapshot): void {
  patchState({ status }, { render: false });
  patchLiveStatusUi(status, state.partialTranscript);
}

export function setSettings(settings: AppSettings): void {
  patchState({ settings });
}

export function setError(lastError: ErrorPayload | null): void {
  patchState({ lastError });
}

export function setActiveTab(activeTab: SettingsTab): void {
  if (state.activeTab === activeTab) {
    return;
  }
  patchState({ activeTab }, { render: false });
  syncSettingsTabUi(activeTab);
}
