import { openUrl } from "@tauri-apps/plugin-opener";

import logoUrl from "./assets/logo.png";

import {
  clearActivityLog,
  clearApiKey,
  EVENTS,
  getActivityLog,
  getDevices,
  getDiagnostics,
  getHomemakerHotkeyPresets,
  getHomemakerLocalSetup,
  prewarmMicrophone,
  prewarmLocalModels,
  setApiKey,
  downloadLlmModel,
  downloadLocalSttModel,
  getLlmModelsDir,
  getLlmModelStatus,
  getDictionaryPath,
  openAboutWindow,
  getWhisperModelsDir,
  listAiSkills,
  listLlmModels,
  listTranscriptionLanguages,
  listLocalSttModels,
  openAiSkillsFolder,
  openTranscriptionDictionaryFolder,
  pickAndImportAiSkill,
  pickLlmModelsDir,
  pickTranscriptionDictionary,
  pickWhisperModelsDir,
  getSettings,
  getStatus,
  getWhisperModelStatus,
  recoverEngine,
  updateSettings,
  hasApiKey,
  type AppSettings,
  type SettingsPatch,
  type LlmModelKind,
  type TextProcessingMode,
  type LocalSttModelKind,
  subscribe,
  type ActivityLogEntry,
  type StatusSnapshot,
  type TranscriptionCompletedPayload,
} from "./api";
import {
  ensureSpacesAfterPunctuation,
  mergeTranscriptChunks,
} from "./lib/text-spacing";
import { bindHotkeyInputs } from "./components/hotkey-input";
import { showConfirmDialog } from "./components/confirm-dialog";
import { FALLBACK_HOMEMAKER_HOTKEY_PRESETS } from "./components/homemaker-hotkeys";
import {
  effectiveDataStorage,
  isHomemakerMode,
  readHomemakerForm,
  renderHomemakerSettings,
} from "./components/homemaker-settings";
import { renderHomemakerConfigBanner } from "./components/homemaker-loading-banner";
import { mountRotatingTagline } from "./components/rotating-tagline";
import { startMicLevelMonitor } from "./components/mic-meter";
import {
  bindVadThresholdPanel,
  startVadThresholdMonitor,
} from "./components/vad-threshold-panel";
import {
  AI_TEXT_MODES,
  renderSettingsForm,
  renderTabBar,
  settingsToForm,
  textModeHintKey,
  updateLlmDownloadUi,
  updateWhisperDownloadUi,
} from "./components/settings";
import {
  bindStandardOnboardingBanner,
  renderStandardOnboardingBanner,
} from "./components/first-run-hint";
import {
  mountUpdateBannerSlot,
  runStartupUpdateCheck,
} from "./components/update-banner";
import {
  patchLiveStatusUi,
  renderCompactDiagnostics,
  renderCompactStatusBar,
  renderErrorBanner,
  renderStatusBar,
  renderUsageHint,
  updateActivityLogDom,
} from "./components/status";
import { bindUiModeSwitch, renderUiModeLink } from "./components/ui-mode-switch";
import { getLocale, setLocale, subscribeLocale, t } from "./i18n";
import { skillCatalogUrl } from "./lib/skill-catalog-url";
import {
  flushPersistSettings,
  schedulePersistSettings,
} from "./settings-persist";
import {
  getState,
  patchState,
  setActiveTab,
  setError,
  setSettings,
  setStatus,
  subscribe as subscribeUi,
  type SettingsTab,
} from "./state";

function captureActivePanelScroll(): number {
  return (
    document.querySelector<HTMLElement>(".tab-panel.active")?.scrollTop ?? 0
  );
}

function restoreActivePanelScroll(scrollTop: number): void {
  const panel = document.querySelector<HTMLElement>(".tab-panel.active");
  if (panel) {
    panel.scrollTop = scrollTop;
  }
}

