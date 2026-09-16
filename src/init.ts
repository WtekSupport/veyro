import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { EVENTS, type StatusSnapshot } from "./api";
import { setLocale, t } from "./i18n";

async function bootstrap(): Promise<void> {
  try {
    const settings = await invoke<{ ui_locale: "en" | "ru" }>("get_settings");
    setLocale(settings.ui_locale);
  } catch {
    setLocale("ru");
  }

  document.querySelector<HTMLElement>("[data-init-title]")!.textContent = t("app.title");
  document.querySelector<HTMLElement>("[data-init-message]")!.textContent = t("init.message");

  await listen<{ status: StatusSnapshot }>(EVENTS.stateChanged, (event) => {
    if (event.payload.status.state !== "initializing") {
      void import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
        void getCurrentWindow().hide();
      });
    }
  });
}

void bootstrap();
