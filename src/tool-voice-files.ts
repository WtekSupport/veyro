import { emit } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { EVENTS, getSettings } from "./api";
import { createVoiceFilesController } from "./components/tool-voice-files-ui";
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
  const root = document.querySelector<HTMLDivElement>("#tool-voice-files-app");
  if (!root) {
    return;
  }

  try {
    const settings = await getSettings();
    setLocale(settings.ui_locale);
  } catch {
    setLocale("en");
  }

  const controller = createVoiceFilesController(root);
  await bindTauriDragDrop(controller.enqueuePaths);
  await emit(EVENTS.voiceFilesWindowReady, {});
}

window.addEventListener("DOMContentLoaded", () => {
  void bootstrap();
});
