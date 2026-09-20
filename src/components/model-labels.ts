import type { LlmModelKind, LocalSttModelKind } from "../api";
import { t } from "../i18n";

const LOCAL_STT_MODEL_NAME_KEYS = {
  base: "settings.whisperModelNameBase",
  small: "settings.whisperModelNameSmall",
  medium: "settings.whisperModelNameMedium",
  large_v3_turbo: "settings.whisperModelNameLargeV3Turbo",
  large_v3: "settings.whisperModelNameLargeV3",
  parakeet_tdt_0_6b_v3: "settings.sttModelNameParakeetTdt06bV3",
  qwen3_asr_0_6b: "settings.sttModelNameQwen3Asr06b",
  qwen3_asr_1_7b: "settings.sttModelNameQwen3Asr17b",
} as const satisfies Record<LocalSttModelKind, string>;

/** Concrete product names for Status plates (not short settings labels). */
const LOCAL_STT_PLATE_NAME_KEYS = {
  base: "status.modelWhisperBase",
  small: "status.modelWhisperSmall",
  medium: "status.modelWhisperMedium",
  large_v3_turbo: "status.modelWhisperTurbo",
  large_v3: "status.modelWhisperLargeV3",
  parakeet_tdt_0_6b_v3: "settings.sttModelNameParakeetTdt06bV3",
  qwen3_asr_0_6b: "settings.sttModelNameQwen3Asr06b",
  qwen3_asr_1_7b: "settings.sttModelNameQwen3Asr17b",
} as const satisfies Record<LocalSttModelKind, string>;

const LLM_MODEL_NAME_KEYS = {
  qwen3_4b: "settings.llmModelNameQwen3_4B",
  t_lite_it21: "settings.llmModelNameTLiteIt21",
  qwen25_7b: "settings.llmModelNameQwen25_7B",
  gec08b: "settings.llmModelNameGec08B",
} as const satisfies Record<LlmModelKind, string>;

const LLM_PLATE_NAME_KEYS = {
  qwen3_4b: "status.modelLlmQwen3_4B",
  t_lite_it21: "status.modelLlmTLiteIt21",
  qwen25_7b: "status.modelLlmQwen25_7B",
  gec08b: "status.modelLlmGec08B",
} as const satisfies Record<LlmModelKind, string>;

export function localSttModelDisplayName(kind: LocalSttModelKind): string {
  return t(LOCAL_STT_MODEL_NAME_KEYS[kind]);
}

export function localSttModelPlateName(kind: LocalSttModelKind): string {
  return t(LOCAL_STT_PLATE_NAME_KEYS[kind]);
}

export function llmModelDisplayName(kind: LlmModelKind): string {
  return t(LLM_MODEL_NAME_KEYS[kind]);
}

export function llmModelPlateName(kind: LlmModelKind): string {
  return t(LLM_PLATE_NAME_KEYS[kind]);
}
