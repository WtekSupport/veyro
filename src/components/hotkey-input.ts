import { t } from "../i18n";

const MODIFIER_CODES = new Set([
  "ControlLeft",
  "ControlRight",
  "ShiftLeft",
  "ShiftRight",
  "AltLeft",
  "AltRight",
  "MetaLeft",
  "MetaRight",
]);

const MODIFIER_ORDER = ["Ctrl", "Shift", "Alt", "Cmd"] as const;

const SPECIAL_KEYS: Record<string, string> = {
  Backquote: "`",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Tab: "Tab",
  Enter: "Enter",
  Backspace: "Backspace",
  Delete: "Delete",
  Insert: "Insert",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  CapsLock: "CapsLock",
  NumLock: "NumLock",
  ScrollLock: "ScrollLock",
};

const RELEASE_COMMIT_MS = 300;

interface CaptureSession {
  pressed: Set<string>;
  maxChord: Set<string>;
  releaseTimer: number | undefined;
  onKeyDown: (event: KeyboardEvent) => void;
  onKeyUp: (event: KeyboardEvent) => void;
}

const captureSessions = new WeakMap<HTMLButtonElement, CaptureSession>();

function codeToModifierName(code: string): string | null {
  if (code === "ControlLeft" || code === "ControlRight") {
    return "Ctrl";
  }
  if (code === "ShiftLeft" || code === "ShiftRight") {
    return "Shift";
  }
  if (code === "AltLeft" || code === "AltRight") {
    return "Alt";
  }
  if (code === "MetaLeft" || code === "MetaRight") {
    return "Cmd";
  }
  return null;
}

function codeToKeyName(code: string): string | null {
  if (code === "Space") {
    return "Space";
  }
  if (/^F\d+$/.test(code)) {
    return code;
  }
  if (code.startsWith("Key")) {
    return code.slice(3);
  }
  if (code.startsWith("Digit")) {
    return code.slice(5);
  }
  if (code.startsWith("Numpad")) {
    return code;
  }

  return SPECIAL_KEYS[code] ?? null;
}

function isTrackableCode(code: string): boolean {
  return MODIFIER_CODES.has(code) || codeToKeyName(code) !== null;
}

export function formatHotkeyFromCodes(codes: Iterable<string>): string | null {
  const modifierSet = new Set<string>();
  const keys: string[] = [];

  for (const code of codes) {
    const modifier = codeToModifierName(code);
    if (modifier) {
      modifierSet.add(modifier);
      continue;
    }

    const key = codeToKeyName(code);
    if (key) {
      keys.push(key);
    }
  }

  const modifiers = MODIFIER_ORDER.filter((name) => modifierSet.has(name));
  if (modifiers.length === 0 && keys.length === 0) {
    return null;
  }

  return [...modifiers, ...keys].join("+");
}

export function formatHotkeyFromKeyboardEvent(event: KeyboardEvent): string | null {
  if (event.key === "Escape") {
    return null;
  }

  const codes = new Set<string>();
  if (event.code && isTrackableCode(event.code)) {
    codes.add(event.code);
  }

  return formatHotkeyFromCodes(codes);
}

function findHiddenInput(button: HTMLButtonElement): HTMLInputElement | null {
  const wrap = button.closest(".hotkey-input-wrap");
  return wrap?.querySelector<HTMLInputElement>('input[name="global_hotkey"]') ?? null;
}

function setHotkeyValue(button: HTMLButtonElement, value: string): void {
  const hidden = findHiddenInput(button);
  if (hidden) {
    hidden.value = value;
  }
  button.textContent = value;
  button.dataset.value = value;
}

function stopCaptureSession(button: HTMLButtonElement): void {
  const session = captureSessions.get(button);
  if (!session) {
    return;
  }

  if (session.releaseTimer !== undefined) {
    window.clearTimeout(session.releaseTimer);
  }

  window.removeEventListener("keydown", session.onKeyDown, true);
  window.removeEventListener("keyup", session.onKeyUp, true);
  captureSessions.delete(button);
}

function commitCapture(
  button: HTMLButtonElement,
  session: CaptureSession,
  onChange?: () => void,
): void {
  const hotkey = formatHotkeyFromCodes(session.maxChord);
  stopCaptureSession(button);

  if (hotkey) {
    finishCapture(button, hotkey, onChange);
    return;
  }

  cancelCapture(button);
}

function updateCapturePreview(button: HTMLButtonElement, codes: Set<string>): void {
  const preview = formatHotkeyFromCodes(codes);
  button.textContent = preview ?? t("hotkey.pressKeys");
}

function cancelCapture(button: HTMLButtonElement): void {
  stopCaptureSession(button);
  const previous = button.dataset.previousValue ?? button.dataset.value ?? "";
  button.classList.remove("capturing");
  setHotkeyValue(button, previous);
  delete button.dataset.previousValue;
}

function finishCapture(
  button: HTMLButtonElement,
  value: string,
  onChange?: () => void,
): void {
  stopCaptureSession(button);
  button.classList.remove("capturing");
  delete button.dataset.previousValue;
  setHotkeyValue(button, value);
  button.blur();
  onChange?.();
}

function startCapture(button: HTMLButtonElement, onChange?: () => void): void {
  stopCaptureSession(button);
  button.dataset.previousValue = button.dataset.value ?? "";
  button.classList.add("capturing");
  button.textContent = t("hotkey.pressKeys");

  const session: CaptureSession = {
    pressed: new Set<string>(),
    maxChord: new Set<string>(),
    releaseTimer: undefined,
    onKeyDown: () => {},
    onKeyUp: () => {},
  };

  session.onKeyDown = (event: KeyboardEvent) => {
    if (!button.classList.contains("capturing")) {
      return;
    }

    if (session.releaseTimer !== undefined) {
      window.clearTimeout(session.releaseTimer);
      session.releaseTimer = undefined;
    }

    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      cancelCapture(button);
      button.blur();
      return;
    }

    if (event.repeat || !event.code || !isTrackableCode(event.code)) {
      return;
    }

    event.preventDefault();
    event.stopPropagation();

    session.pressed.add(event.code);
    if (session.pressed.size > session.maxChord.size) {
      session.maxChord = new Set(session.pressed);
      updateCapturePreview(button, session.maxChord);
    }
  };

  session.onKeyUp = (event: KeyboardEvent) => {
    if (!button.classList.contains("capturing") || !event.code) {
      return;
    }

    event.preventDefault();
    event.stopPropagation();

    session.pressed.delete(event.code);

    if (session.pressed.size === 0 && session.maxChord.size > 0) {
      session.releaseTimer = window.setTimeout(() => {
        session.releaseTimer = undefined;
        if (!button.classList.contains("capturing")) {
          return;
        }
        commitCapture(button, session, onChange);
      }, RELEASE_COMMIT_MS);
    }
  };

  captureSessions.set(button, session);
  window.addEventListener("keydown", session.onKeyDown, true);
  window.addEventListener("keyup", session.onKeyUp, true);
}

function bindHotkeyButton(button: HTMLButtonElement, onChange?: () => void): void {
  button.addEventListener("click", () => {
    button.focus();
  });

  button.addEventListener("focus", () => {
    startCapture(button, onChange);
  });

  button.addEventListener("blur", () => {
    if (button.classList.contains("capturing")) {
      cancelCapture(button);
    }
  });
}

export function bindHotkeyInputs(
  root: ParentNode = document,
  onChange?: () => void,
): void {
  root
    .querySelectorAll<HTMLButtonElement>("[data-hotkey-capture]")
    .forEach((button) => bindHotkeyButton(button, onChange));
}
