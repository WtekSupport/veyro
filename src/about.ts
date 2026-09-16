import { getAppInfo, getSettings, getThirdPartyLicenses } from "./api";
import { renderAboutWindow } from "./components/about";
import { setLocale } from "./i18n";

async function bootstrap(): Promise<void> {
  const root = document.querySelector<HTMLDivElement>("#about-app");
  if (!root) {
    return;
  }

  try {
    const settings = await getSettings();
    setLocale(settings.ui_locale);
  } catch {
    setLocale("en");
  }

  const [info, licenses] = await Promise.all([
    getAppInfo(),
    getThirdPartyLicenses(),
  ]);
  root.innerHTML = renderAboutWindow(info, licenses);
}

window.addEventListener("DOMContentLoaded", () => {
  void bootstrap();
});
