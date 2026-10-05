import { emit } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { EVENTS, getSettings } from "./api";
import { createSpeechAnalysisController } from "./components/tool-speech-analysis-ui";
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
    // File picker remains available.
  }
}

async function bootstrap(): Promise<void> {
  const root = document.querySelector<HTMLDivElement>("#tool-speech-analysis-app");
  if (!root) {
    return;
  }
  try {
    const settings = await getSettings();
    setLocale(settings.ui_locale);
  } catch {
    setLocale("en");
  }
  const controller = createSpeechAnalysisController(root);
  await bindTauriDragDrop((paths) => controller.enqueue(paths));
  await emit(EVENTS.speechAnalysisWindowReady);
}

window.addEventListener("DOMContentLoaded", () => {
  void bootstrap();
});