function render(): void {
  const root = document.querySelector<HTMLDivElement>("#app");
  if (!root) {
    return;
  }

  const scrollTop = captureActivePanelScroll();

  const {
    status,
    settings,
    devices,
    diagnostics,
    hasApiKey,
    activeTab,
    lastError,
    activityLog,
    whisperModel,
    whisperModels,
    aiSkills,
    whisperModelDownload,
    llmModel,
    llmModels,
    llmModelDownload,
    homemakerLocalSetup,
    homemakerHotkeyPresets,
    homemakerConfigLoading,
  } = getState();
  const homemaker = isHomemakerMode(settings);
  const uiMode = settings?.ui_mode ?? "homemaker";
  const selectedWhisper = whisperModels.find((model) => model.selected);
  const whisperModelsDir = getState().whisperModelsDir;
  const dictionaryPath = getState().dictionaryPath;
  const selectedLlm = llmModels.find((model) => model.selected);
  const llmModelsDir = getState().llmModelsDir;
  const formValues = settings
    ? settingsToForm(
        settings,
        hasApiKey,
        whisperModelsDir,
        selectedWhisper?.exists ?? whisperModel?.exists ?? false,
        dictionaryPath,
        llmModelsDir,
        selectedLlm?.exists ?? llmModel?.exists ?? false,
      )
    : null;

  root.innerHTML = `
    <main class="window${homemaker ? " window--standard" : ""}">
      <header class="header">
        <button
          type="button"
          class="app-logo-btn"
          data-open-about
          title="${escapeHtml(t("about.open"))}"
          aria-label="${escapeHtml(t("about.open"))}"
        >
          <img class="app-logo" src="${logoUrl}" alt="" width="44" height="44" />
        </button>
        <div class="header-text">
          <h1>${escapeHtml(t("app.title"))}</h1>
          <p class="subtitle subtitle-rotator" aria-live="polite">
            <span class="subtitle-line" data-tagline></span>
          </p>
        </div>
        <div class="header-actions">
          ${renderUiModeLink(uiMode)}
        </div>
      </header>

      ${homemaker && homemakerConfigLoading ? renderHomemakerConfigBanner() : ""}

      ${homemaker ? renderStandardOnboardingBanner() : ""}

      ${
        homemaker
          ? renderCompactStatusBar(status, getState().partialTranscript)
          : `${renderStatusBar(status, getState().partialTranscript)}
      ${renderUsageHint(status, diagnostics)}`
      }

      ${renderErrorBanner(status, lastError)}

      ${homemaker ? "" : renderCompactDiagnostics(diagnostics)}

      ${
        settings && homemaker
          ? renderHomemakerSettings(
              settings,
              hasApiKey,
              homemakerLocalSetup,
              whisperModelDownload,
              llmModelDownload,
              homemakerHotkeyPresets.length > 0
                ? homemakerHotkeyPresets
                : FALLBACK_HOMEMAKER_HOTKEY_PRESETS,
              homemakerConfigLoading,
            )
          : formValues
            ? `<section class="panel">
              ${renderTabBar(activeTab)}
              ${renderSettingsForm(formValues, devices, activeTab, activityLog, whisperModelDownload, whisperModels, aiSkills, diagnostics, llmModelDownload, llmModels, getState().transcriptionLanguages)}
            </section>`
            : settings
              ? `<section class="panel"><p class="hint">${escapeHtml(t("status.loading"))}</p></section>`
              : !getState().loading
                ? `<section class="panel"><p class="hint">${escapeHtml(lastError?.message ?? t("errors.bootstrap"))}</p></section>`
                : ""
      }
    </main>
  `;

  bindEvents();
  restoreActivePanelScroll(scrollTop);
}

function syncTextModeUi(form: HTMLFormElement): void {
  const select = form.querySelector<HTMLSelectElement>(
    'select[name="text_processing_mode"]',
  );
  const hint = form.querySelector<HTMLElement>("[data-text-mode-hint]");
  if (!select || !hint) {
    return;
  }

  const mode = select.value as TextProcessingMode;
  hint.textContent = t(textModeHintKey(mode));

  const aiAvailable = isAiRewriteAvailable(form);
  const pttEnabled =
    form.querySelector<HTMLInputElement>('input[name="push_to_talk"]')?.checked ?? true;
  select.querySelectorAll("option").forEach((option) => {
    const optionMode = option.value as TextProcessingMode;
    if (AI_TEXT_MODES.has(optionMode)) {
      option.disabled = !aiAvailable || !pttEnabled;
    }
  });

  syncAiSkillUi(form, mode);
}

function isAiRewriteAvailable(form: HTMLFormElement): boolean {
  const state = getState();
  const provider =
    form.querySelector<HTMLSelectElement>('select[name="text_rewrite_provider"]')
      ?.value ??
    state.settings?.text_rewrite_provider ??
    "openai";
  if (provider === "openai") {
    return state.hasApiKey;
  }
  if (!state.diagnostics?.local_llm_compiled) {
    return false;
  }
  const model =
    (form.querySelector<HTMLSelectElement>('select[name="local_llm_model"]')
      ?.value as LlmModelKind | undefined) ?? state.settings?.local_llm_model;
  const selected = state.llmModels.find((entry) => entry.kind === model);
  return selected?.exists ?? state.llmModel?.exists ?? false;
}

function syncAiSkillUi(form: HTMLFormElement, mode?: TextProcessingMode): void {
  const textMode =
    mode ??
    (form.querySelector<HTMLSelectElement>('select[name="text_processing_mode"]')
      ?.value as TextProcessingMode | undefined);
  const enabled = isAiRewriteAvailable(form) && textMode === "custom_skill";
  const field = form.querySelector<HTMLElement>("[data-ai-skill-field]");
  const skillSelect = form.querySelector<HTMLSelectElement>(
    'select[name="ai_rewrite_skill"]',
  );
  const folderBtn = form.querySelector<HTMLButtonElement>("[data-open-skills-folder]");
  const importBtn = form.querySelector<HTMLButtonElement>("[data-import-skill]");
  const catalogBtn = form.querySelector<HTMLButtonElement>("[data-open-skill-catalog]");

  if (field) {
    field.hidden = !enabled;
  }
  if (skillSelect) {
    skillSelect.disabled = !enabled;
  }
  folderBtn?.toggleAttribute("disabled", !enabled);
  importBtn?.toggleAttribute("disabled", !enabled);
  catalogBtn?.toggleAttribute("disabled", !enabled);
}

function syncTranscriptionProviderUi(form: HTMLFormElement): void {
  const provider =
    form.querySelector<HTMLSelectElement>('select[name="transcription_provider"]')
      ?.value ?? "local";
  form.querySelectorAll<HTMLElement>("[data-local-model-panel]").forEach((element) => {
    element.hidden = provider !== "local";
  });
  form.querySelectorAll<HTMLElement>("[data-local-only-toggle]").forEach((element) => {
    element.hidden = provider !== "local";
  });
}

