import { t, type MessageKey } from "../i18n";
import { en } from "../i18n/locales/en";

const TOOL_ERROR_KEY = /^tools\.[a-zA-Z0-9.]+/;

export function parseToolErrorRaw(raw: string): { key: MessageKey | null; technical: string } {
  const trimmed = raw.trim();
  if (!trimmed) {
    return { key: null, technical: "" };
  }

  const pipe = trimmed.indexOf("|");
  if (pipe > 0 && TOOL_ERROR_KEY.test(trimmed)) {
    const key = trimmed.slice(0, pipe).trim() as MessageKey;
    return { key, technical: trimmed.slice(pipe + 1).trim() };
  }

  const colon = trimmed.indexOf(":");
  if (colon > 0 && trimmed.startsWith("tools.")) {
    const maybeKey = trimmed.slice(0, colon).trim();
    if (TOOL_ERROR_KEY.test(maybeKey) && maybeKey in en) {
      return {
        key: maybeKey as MessageKey,
        technical: trimmed.slice(colon + 1).trim(),
      };
    }
  }

  if (TOOL_ERROR_KEY.test(trimmed) && trimmed in en) {
    return { key: trimmed as MessageKey, technical: "" };
  }

  return { key: null, technical: trimmed };
}

function detailKeyFor(messageKey: MessageKey): MessageKey {
  return `${messageKey}.detail` as MessageKey;
}

function hasMessageKey(key: string): key is MessageKey {
  return key in en;
}

/** Full error text for the result textarea when a tool job failed. */
export function formatToolErrorForResultField(
  raw: string,
  defaultFailedKey: MessageKey,
): string {
  const { key, technical } = parseToolErrorRaw(raw);
  const messageKey = key ?? defaultFailedKey;
  const lines: string[] = [
    t(messageKey),
  ];

  const detailKey = detailKeyFor(messageKey);
  if (hasMessageKey(detailKey)) {
    lines.push("", t(detailKey));
  }

  if (technical) {
    lines.push("", `${t("tools.error.technicalDetail")}: ${technical}`);
  } else if (!key && raw.trim()) {
    lines.push("", raw.trim());
  }

  return lines.join("\n");
}

export function toolErrorMessageKey(raw: string, defaultFailedKey: MessageKey): MessageKey {
  return parseToolErrorRaw(raw).key ?? defaultFailedKey;
}
