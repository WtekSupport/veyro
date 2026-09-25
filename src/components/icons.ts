import {
  AudioLines,
  AudioWaveform,
  Bell,
  Bot,
  Copy,
  Download,
  FolderOpen,
  KeyRound,
  Mic,
  MicAudioLines,
  Plus,
  Settings,
  Speech,
  TextAlignStart,
  TextCursorInput,
  Trash2,
  WandSparkles,
  Wrench,
  Zap,
  type IconNode,
} from "lucide";
import { lucideIcon } from "./lucide-icon";

function icon(node: IconNode): string {
  return lucideIcon(node);
}

export function iconFolder(): string {
  return icon(FolderOpen);
}

export function iconImport(): string {
  return icon(Download);
}

export function iconCopy(): string {
  return icon(Copy);
}

export function iconPlus(): string {
  return icon(Plus);
}

export function iconTrash(): string {
  return icon(Trash2);
}

export function iconSettings(): string {
  return icon(Settings);
}

export function iconTools(): string {
  return icon(Wrench);
}

/** Audio pipeline (capture / playback). */
export function iconDiagAudio(): string {
  return icon(AudioLines);
}

/** Text injection into the focused window. */
export function iconDiagInject(): string {
  return icon(TextCursorInput);
}

/** OpenAI / API key configured. */
export function iconDiagKey(): string {
  return icon(KeyRound);
}

/** Microphone device selected. */
export function iconDiagMic(): string {
  return icon(Mic);
}

/** Post-processing callbacks enabled. */
export function iconDiagCallbacks(): string {
  return icon(Zap);
}

/** Local speech model loaded (Whisper / sherpa). */
export function iconDiagWhisper(): string {
  return icon(MicAudioLines);
}

/** Voice activity detection (continuous mode). */
export function iconDiagVad(): string {
  return icon(AudioWaveform);
}

/** Text optimization / custom skill mode. */
export function iconDiagText(): string {
  return icon(TextAlignStart);
}

/** Speech-to-text provider active. */
export function iconDiagStt(): string {
  return icon(Speech);
}

/** Text rewrite provider active. */
export function iconDiagRewrite(): string {
  return icon(WandSparkles);
}

/** Local LLM loaded for rewrite. */
export function iconDiagLlm(): string {
  return icon(Bot);
}

/** System toast notifications enabled in settings. */
export function iconDiagNotify(): string {
  return icon(Bell);
}
