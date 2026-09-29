import logoUrl from "../assets/logo.png";

import type { AppInfo, ThirdPartyLicense } from "../api";
import { t } from "../i18n";

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

/** Local / downloadable models shown in About (collapsed, like third-party). */
const LOCAL_MODELS: ReadonlyArray<{ name: string; detail: string }> = [
  {
    name: "Whisper (GGML)",
    detail: "ggerganov/whisper.cpp — base / small / medium / large-v3-turbo / large-v3",
  },
  {
    name: "Parakeet TDT 0.6B v3",
    detail: "k2-fsa sherpa-onnx (INT8); Yiivgeny HF packs (FP16 / FP32)",
  },
  {
    name: "Qwen3-ASR 0.6B / 1.7B",
    detail: "k2-fsa sherpa-onnx asr-models (INT8)",
  },
  {
    name: "Qwen3-4B-Instruct (GGUF)",
    detail: "bartowski/Qwen_Qwen3-4B-Instruct-2507-GGUF",
  },
  {
    name: "T-lite-it-2.1 (GGUF)",
    detail: "t-tech/T-lite-it-2.1-GGUF",
  },
  {
    name: "Qwen2.5-7B-Instruct (GGUF)",
    detail: "bartowski/Qwen2.5-7B-Instruct-GGUF",
  },
  {
    name: "Qwen3.5-0.8B-GEC (GGUF)",
    detail: "loqira/Qwen3.5-0.8B-GEC-KAZ-RUS-ENG",
  },
  {
    name: "Silero VAD",
    detail: "snakers4/silero-vad (ONNX)",
  },
  {
    name: "Silero TE",
    detail: "snakers4/silero-models — text enhancement",
  },
  {
    name: "Google ReFormer (vocal separation)",
    detail: "musetric/vocal-separation-roformer-onnx — quality / speed profiles",
  },
  {
    name: "Hybrid Transformer Demucs (vocal separation)",
    detail: "StemSplitio/htdemucs-ft-vocals-onnx",
  },
];

function renderLocalModels(): string {
  const items = LOCAL_MODELS.map(
    (entry) => `
        <li class="about-license-item">
          <div class="about-license-name">${escapeHtml(entry.name)}</div>
          <div class="about-license-meta">${escapeHtml(entry.detail)}</div>
        </li>
      `,
  ).join("");
  return `
    <details class="about-licenses">
      <summary class="about-licenses-summary">${escapeHtml(t("about.modelsTitle"))}</summary>
      <ul class="about-licenses-list">${items}</ul>
    </details>
  `;
}

function renderThirdPartyLicenses(licenses: ThirdPartyLicense[]): string {
  if (licenses.length === 0) {
    return "";
  }

  const items = licenses
    .map(
      (entry) => `
        <li class="about-license-item">
          <div class="about-license-name">${escapeHtml(entry.name)}</div>
          <div class="about-license-meta">${escapeHtml(entry.license)}</div>
          <div class="about-license-copy">${escapeHtml(entry.copyright)}</div>
        </li>
      `,
    )
    .join("");

  return `
    <details class="about-licenses">
      <summary class="about-licenses-summary">${escapeHtml(t("about.thirdPartyTitle"))}</summary>
      <ul class="about-licenses-list">${items}</ul>
    </details>
  `;
}

export function renderAboutWindow(info: AppInfo, licenses: ThirdPartyLicense[]): string {
  return `
    <main class="about-window">
      <div class="about-brand">
        <img class="about-logo" src="${logoUrl}" alt="${escapeHtml(t("app.title"))}" width="72" height="72" />
        <h1 class="about-title">${escapeHtml(t("app.title"))}</h1>
        <p class="about-version">${escapeHtml(t("about.version", { version: info.version_display }))}</p>
      </div>

      <section class="about-section">
        <h2 class="about-section-title">${escapeHtml(t("about.licenseTitle"))}</h2>
        <p class="about-body">${escapeHtml(t("about.licenseBody"))}</p>
      </section>

      ${renderLocalModels()}

      ${renderThirdPartyLicenses(licenses)}

      <section class="about-section">
        <h2 class="about-section-title">${escapeHtml(t("about.publisherTitle"))}</h2>
        <p class="about-body">${escapeHtml(info.publisher)}</p>
      </section>

      <section class="about-section">
        <h2 class="about-section-title">${escapeHtml(t("about.copyrightTitle"))}</h2>
        <p class="about-body">${escapeHtml(info.copyright)}</p>
      </section>

      <section class="about-section about-section--muted">
        <p class="about-body about-disclaimer">${escapeHtml(t("about.disclaimer"))}</p>
      </section>

      <footer class="about-hwid">
        <span class="about-hwid-label">${escapeHtml(t("about.hwidTitle"))}</span>
        <code class="about-hwid-hash">${escapeHtml(info.hwid_hash)}</code>
      </footer>
    </main>
  `;
}
