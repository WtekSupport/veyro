import {
  downloadSileroTeModel,
  downloadSileroVadModel,
  getSileroTeModelStatus,
  getSileroVadModelStatus,
  recoverEngine,
} from "../api";
import { patchState, setError } from "../state";

/** Standard mode: Silero VAD + TE assets required for local on-device stack. */
export async function ensureHomemakerSileroAssets(): Promise<void> {
  const te = await getSileroTeModelStatus();
  if (!te.exists) {
    try {
      await downloadSileroTeModel();
      patchState({ sileroTeModel: await getSileroTeModelStatus() });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setError({ code: "silero_te_download", message });
      throw error;
    }
  }

  const vad = await getSileroVadModelStatus();
  if (!vad.exists) {
    try {
      await downloadSileroVadModel();
      patchState({ sileroVadModel: await getSileroVadModelStatus() });
      await recoverEngine();
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setError({ code: "silero_vad_download", message });
      throw error;
    }
  }
}
