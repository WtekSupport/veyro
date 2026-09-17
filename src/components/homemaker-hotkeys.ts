export const FALLBACK_HOMEMAKER_HOTKEY_PRESETS = ["CapsLock", "ScrollLock"] as const;

export function resolveHomemakerHotkeySelection(
  currentHotkey: string,
  capslockPtt: boolean,
  presets: readonly string[],
): string {
  if (capslockPtt && presets.includes("CapsLock")) {
    return "CapsLock";
  }

  if (presets.includes(currentHotkey)) {
    return currentHotkey;
  }

  return presets[0] ?? FALLBACK_HOMEMAKER_HOTKEY_PRESETS[0];
}