function syncTextRewriteProviderUi(form: HTMLFormElement): void {
  const provider =
    form.querySelector<HTMLSelectElement>('select[name="text_rewrite_provider"]')
      ?.value ?? "openai";
  form.querySelectorAll<HTMLElement>("[data-local-llm-panel]").forEach((element) => {
    element.hidden = provider !== "local";
  });
}

function syncOpenAiApiKeyUi(form: HTMLFormElement): void {
  const transcriptionProvider =
    form.querySelector<HTMLSelectElement>('select[name="transcription_provider"]')
      ?.value ?? "local";
  const textRewriteProvider =
    form.querySelector<HTMLSelectElement>('select[name="text_rewrite_provider"]')
      ?.value ?? "openai";
  const needsKey =
    transcriptionProvider === "openai" || textRewriteProvider === "openai";

  form.querySelectorAll<HTMLElement>("[data-openai-api-key-panel]").forEach((element) => {
    element.hidden = !needsKey;
  });
  form.querySelectorAll<HTMLElement>("[data-openai-connections-section]").forEach((element) => {
    element.hidden = !needsKey;
  });
}

function syncDependentSettingsUi(form: HTMLFormElement): void {
  syncCaptureModeUi(form);
  syncEnterPhraseUi(form);
  syncTranscriptionProviderUi(form);
  syncTextRewriteProviderUi(form);
  syncOpenAiApiKeyUi(form);
  syncTextModeUi(form);
}

function syncEnterPhraseUi(form: HTMLFormElement): void {
  const enabled =
    form.querySelector<HTMLInputElement>('input[name="emulate_enter"]')?.checked ??
    false;

  form.querySelectorAll<HTMLElement>(".enter-phrase-only").forEach((element) => {
    element.hidden = !enabled;
  });
}

function syncCaptureModeUi(form: HTMLFormElement): void {
  const pushToTalk =
    form.querySelector<HTMLInputElement>('input[name="push_to_talk"]')?.checked ??
    false;

  form.querySelectorAll<HTMLElement>(".ptt-only").forEach((element) => {
    element.hidden = !pushToTalk;
  });

  form.querySelectorAll<HTMLElement>(".continuous-only").forEach((element) => {
    element.hidden = pushToTalk;
  });

  syncGameModeUi(form);
}

function syncGameModeUi(form: HTMLFormElement): void {
  const gameMode =
    form.querySelector<HTMLInputElement>('input[name="hotkey_game_mode"]')
      ?.checked ?? false;

  form
    .querySelectorAll<HTMLElement>(".voice-block-system-toggle")
    .forEach((element) => {
      element.hidden = !gameMode;
    });
}

async function handleRecoverEngine(): Promise<void> {
  const button = document.querySelector<HTMLButtonElement>("[data-recover-engine]");
  if (button) {
    button.disabled = true;
  }

  try {
    const status = await recoverEngine();
    setStatus(status);
    setError(null);
    patchState({
      diagnostics: await getDiagnostics(),
      lastError: null,
    });
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    setError({
      code: "recover",
      message,
    });
  } finally {
    if (button) {
      button.disabled = false;
    }
  }
}

async function refreshHomemakerLocalSetup(): Promise<void> {
  const { settings } = getState();
  if (!settings || !isHomemakerMode(settings)) {
    patchState({ homemakerLocalSetup: null });
    return;
  }

  if (settings.transcription_provider === "local") {
    patchState({ homemakerLocalSetup: await getHomemakerLocalSetup() });
  } else {
    patchState({ homemakerLocalSetup: null });
  }
}

async function persistHomemakerSettings(form: HTMLFormElement): Promise<void> {
  const values = readHomemakerForm(form);
  const currentSettings = getState().settings;
  const switchingToLocal =
    values.homemaker_data_storage === "local" &&
    currentSettings != null &&
    effectiveDataStorage(currentSettings) !== "local";
  const capslockSupported = getState().diagnostics?.capslock_ptt_supported ?? false;
  const patch: SettingsPatch = {
    homemaker_data_storage: values.homemaker_data_storage,
    text_processing_mode: values.text_processing_mode,
    hotkey_game_mode: false,
    push_to_talk: true,
  };

  if (values.global_hotkey === "CapsLock" && capslockSupported) {
    patch.capslock_ptt = true;
  } else {
    patch.capslock_ptt = false;
    patch.global_hotkey = values.global_hotkey;
  }

  if (values.homemaker_data_storage === "local") {
    patch.apply_homemaker_local_setup = true;
  }

  if (switchingToLocal) {
    patchState({ homemakerConfigLoading: true });
  }

  try {
    if (values.api_key) {
      await setApiKey(values.api_key);
      patchState({ hasApiKey: true });
      const apiInput = form.querySelector<HTMLInputElement>('input[name="api_key"]');
      if (apiInput) {
        apiInput.value = "";
      }
    }

    const nextSettings = await updateSettings(patch);
    setSettings(nextSettings);
    setLocale(nextSettings.ui_locale);
    await refreshHomemakerLocalSetup();
  } finally {
    if (switchingToLocal) {
      patchState({ homemakerConfigLoading: false });
    }
  }
}

