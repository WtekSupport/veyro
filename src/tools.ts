import { getSettings } from "./api";
import { bindToolsList, renderToolsList } from "./components/tools-list";
import { setLocale } from "./i18n";

async function bootstrap(): Promise<void> {
  const root = document.querySelector<HTMLDivElement>("#tools-app");
  if (!root) {
    return;
  }

  try {
    const settings = await getSettings();
    setLocale(settings.ui_locale);
  } catch {
    setLocale("en");
  }

  root.innerHTML = renderToolsList();
  bindToolsList(root);
}

window.addEventListener("DOMContentLoaded", () => {
  void bootstrap();
});
