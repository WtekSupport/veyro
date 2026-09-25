import { en, type MessageKey } from "./locales/en";

export type { MessageKey };
import { ru } from "./locales/ru";

export type UiLocale = "en" | "ru";

const locales = { en, ru } as const;

let currentLocale: UiLocale = "en";
const listeners = new Set<() => void>();

export function setLocale(locale: UiLocale): void {
  if (currentLocale === locale) {
    return;
  }
  currentLocale = locale;
  document.documentElement.lang = locale;
  listeners.forEach((listener) => listener());
}

export function getLocale(): UiLocale {
  return currentLocale;
}

export function subscribeLocale(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

type Params = Record<string, string | number | undefined | null>;

function resolveParams(
  template: string,
  params: Params,
  key: string,
): string {
  let result = template;
  for (const [name, value] of Object.entries(params)) {
    if (value === undefined || value === null) {
      continue;
    }
    if (name === "mode" && key.startsWith("activity.")) {
      const modeKey =
        String(value) === "ptt"
          ? "activity.mode.ptt"
          : "activity.mode.continuous";
      result = result.replaceAll(`{mode}`, t(modeKey as MessageKey));
      continue;
    }
    result = result.replaceAll(`{${name}}`, String(value));
  }

  return result;
}

export function t(key: MessageKey, params: Params = {}): string {
  const table = locales[currentLocale];
  const template = table[key] ?? en[key] ?? key;
  return resolveParams(template, params, key);
}

export function translateActivity(
  messageKey: string,
  messageArgs: Record<string, unknown> = {},
): string {
  const params: Params = {};
  for (const [name, value] of Object.entries(messageArgs)) {
    if (typeof value === "string" || typeof value === "number") {
      params[name] = value;
    }
  }

  const key = messageKey as MessageKey;
  if (key in en) {
    return t(key, params);
  }
  return messageKey;
}

export function translateError(code: string, fallback: string): string {
  const key = `errors.${code}` as MessageKey;
  if (key in en) {
    return t(key);
  }
  return fallback;
}

export function translateState(state: string): string {
  const key = `states.${state}` as MessageKey;
  if (key in en) {
    return t(key);
  }
  return state.replaceAll("_", " ");
}

export function whisperBackendLabel(backend: string): string {
  switch (backend) {
    case "vulkan":
      return t("settings.whisperBackendVulkan");
    case "cuda":
      return t("settings.whisperBackendCuda");
    case "metal":
      return t("settings.whisperBackendMetal");
    default:
      return t("settings.whisperBackendCpu");
  }
}

/** Short label for the active VAD engine (diagnostics or settings). */
export function vadEngineShortLabel(engine: string): string {
  if (engine === "webrtc") {
    return t("settings.vadEngineChipWebRtc");
  }
  return t("settings.vadEngineChipSilero");
}

export function vadEngineDiagLabel(diagnostics: {
  vad_engine: string;
  vad_silero_runtime_ok?: boolean;
}): string {
  const engine = diagnostics.vad_engine;
  let label = vadEngineShortLabel(engine);
  if (engine === "silero" && diagnostics.vad_silero_runtime_ok === false) {
    label = `${label} (${t("settings.vadEngineChipFallback")})`;
  }
  return label;
}
