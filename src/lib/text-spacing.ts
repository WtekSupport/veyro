/**
 * Keeps word boundaries when STT/rewrite chunks are merged for display.
 * Mirrors backend `ensure_spaces_after_punctuation` (sentence punctuation only).
 */
export function ensureSpacesAfterPunctuation(text: string): string {
  if (!text) {
    return text;
  }

  const chars = [...text];
  let result = "";

  for (let index = 0; index < chars.length; index += 1) {
    const ch = chars[index];
    result += ch;
    const next = chars[index + 1];
    if (next !== undefined && shouldAddSpaceAfter(chars, index)) {
      result += " ";
    }
  }

  return result;
}

function shouldAddSpaceAfter(chars: string[], index: number): boolean {
  const ch = chars[index];
  const next = chars[index + 1];
  if (next === undefined || /\s/.test(next)) {
    return false;
  }

  switch (ch) {
    case ".":
      return (
        !isDecimalPoint(chars, index) &&
        !isFilenameExtensionDot(chars, index) &&
        shouldSeparateFromNext(next)
      );
    case ":":
      return (
        !isTimeColon(chars, index) &&
        !isProtocolColon(chars, index) &&
        shouldSeparateFromNext(next)
      );
    case ",":
    case "!":
    case "?":
    case ";":
    case "…":
    case "—":
    case "–":
    case "»":
    case ")":
    case "]":
    case "‚":
    case "“":
      return shouldSeparateFromNext(next);
    default:
      return false;
  }
}

function shouldSeparateFromNext(next: string): boolean {
  return (
    /\p{L}|\p{N}/u.test(next) ||
    next === "«" ||
    next === "(" ||
    next === '"' ||
    next === "„" ||
    next === "—" ||
    next === "–"
  );
}

function isProtocolColon(chars: string[], index: number): boolean {
  if (chars[index] !== ":") {
    return false;
  }
  const before = chars.slice(0, index).join("");
  return before.endsWith("http") || before.endsWith("https") || before.endsWith("ftp");
}

function isTimeColon(chars: string[], index: number): boolean {
  if (chars[index] !== ":") {
    return false;
  }
  const prev = chars[index - 1];
  const next = chars[index + 1];
  return prev !== undefined && next !== undefined && /\d/.test(prev) && /\d/.test(next);
}

function isFilenameExtensionDot(chars: string[], index: number): boolean {
  if (chars[index] !== ".") {
    return false;
  }
  let ext = "";
  for (let i = index + 1; i < chars.length; i += 1) {
    const c = chars[i];
    if (!/[a-z]/.test(c)) {
      break;
    }
    ext += c;
  }
  if (ext.length < 2 || ext.length > 5) {
    return false;
  }
  for (let i = index - 1; i >= 0; i -= 1) {
    const c = chars[i];
    if (/[A-Za-z0-9_]/.test(c)) {
      return true;
    }
    if (!/\s/.test(c)) {
      return false;
    }
  }
  return false;
}

function isDecimalPoint(chars: string[], index: number): boolean {
  if (chars[index] !== ".") {
    return false;
  }
  const prev = chars[index - 1];
  const next = chars[index + 1];
  return prev !== undefined && next !== undefined && /\d/.test(prev) && /\d/.test(next);
}

/** Merge preview chunks without gluing words across punctuation boundaries. */
export function mergeTranscriptChunks(previous: string, incoming: string): string {
  const left = previous.trimEnd();
  const right = incoming.trimStart();
  if (!left) {
    return ensureSpacesAfterPunctuation(right);
  }
  if (!right) {
    return ensureSpacesAfterPunctuation(left);
  }
  return ensureSpacesAfterPunctuation(`${left}${right}`);
}