function bindHomemakerEvents(form: HTMLFormElement): void {
  const onChange = (): void => {
    void persistHomemakerSettings(form).catch((error) => {
      const message = error instanceof Error ? error.message : String(error);
      setError({ code: "settings", message });
    });
  };

  form.querySelectorAll<HTMLInputElement>('input[name="homemaker_data_storage"]').forEach((input) => {
    input.addEventListener("change", onChange);
  });

  form.querySelectorAll<HTMLInputElement>('input[name="text_processing_mode"]').forEach((input) => {
    input.addEventListener("change", onChange);
  });

  form.querySelectorAll<HTMLInputElement>('input[name="homemaker_hotkey"]').forEach((input) => {
    input.addEventListener("change", onChange);
  });

  form.querySelector<HTMLInputElement>('input[name="api_key"]')?.addEventListener("change", onChange);

  form.querySelector<HTMLButtonElement>("[data-open-skill-catalog]")?.addEventListener("click", () => {
    void openUrl(skillCatalogUrl(getLocale()));
  });

  form.querySelector<HTMLButtonElement>("[data-download-whisper-homemaker]")?.addEventListener("click", () => {
    if (getState().whisperModelDownload) {
      return;
    }

    void (async () => {
      const setup = getState().homemakerLocalSetup ?? (await getHomemakerLocalSetup());
      const confirmed = await showConfirmDialog({
        message: t("homemaker.downloadConfirm", {
          model: t("homemaker.downloadWhisper"),
          size: setup.whisper_size_mb,
        }),
      });
      if (!confirmed) {
        return;
      }

      const model = setup.local_stt_model;
      patchState({
        whisperModelDownload: { downloaded: 0, total: null, percent: null },
        lastError: null,
      });

      const current = getState().settings;
      if (current && current.local_stt_model !== model) {
        setSettings(await updateSettings({ local_stt_model: model }));
      }

      await downloadLocalSttModel(model);
      patchState({
        whisperModel: await getWhisperModelStatus(),
        whisperModels: await listLocalSttModels(),
        whisperModelDownload: null,
      });
      await refreshHomemakerLocalSetup();
    })().catch((error) => {
      patchState({ whisperModelDownload: null });
      const message = error instanceof Error ? error.message : String(error);
      setError({ code: "model_download", message });
    });
  });

  form.querySelector<HTMLButtonElement>("[data-download-llm-homemaker]")?.addEventListener("click", () => {
    if (getState().llmModelDownload) {
      return;
    }

    void (async () => {
      const setup = getState().homemakerLocalSetup ?? (await getHomemakerLocalSetup());
      const confirmed = await showConfirmDialog({
        message: t("homemaker.downloadConfirm", {
          model: t("homemaker.downloadLlm"),
          size: setup.llm_size_mb,
        }),
      });
      if (!confirmed) {
        return;
      }

      const model = setup.local_llm_model;
      patchState({
        llmModelDownload: { downloaded: 0, total: null, percent: null },
        lastError: null,
      });

      const current = getState().settings;
      if (current && current.local_llm_model !== model) {
        setSettings(await updateSettings({ local_llm_model: model }));
      }

      await downloadLlmModel(model);
      patchState({
        llmModel: await getLlmModelStatus(),
        llmModels: await listLlmModels(),
        llmModelDownload: null,
      });
      await refreshHomemakerLocalSetup();
    })().catch((error) => {
      patchState({ llmModelDownload: null });
      const message = error instanceof Error ? error.message : String(error);
      setError({ code: "llm_model_download", message });
    });
  });
}

let expertTabListenersBound = false;

function bindExpertTabListeners(): void {
  const root = document.querySelector<HTMLDivElement>("#app");
  if (!root || expertTabListenersBound) {
    return;
  }
  expertTabListenersBound = true;

  root.addEventListener("click", (event) => {
    const target = event.target;
    if (!(target instanceof Element)) {
      return;
    }
    const button = target.closest<HTMLButtonElement>(".tab[data-tab]");
    if (!button || !root.contains(button)) {
      return;
    }
    const tab = button.dataset.tab as SettingsTab | undefined;
    if (!tab) {
      return;
    }
    setActiveTab(tab);
    if (tab === "advanced") {
      void refreshDiagnostics();
    }
  });

  root.addEventListener("keydown", (event) => {
    const target = event.target;
    if (!(target instanceof HTMLButtonElement) || !target.matches(".tab[data-tab]")) {
      return;
    }
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") {
      return;
    }
    const tabs = Array.from(root.querySelectorAll<HTMLButtonElement>(".tab[data-tab]"));
    const index = tabs.indexOf(target);
    if (index < 0) {
      return;
    }
    event.preventDefault();
    const direction = event.key === "ArrowRight" ? 1 : -1;
    const next = tabs[(index + direction + tabs.length) % tabs.length];
    next?.click();
    next?.focus();
  });
}

