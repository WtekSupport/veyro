import path from "node:path";
import { defineConfig } from "vite";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig(() => ({
  clearScreen: false,
  base: "./",
  build: {
    rollupOptions: {
      input: {
        main: path.resolve(__dirname, "index.html"),
        about: path.resolve(__dirname, "about.html"),
        overlay: path.resolve(__dirname, "overlay.html"),
        init: path.resolve(__dirname, "init.html"),
        skillImport: path.resolve(__dirname, "skill-import.html"),
        tools: path.resolve(__dirname, "tools.html"),
        toolVoiceFiles: path.resolve(__dirname, "tool-voice-files.html"),
        toolDictationTranscripts: path.resolve(__dirname, "tool-dictation-transcripts.html"),
        toolAudioSrt: path.resolve(__dirname, "tool-audio-srt.html"),
        toolVocalSeparator: path.resolve(__dirname, "tool-vocal-separator.html"),
        toolSpeechAnalysis: path.resolve(__dirname, "tool-speech-analysis.html"),
      },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
