import { emit } from "@tauri-apps/api/event";
import { EVENTS, getSettings } from "./api";
import { createDictationTranscriptsController } from "./components/tool-dictation-transcripts-ui";
import { setLocale } from "./i18n";

async function bootstrap(): Promise<void> {
  const root = document.querySelector<HTMLDivElement>("#tool-dictation-transcripts-app");
  if (!root) {
    return;
  }

  try {
    const settings = await getSettings();
    setLocale(settings.ui_locale);
  } catch {
    setLocale("en");
  }

  createDictationTranscriptsController(root);
  await emit(EVENTS.dictationTranscriptsWindowReady, {});
}

window.addEventListener("DOMContentLoaded", () => {
  void bootstrap();
});