function bindEvents(): void {
  const uiMode = getState().settings?.ui_mode ?? "homemaker";
  mountRotatingTagline(document.querySelector<HTMLElement>(".subtitle-rotator"), uiMode);
  bindUiModeSwitch(document, uiMode);
  bindStandardOnboardingBanner(document);

  document
    .querySelector<HTMLButtonElement>("[data-open-about]")
    ?.addEventListener("click", () => {
      void openAboutWindow().catch((error) => {
        const message = error instanceof Error ? error.message : String(error);
        setError({ code: "about_window", message });
      });
    });

  document
    .querySelector<HTMLButtonElement>("[data-recover-engine]")
    ?.addEventListener("click", () => {
      void handleRecoverEngine();
    });

  const homemakerForm = document.querySelector<HTMLFormElement>("#homemaker-form");
  if (homemakerForm) {
    bindHomemakerEvents(homemakerForm);
    return;
  }

  const form = document.querySelector<HTMLFormElement>("#settings-form");
  if (!form) {
    return;
  }

  const onSettingsChange = (): void => {
    schedulePersistSettings();
  };

  bindHotkeyInputs(form, onSettingsChange);
  bindVadThresholdPanel(form, onSettingsChange);
  syncDependentSettingsUi(form);

  form.querySelector<HTMLButtonElement>("[data-download-whisper-model]")?.addEventListener("click", () => {
    if (getState().whisperModelDownload) {
      return;
    }

    const model =
      (form.querySelector<HTMLSelectElement>('select[name="local_stt_model"]')?.value ??
        "base") as LocalSttModelKind;

    patchState({
      whisperModelDownload: { downloaded: 0, total: null, percent: null },
      lastError: null,
    });

    const current = getState().settings;
    const persistModel =
      current && current.local_stt_model !== model
        ? updateSettings({ local_stt_model: model }).then(setSettings)
        : Promise.resolve();

    void persistModel
      .then(() => downloadLocalSttModel(model))
      .then(async () => {
        patchState({
          whisperModel: await getWhisperModelStatus(),
          whisperModels: await listLocalSttModels(),
          whisperModelDownload: null,
        });
      })
      .catch((error) => {
        patchState({ whisperModelDownload: null });
        const message = error instanceof Error ? error.message : String(error);
        setError({
          code: "model_download",
          message,
        });
      });
  });

  form.querySelector<HTMLButtonElement>("[data-download-llm-model]")?.addEventListener("click", () => {
    if (getState().llmModelDownload) {
      return;
    }

    const model =
      (form.querySelector<HTMLSelectElement>('select[name="local_llm_model"]')?.value ??
        "qwen3_4b") as LlmModelKind;

    patchState({
      llmModelDownload: { downloaded: 0, total: null, percent: null },
      lastError: null,
    });

    const current = getState().settings;
    const persistModel =
      current && current.local_llm_model !== model
        ? updateSettings({ local_llm_model: model }).then(setSettings)
        : Promise.resolve();

    void persistModel
      .then(() => downloadLlmModel(model))
      .then(async () => {
        patchState({
          llmModel: await getLlmModelStatus(),
          llmModels: await listLlmModels(),
          llmModelDownload: null,
        });
      })
      .catch((error) => {
        patchState({ llmModelDownload: null });
        const message = error instanceof Error ? error.message : String(error);
        setError({
          code: "llm_model_download",
          message,
        });
      });
  });

  form.querySelector<HTMLButtonElement>("[data-pick-llm-models-dir]")?.addEventListener("click", () => {
    void pickLlmModelsDir()
      .then(async (dir) => {
        if (!dir) {
          return;
        }

        const nextSettings = await updateSettings({ local_llm_models_dir: dir });
        setSettings(nextSettings);
        patchState({
          llmModelsDir: await getLlmModelsDir(),
          llmModels: await listLlmModels(),
          llmModel: await getLlmModelStatus(),
          lastError: null,
        });
      })
      .catch((error) => {
        const message = error instanceof Error ? error.message : String(error);
        setError({
          code: "llm_models_dir",
          message,
        });
      });
  });

  form.querySelector<HTMLButtonElement>("[data-open-dictionary-folder]")?.addEventListener("click", () => {
    void openTranscriptionDictionaryFolder().catch((error) => {
      const message = error instanceof Error ? error.message : String(error);
      setError({ code: "dictionary_folder", message });
    });
  });

  form.querySelector<HTMLButtonElement>("[data-pick-transcription-dictionary]")?.addEventListener("click", () => {
    void pickTranscriptionDictionary()
      .then(async (path) => {
        if (!path) {
          return;
        }

        const nextSettings = await updateSettings({ transcription_dictionary_path: path });
        setSettings(nextSettings);
        patchState({
          dictionaryPath: await getDictionaryPath(),
          lastError: null,
        });
      })
      .catch((error) => {
        const message = error instanceof Error ? error.message : String(error);
        setError({ code: "dictionary_path", message });
      });
  });

  form.querySelector<HTMLInputElement>('input[name="audio_preprocess_enabled"]')?.addEventListener("change", (event) => {
    const enabled = (event.target as HTMLInputElement).checked;
    const noiseInput = form.querySelector<HTMLInputElement>('input[name="audio_noise_reduction_enabled"]');
    if (noiseInput) {
      noiseInput.disabled = !enabled;
      if (!enabled) {
        noiseInput.checked = false;
      }
    }
  });

  form.querySelector<HTMLButtonElement>("[data-pick-whisper-models-dir]")?.addEventListener("click", () => {
    void pickWhisperModelsDir()
      .then(async (dir) => {
        if (!dir) {
          return;
        }

        const nextSettings = await updateSettings({ local_whisper_models_dir: dir });
        setSettings(nextSettings);
        patchState({
          whisperModelsDir: await getWhisperModelsDir(),
          whisperModels: await listLocalSttModels(),
          whisperModel: await getWhisperModelStatus(),
          lastError: null,
        });
      })
      .catch((error) => {
        const message = error instanceof Error ? error.message : String(error);
        setError({
          code: "models_dir",
          message,
        });
      });
  });

  form.querySelector<HTMLButtonElement>("[data-clear-api-key]")?.addEventListener("click", () => {
    void clearApiKey()
      .then(async () => {
        const input = form.querySelector<HTMLInputElement>('input[name="api_key"]');
        if (input) {
          input.value = "";
        }

        const current = getState().settings;
        const aiModes = new Set<TextProcessingMode>(["optimization", "custom_skill"]);
        const patch: SettingsPatch = {};
        if (
          current &&
          aiModes.has(current.text_processing_mode) &&
          current.text_rewrite_provider === "openai"
        ) {
          patch.text_processing_mode = "basic";
        }
        if (current?.transcription_provider === "openai") {
          patch.transcription_provider = "local";
        }
        if (Object.keys(patch).length > 0) {
          const nextSettings = await updateSettings(patch);
          setSettings(nextSettings);
        }

        patchState({
          hasApiKey: false,
          diagnostics: await getDiagnostics(),
          lastError: null,
        });
        syncDependentSettingsUi(form);
      })
      .catch((error) => {
        const message = error instanceof Error ? error.message : String(error);
        setError({
          code: "api_key",
          message,
        });
      });
  });

  form.querySelector<HTMLButtonElement>("[data-open-skills-folder]")?.addEventListener("click", () => {
    void openAiSkillsFolder().then(async () => {
      patchState({ aiSkills: await listAiSkills() });
    });
  });

  form.querySelector<HTMLButtonElement>("[data-open-skill-catalog]")?.addEventListener("click", () => {
    void openUrl(skillCatalogUrl(getLocale()));
  });

  form.querySelector<HTMLButtonElement>("[data-import-skill]")?.addEventListener("click", () => {
    void pickAndImportAiSkill()
      .then(async (skill) => {
        patchState({ aiSkills: await listAiSkills() });
        const select = form.querySelector<HTMLSelectElement>('select[name="ai_rewrite_skill"]');
        if (select) {
          select.value = skill.filename;
        }
        schedulePersistSettings();
      })
      .catch((error) => {
        const message = error instanceof Error ? error.message : String(error);
        if (message.includes("skill_import_cancelled")) {
          return;
        }
        setError({
          code: "skill_import",
          message,
        });
      });
  });

  form
    .querySelectorAll<HTMLSelectElement | HTMLInputElement>(
      "select, input[type='checkbox'], input[type='number']",
    )
    .forEach((element) => {
      element.addEventListener("change", () => {
        if (element instanceof HTMLInputElement && element.name === "push_to_talk") {
          syncCaptureModeUi(form);
          if (!element.checked) {
            const current = getState().settings;
            const aiModes = new Set<TextProcessingMode>(["optimization", "custom_skill"]);
            if (current && aiModes.has(current.text_processing_mode)) {
              const fallback: TextProcessingMode =
                current.ui_mode === "homemaker" ? "original" : "basic";
              const select = form.querySelector<HTMLSelectElement>(
                'select[name="text_processing_mode"]',
              );
              if (select) {
                select.value = fallback;
              }
              setSettings({ ...current, text_processing_mode: fallback });
              void flushPersistSettings();
            }
          }
          syncTextModeUi(form);
        }
        if (
          element instanceof HTMLInputElement &&
          element.name === "hotkey_game_mode"
        ) {
          syncGameModeUi(form);
        }
        if (element instanceof HTMLInputElement && element.name === "emulate_enter") {
          syncEnterPhraseUi(form);
        }
        if (
          element instanceof HTMLSelectElement &&
          element.name === "ui_locale"
        ) {
          const locale = element.value as import("./api").UiLocale;
          const current = getState().settings;
          if (current) {
            const baseline = current;
            setSettings({ ...current, ui_locale: locale });
            setLocale(locale);
            void flushPersistSettings({ compareWith: baseline });
          } else {
            setLocale(locale);
          }
          return;
        }
        if (
          element instanceof HTMLSelectElement &&
          element.name === "text_processing_mode"
        ) {
          const mode = element.value as TextProcessingMode;
          const current = getState().settings;
          if (current) {
            const baseline = current;
            let aiRewriteSkill = current.ai_rewrite_skill;
            if (
              mode === "custom_skill" &&
              (!aiRewriteSkill || aiRewriteSkill.length === 0)
            ) {
              const firstSkill = getState().aiSkills[0];
              if (firstSkill) {
                aiRewriteSkill = firstSkill.filename;
                const skillSelect = form.querySelector<HTMLSelectElement>(
                  'select[name="ai_rewrite_skill"]',
                );
                if (skillSelect) {
                  skillSelect.value = firstSkill.filename;
                }
              }
            }
            setSettings({
              ...current,
              text_processing_mode: mode,
              ai_rewrite_skill: aiRewriteSkill,
            });
            syncTextModeUi(form);
            void flushPersistSettings({ compareWith: baseline });
          } else {
            syncTextModeUi(form);
          }
          return;
        }
        if (
          element instanceof HTMLSelectElement &&
          element.name === "transcription_provider"
        ) {
          syncDependentSettingsUi(form);
          const current = getState().settings;
          if (current) {
            const baseline = current;
            const provider = element.value;
            setSettings({ ...current, transcription_provider: provider });
            void flushPersistSettings({ compareWith: baseline });
          }
          return;
        }
        if (
          element instanceof HTMLSelectElement &&
          element.name === "local_stt_model"
        ) {
          const current = getState().settings;
          if (current) {
            const baseline = current;
            const model = element.value as LocalSttModelKind;
            setSettings({ ...current, local_stt_model: model });
            void flushPersistSettings({ compareWith: baseline });
          }
          return;
        }
        if (
          element instanceof HTMLSelectElement &&
          element.name === "text_rewrite_provider"
        ) {
          syncDependentSettingsUi(form);
          const current = getState().settings;
          if (current) {
            const baseline = current;
            const provider = element.value as import("./api").TextRewriteProvider;
            setSettings({ ...current, text_rewrite_provider: provider });
            void flushPersistSettings({ compareWith: baseline });
          }
          return;
        }
        if (
          element instanceof HTMLSelectElement &&
          element.name === "local_llm_model"
        ) {
          syncTextModeUi(form);
          const current = getState().settings;
          if (current) {
            const baseline = current;
            const model = element.value as LlmModelKind;
            setSettings({ ...current, local_llm_model: model });
            void flushPersistSettings({ compareWith: baseline });
          }
          return;
        }
        if (
          element instanceof HTMLSelectElement &&
          element.name === "microphone_device"
        ) {
          const current = getState().settings;
          if (current) {
            const baseline = current;
            const device =
              element.value.length > 0 ? element.value : null;
            setSettings({ ...current, microphone_device: device });
            void flushPersistSettings({ compareWith: baseline });
          }
          return;
        }
        onSettingsChange();
      });
    });

  form
    .querySelector<HTMLInputElement>('input[name="api_key"]')
    ?.addEventListener("blur", () => {
      void flushPersistSettings().then(() => syncDependentSettingsUi(form));
    });

  form
    .querySelector<HTMLInputElement>('input[name="enter_trigger_phrase"]')
    ?.addEventListener("change", () => {
      onSettingsChange();
    });

  form.querySelector<HTMLButtonElement>("[data-clear-log]")?.addEventListener("click", () => {
    void clearActivityLog().then(async () => {
      patchState({
        activityLog: await getActivityLog(),
        diagnostics: await getDiagnostics(),
      });
    });
  });
}

