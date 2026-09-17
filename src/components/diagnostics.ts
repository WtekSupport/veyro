import type { DiagnosticsSnapshot } from "../api";

export function renderDiagnostics(diagnostics: DiagnosticsSnapshot | null): string {
  if (!diagnostics) {
    return `<p class="hint">Diagnostics unavailable.</p>`;
  }

  return `
    <section class="panel diagnostics">
      <h2>Diagnostics</h2>
      <ul class="diagnostics-list">
        <li><span>State</span><strong>${diagnostics.state.replaceAll("_", " ")}</strong></li>
        <li><span>Audio</span><strong>${diagnostics.audio_running ? "Running" : "Stopped"}</strong></li>
        <li><span>Running as admin</span><strong>${diagnostics.process_elevated ? "Yes" : "No"}</strong></li>
        <li><span>PTT blocked by elevation</span><strong>${diagnostics.hotkeys_blocked_by_elevation ? "Yes" : "No"}</strong></li>
        <li><span>Injection</span><strong>${diagnostics.injection_available ? diagnostics.injection_backend : "Unavailable"}</strong></li>
        <li><span>API key</span><strong>${diagnostics.has_api_key ? "Configured" : "Missing"}</strong></li>
        <li><span>Microphone</span><strong>${escapeHtml(diagnostics.microphone_device ?? "Default")}</strong></li>
        <li><span>Whisper in RAM</span><strong>${diagnostics.whisper_loaded ? "Yes" : "No"}</strong></li>
        <li><span>LLM in RAM</span><strong>${diagnostics.llm_loaded ? "Yes" : "No"}</strong></li>
        <li><span>Settings WebView</span><strong>${diagnostics.settings_webview_alive ? "Alive" : "Destroyed"}</strong></li>
        <li><span>About WebView</span><strong>${diagnostics.about_webview_alive ? "Alive" : "Destroyed"}</strong></li>
      </ul>
    </section>
  `;
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}
