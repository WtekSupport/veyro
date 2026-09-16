import type { UiMode } from "../api";
import { subscribeLocale, t } from "../i18n";
import type { MessageKey } from "../i18n/locales/en";

const EXPERT_TAGLINE_KEYS: MessageKey[] = [
  "app.tagline.1",
  "app.tagline.2",
  "app.tagline.3",
  "app.tagline.4",
  "app.tagline.5",
  "app.tagline.6",
  "app.tagline.7",
  "app.tagline.8",
  "app.tagline.9",
];

const STANDARD_TAGLINE_KEYS: MessageKey[] = [
  "app.tagline.standard.1",
  "app.tagline.standard.2",
  "app.tagline.standard.3",
  "app.tagline.standard.4",
  "app.tagline.standard.5",
  "app.tagline.standard.6",
];

const ROTATE_MS = 5000;
const FADE_MS = 500;

let timer: ReturnType<typeof setInterval> | undefined;
let localeUnsub: (() => void) | undefined;
let mountedHost: HTMLElement | null = null;
let mountedMode: UiMode | null = null;
let index = 0;
let taglineKeys: MessageKey[] = EXPERT_TAGLINE_KEYS;

function keysForMode(mode: UiMode): MessageKey[] {
  return mode === "homemaker" ? STANDARD_TAGLINE_KEYS : EXPERT_TAGLINE_KEYS;
}

function resetTagline(host: HTMLElement): void {
  const line = host.querySelector<HTMLElement>("[data-tagline]");
  if (!line) {
    return;
  }

  index = Math.floor(Math.random() * taglineKeys.length);
  host.classList.remove("is-fading");
  line.textContent = t(taglineKeys[index]);
}

export function mountRotatingTagline(host: HTMLElement | null, mode: UiMode = "homemaker"): void {
  if (host && host === mountedHost && mode === mountedMode) {
    return;
  }

  stopRotatingTagline();
  mountedHost = host;
  mountedMode = mode;
  taglineKeys = keysForMode(mode);

  if (!host) {
    return;
  }

  resetTagline(host);

  const line = host.querySelector<HTMLElement>("[data-tagline]");
  if (!line) {
    return;
  }

  const rotate = (): void => {
    host.classList.add("is-fading");
    window.setTimeout(() => {
      index = (index + 1) % taglineKeys.length;
      line.textContent = t(taglineKeys[index]);
      host.classList.remove("is-fading");
    }, FADE_MS);
  };

  timer = setInterval(rotate, ROTATE_MS);
  localeUnsub = subscribeLocale(() => {
    resetTagline(host);
  });
}

export function stopRotatingTagline(): void {
  if (timer) {
    clearInterval(timer);
    timer = undefined;
  }

  localeUnsub?.();
  localeUnsub = undefined;
  mountedHost = null;
  mountedMode = null;
}