async function refreshDiagnostics(): Promise<void> {
  patchState({ diagnostics: await getDiagnostics() });
}

const BUSY_INVOKE_RE = /busy|try again/i;

async function invokeWithRetry<T>(
  fn: () => Promise<T>,
  attempts = 40,
  delayMs = 250,
): Promise<T> {
  let lastError: unknown;
  for (let attempt = 0; attempt < attempts; attempt++) {
    try {
      return await fn();
    } catch (error) {
      lastError = error;
      const message = error instanceof Error ? error.message : String(error);
      if (!BUSY_INVOKE_RE.test(message) || attempt === attempts - 1) {
        throw error;
      }
      await new Promise((resolve) => setTimeout(resolve, delayMs));
    }
  }
  throw lastError;
}

async function loadBootstrapSecondaryData(initialSettings: AppSettings): Promise<void> {
  try {
    const apiKeyConfigured = await invokeWithRetry(() => hasApiKey());

    const [devices, transcriptionLanguages, whisperModels, diagnostics] = await Promise.all([
      invokeWithRetry(() => getDevices()),
      invokeWithRetry(() => listTranscriptionLanguages()),
      invokeWithRetry(() => listLocalSttModels()),
      invokeWithRetry(() => getDiagnostics()),
    ]);

    patchState({
      devices,
      transcriptionLanguages,
      whisperModels,
      diagnostics,
      hasApiKey: apiKeyConfigured,
    });

    const [
      activityLog,
      whisperModel,
      aiSkills,
      whisperModelsDir,
      dictionaryPath,
      llmModel,
      llmModels,
      llmModelsDir,
      homemakerHotkeyPresets,
    ] = await Promise.all([
      invokeWithRetry(() => getActivityLog()),
      invokeWithRetry(() => getWhisperModelStatus()).catch(() => null),
      invokeWithRetry(() => listAiSkills()).catch(() => []),
      invokeWithRetry(() => getWhisperModelsDir()).catch(() => ""),
      invokeWithRetry(() => getDictionaryPath()).catch(() => ""),
      invokeWithRetry(() => getLlmModelStatus()).catch(() => null),
      invokeWithRetry(() => listLlmModels()).catch(() => []),
      invokeWithRetry(() => getLlmModelsDir()).catch(() => ""),
      invokeWithRetry(() => getHomemakerHotkeyPresets()).catch(() => [
        ...FALLBACK_HOMEMAKER_HOTKEY_PRESETS,
      ]),
    ]);

    let effectiveSettings = initialSettings;
    if (!apiKeyConfigured && initialSettings.transcription_provider === "openai") {
      effectiveSettings = await invokeWithRetry(() =>
        updateSettings({ transcription_provider: "local" }),
      );
    }

    patchState({
      settings: effectiveSettings,
      activityLog,
      whisperModel,
      whisperModelsDir,
      dictionaryPath,
      llmModel,
      llmModels,
      llmModelsDir,
      aiSkills,
      homemakerHotkeyPresets,
    });

    if (isHomemakerMode(effectiveSettings)) {
      await refreshHomemakerLocalSetup();
    }

    void prewarmMicrophone(effectiveSettings.microphone_device);
    void prewarmLocalModels();

    void runStartupUpdateCheck(
      mountUpdateBannerSlot(),
      effectiveSettings.check_updates_on_startup ?? true,
    );
  } catch (error) {
    setError({
      code: "bootstrap",
      message: error instanceof Error ? error.message : String(error),
    });
  }
}

