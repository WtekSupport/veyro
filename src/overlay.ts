import { listen } from "@tauri-apps/api/event";

import { EVENTS, getSettings } from "./api";
import { setLocale, t } from "./i18n";

document.documentElement.style.background = "transparent";
document.body.style.background = "transparent";

const recEl = document.querySelector<HTMLElement>("[data-overlay-rec]");
const recLabelEl = document.querySelector<HTMLElement>("[data-overlay-rec-label]");
const listeningDotsEl = document.querySelector<HTMLElement>("[data-overlay-listening-dots]");

function applyRecLabel(): void {
  if (recLabelEl) {
    recLabelEl.textContent = t("overlay.recLabel");
  }
}

function setListening(active: boolean): void {
  if (recEl) {
    recEl.hidden = !active;
    recEl.setAttribute("aria-hidden", active ? "false" : "true");
  }
  if (listeningDotsEl) {
    listeningDotsEl.hidden = !active;
    listeningDotsEl.setAttribute("aria-hidden", active ? "false" : "true");
  }
}

setListening(true);

void getSettings()
  .then((settings) => {
    setLocale(settings.ui_locale ?? "en");
    applyRecLabel();
  })
  .catch(() => {
    applyRecLabel();
  });

void listen<{ active: boolean }>(EVENTS.overlayListening, (event) => {
  setListening(Boolean(event.payload.active));
});

void listen(EVENTS.listeningStarted, () => {
  setListening(true);
});

void listen(EVENTS.listeningStopped, () => {
  setListening(false);
});

void listen(EVENTS.transcriptionPartialClear, () => {
  setListening(false);
});
