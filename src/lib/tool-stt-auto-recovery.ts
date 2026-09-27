import { promptSttLanguageSelection } from "../components/stt-language-dialog";
import { updateSettings } from "../api";

export const STT_SELECT_LANGUAGE_ERROR = "tools.stt.selectLanguage";

export function isSttSelectLanguageError(error: unknown): boolean {
  const raw = error instanceof Error ? error.message : String(error);
  return raw === STT_SELECT_LANGUAGE_ERROR;
}

/**
 * When STT is on Auto and recognition fails, ask for a fixed language, save it, and retry once.
 */
export async function runToolWithSttLanguageRecovery<T>(
  run: () => Promise<T>,
): Promise<T> {
  try {
    return await run();
  } catch (error) {
    if (!isSttSelectLanguageError(error)) {
      throw error;
    }
    const language = await promptSttLanguageSelection();
    if (!language) {
      throw error;
    }
    await updateSettings({ language });
    return await run();
  }
}