async function bootstrap(): Promise<void> {
  bindExpertTabListeners();
  subscribeUi(() => render());
  subscribeLocale(() => render());
  startMicLevelMonitor();
  startVadThresholdMonitor();

  try {
    const [status, settings] = await Promise.all([
      invokeWithRetry(() => getStatus()),
      invokeWithRetry(() => getSettings()),
    ]);

    setLocale(settings.ui_locale ?? "en");

    patchState({
      status,
      settings,
      loading: false,
      lastError: null,
    });

    void loadBootstrapSecondaryData(settings);
  } catch (error) {
    patchState({ loading: false });
    setError({
      code: "bootstrap",
      message: error instanceof Error ? error.message : String(error),
    });
  }

  await subscribe<{ status: StatusSnapshot }>(EVENTS.stateChanged, (payload) => {
    setStatus(payload.status);
  });

  await subscribe<import("./api").ErrorPayload>(EVENTS.error, (payload) => {
    setError(payload);
    void refreshDiagnostics();
  });

  await subscribe<TranscriptionCompletedPayload>(EVENTS.transcriptionCompleted, () => {
    patchState({ partialTranscript: null });
    void refreshDiagnostics();
  });

  await subscribe<import("./api").TranscriptionPartialPayload>(
    EVENTS.transcriptionPartial,
    (payload) => {
      const raw = payload.text ?? "";
      const previous = getState().partialTranscript ?? "";
      const merged =
        previous && raw.startsWith(previous.trim())
          ? ensureSpacesAfterPunctuation(raw)
          : mergeTranscriptChunks(previous, raw);
      patchState({ partialTranscript: merged }, { render: false });
      const currentStatus = getState().status;
      if (currentStatus) {
        patchLiveStatusUi(currentStatus, merged);
      }
    },
  );

  await subscribe(EVENTS.transcriptionPartialClear, () => {
    patchState({ partialTranscript: null });
  });

  await subscribe<ActivityLogEntry[]>(EVENTS.activityLog, (entries) => {
    patchState({ activityLog: entries }, { render: false });
    updateActivityLogDom(entries);
  });

  await subscribe<import("./api").WhisperModelDownloadProgress>(
    EVENTS.whisperModelDownloadProgress,
    (progress) => {
      if (isHomemakerMode(getState().settings)) {
        patchState({ whisperModelDownload: progress });
        return;
      }
      patchState({ whisperModelDownload: progress }, { render: false });
      updateWhisperDownloadUi(progress);
    },
  );

  await subscribe<import("./api").LlmModelDownloadProgress>(
    EVENTS.llmModelDownloadProgress,
    (progress) => {
      if (isHomemakerMode(getState().settings)) {
        patchState({ llmModelDownload: progress });
        return;
      }
      patchState({ llmModelDownload: progress }, { render: false });
      updateLlmDownloadUi(progress);
    },
  );

  await subscribe<AppSettings>(EVENTS.settingsChanged, async (settings) => {
    setSettings(settings);
    setLocale(settings.ui_locale);
    patchState({ hasApiKey: await hasApiKey() });
    if (isHomemakerMode(settings)) {
      await refreshHomemakerLocalSetup();
    } else {
      patchState({ homemakerLocalSetup: null });
    }
  });

  await subscribe(EVENTS.skillsChanged, async () => {
    const aiSkills = await listAiSkills();
    const currentSkill = getState().settings?.ai_rewrite_skill ?? "";
    patchState({ aiSkills });

    const form = document.querySelector<HTMLFormElement>("#settings-form");
    const select = form?.querySelector<HTMLSelectElement>('select[name="ai_rewrite_skill"]');
    if (!select) {
      return;
    }

    const stillExists = aiSkills.some((skill) => skill.filename === currentSkill);
    if (!stillExists && aiSkills.length > 0) {
      select.value = aiSkills[0].filename;
      schedulePersistSettings();
    }
  });

  await subscribe<import("./api").AiSkillInfo>(EVENTS.skillImported, async (skill) => {
    const nextSettings = await getSettings();
    patchState({
      settings: nextSettings,
      aiSkills: await listAiSkills(),
    });
    const form = document.querySelector<HTMLFormElement>("#settings-form");
    if (form) {
      const select = form.querySelector<HTMLSelectElement>('select[name="ai_rewrite_skill"]');
      if (select) {
        select.value = skill.filename;
      }
      const modeSelect = form.querySelector<HTMLSelectElement>(
        'select[name="text_processing_mode"]',
      );
      if (modeSelect) {
        modeSelect.value = "custom_skill";
      }
      syncDependentSettingsUi(form);
    }
  });

  for (const event of [
    EVENTS.listeningStarted,
    EVENTS.listeningStopped,
    EVENTS.transcriptionStarted,
    EVENTS.injectionCompleted,
    EVENTS.transcriptionPartialClear,
  ]) {
    await subscribe(event, () => {
      void refreshDiagnostics();
    });
  }

  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") {
      void refreshDiagnostics();
    }
  });
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

export function startApp(): void {
  void bootstrap();
}
