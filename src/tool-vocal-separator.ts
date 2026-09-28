import { emit } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { EVENTS, getSettings } from "./api";
import { createVocalSeparatorController } from "./components/tool-vocal-separator-ui";
import { setLocale } from "./i18n";

async function bindTauriDragDrop(enqueuePaths: (paths: string[]) => void): Promise<void> {
  try {
    const webview = getCurrentWebviewWindow();
    await webview.onDragDropEvent((event) => {
      const payload = event.payload;
      if (payload.type === "drop" && payload.paths.length > 0) {
        enqueuePaths(payload.paths);
      }
    });
  } catch {
    // HTML5 drop + file picker remain available.
  }
}

async function bootstrap(): Promise<void> {
  const root = document.querySelector<HTMLDivElement>("#tool-vocal-separator-app");
  if (!root) {
    return;
  }

  try {
    const settings = await getSettings();
    setLocale(settings.ui_locale);
  } catch {
    setLocale("en");
  }

  const controller = createVocalSeparatorController(root);
  await bindTauriDragDrop(controller.enqueuePaths);
  await emit(EVENTS.vocalSeparatorWindowReady, {});
}

window.addEventListener("DOMContentLoaded", () => {
  void bootstrap();
});
