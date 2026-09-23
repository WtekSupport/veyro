import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { EVENTS, type AiSkillInfo } from "./api";
import { setLocale, t } from "./i18n";

type SkillImportPhase = "preparing" | "preview" | "installing" | "done" | "error";

interface SkillImportFlowPayload {
  phase: SkillImportPhase;
  skill?: AiSkillInfo;
  source?: string;
  size_bytes?: number;
  error?: string;
}

function query<T extends HTMLElement>(selector: string): T {
  const el = document.querySelector<T>(selector);
  if (!el) {
    throw new Error(`missing element: ${selector}`);
  }
  return el;
}

function applyStaticLabels(): void {
  document.title = t("app.title");
  query<HTMLElement>("[data-skill-import-title]").textContent = t("skillImport.title");
  query<HTMLElement>("[data-skill-import-preparing-message]").textContent = t("skillImport.preparing");
  query<HTMLElement>("[data-skill-import-installing-message]").textContent = t("skillImport.installing");
  query<HTMLElement>("[data-skill-import-question]").textContent = t("skillImport.question");
  query<HTMLElement>("[data-skill-import-label-name]").textContent = t("skillImport.name");
  query<HTMLElement>("[data-skill-import-label-filename]").textContent = t("skillImport.filename");
  query<HTMLElement>("[data-skill-import-label-size]").textContent = t("skillImport.size");
  query<HTMLElement>("[data-skill-import-label-source]").textContent = t("skillImport.source");
  query<HTMLElement>("[data-skill-import-label-description]").textContent = t("skillImport.description");
  query<HTMLButtonElement>("[data-skill-import-cancel]").textContent = t("common.cancel");
  query<HTMLButtonElement>("[data-skill-import-install]").textContent = t("skillImport.install");
  query<HTMLButtonElement>("[data-skill-import-error-close]").textContent = t("common.close");
  query<HTMLButtonElement>("[data-skill-import-preparing-cancel]").textContent = t("common.cancel");
}

function showPhase(phase: SkillImportPhase): void {
  const sections = document.querySelectorAll<HTMLElement>(
    "[data-skill-import-preparing], [data-skill-import-preview], [data-skill-import-installing], [data-skill-import-error]",
  );
  for (const section of sections) {
    section.hidden = true;
  }

  const selector =
    phase === "preparing"
      ? "[data-skill-import-preparing]"
      : phase === "preview"
        ? "[data-skill-import-preview]"
        : phase === "error"
          ? "[data-skill-import-error]"
          : "[data-skill-import-installing]";
  query<HTMLElement>(selector).hidden = false;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

function applyPreview(payload: SkillImportFlowPayload): void {
  const skill = payload.skill!;
  query<HTMLElement>("[data-skill-import-name]").textContent = skill.name;
  query<HTMLElement>("[data-skill-import-filename]").textContent = skill.filename;
  query<HTMLElement>("[data-skill-import-size]").textContent =
    payload.size_bytes != null ? formatBytes(payload.size_bytes) : "—";
  query<HTMLElement>("[data-skill-import-source]").textContent = payload.source ?? "—";

  const descriptionRow = query<HTMLElement>("[data-skill-import-description-row]");
  const description = skill.description.trim();
  if (description) {
    descriptionRow.hidden = false;
    query<HTMLElement>("[data-skill-import-description]").textContent = description;
  } else {
    descriptionRow.hidden = true;
  }
}

function setInstallProgress(percent: number): void {
  query<HTMLElement>("[data-skill-import-progress]").style.width = `${Math.min(100, Math.max(0, percent))}%`;
}

async function hideWindow(): Promise<void> {
  await getCurrentWindow().hide();
}

function handleFlow(payload: SkillImportFlowPayload): void {
  if (payload.phase === "preparing") {
    showPhase("preparing");
    return;
  }
  if (payload.phase === "preview" && payload.skill) {
    applyPreview(payload);
    showPhase("preview");
    query<HTMLButtonElement>("[data-skill-import-install]").focus();
    return;
  }
  if (payload.phase === "installing") {
    showPhase("installing");
    setInstallProgress(35);
    return;
  }
  if (payload.phase === "done") {
    setInstallProgress(100);
    void hideWindow();
    return;
  }
  if (payload.phase === "error") {
    query<HTMLElement>("[data-skill-import-error-text]").textContent = payload.error ?? t("skillImport.errorGeneric");
    showPhase("error");
  }
}

async function bootstrap(): Promise<void> {
  try {
    const settings = await invoke<{ ui_locale: "en" | "ru" }>("get_settings");
    setLocale(settings.ui_locale);
  } catch {
    setLocale("ru");
  }

  applyStaticLabels();
  showPhase("preparing");

  const unlisten = await listen<SkillImportFlowPayload>(EVENTS.skillImportFlow, (event) => {
    handleFlow(event.payload);
  });

  try {
    await invoke("skill_import_ui_ready");
  } catch {
    try {
      const snapshot = await invoke<SkillImportFlowPayload | null>("get_skill_import_flow_snapshot");
      if (snapshot) {
        handleFlow(snapshot);
      }
    } catch {
      /* backend may be older during dev hot reload */
    }
  }

  window.addEventListener("beforeunload", () => {
    void unlisten();
  });

  const cancelImport = (): void => {
    void invoke("cancel_deeplink_skill_import").then(() => hideWindow());
  };

  query<HTMLButtonElement>("[data-skill-import-preparing-cancel]").addEventListener("click", cancelImport);

  query<HTMLButtonElement>("[data-skill-import-cancel]").addEventListener("click", () => {
    cancelImport();
  });

  query<HTMLButtonElement>("[data-skill-import-install]").addEventListener("click", () => {
    const installBtn = query<HTMLButtonElement>("[data-skill-import-install]");
    const cancelBtn = query<HTMLButtonElement>("[data-skill-import-cancel]");
    installBtn.disabled = true;
    cancelBtn.disabled = true;
    void invoke("confirm_deeplink_skill_import");
  });

  query<HTMLButtonElement>("[data-skill-import-error-close]").addEventListener("click", cancelImport);
}

void bootstrap();
