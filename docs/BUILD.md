# Veyro — Сборка

English summary: [BUILD.en.md](BUILD.en.md).

## Философия

**Любая стандартная сборка включает весь доступный локальный функционал:**

- локальный Whisper (распознавание речи);
- локальный LLM (режим «Оптимизация (ИИ)» и custom skills);
- GPU-ускорение — **автоматически**, если на машине есть Vulkan SDK (Windows/Linux), Metal (macOS) или CUDA (опционально).

Скрипты `npm run tauri:dev` и `npm run tauri:build` на Windows **не требуют** ручного выбора `--features`.  
Переменные окружения нужны только чтобы **отключить** части стека, а не чтобы «включить» базовые возможности.

| Команда (Windows) | Что собирается |
|-------------------|----------------|
| `npm run tauri:dev` | dev + полный локальный стек + лучший GPU backend |
| `npm run tauri:build` | release + NSIS + полный локальный стек + GPU |

Облачный режим (только OpenAI, без whisper.cpp / llama.cpp) — **исключение** для CI или отладки, когда локальные компоненты явно отключены (см. ниже).

### Версионирование

Каждая сборка (dev и prod) увеличивает номер билда: `1.7.98` → `1.7.99` → `1.8.0`.  
Билд: `0…99`, при переполнении +1 к minor, билд сбрасывается в `0`.  
Скрипт: `scripts/bump-version.mjs` (вызывается автоматически из `tauri:dev` / `tauri:build`).

---

## Требования

### Все платформы

- [Node.js](https://nodejs.org/) 18+ (рекомендуется 20+)
- [Rust](https://www.rust-lang.org/tools/install) (stable)
- npm

```powershell
cd d:\Develop\veyro
npm install
```

### Windows

- [WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)
- Visual Studio Build Tools — workload **Desktop development with C++**
- NSIS подтягивается Tauri при первой сборке
- **CMake 3.20+** — portable копия в `.tools/cmake` ставится скриптами автоматически
- **Vulkan SDK** — для GPU; portable в `.tools/vulkan` (или `vulkan_sdk.exe` в `.tools/` для bootstrap)

### macOS

- Xcode Command Line Tools
- CMake (`brew install cmake`)
- GPU: **Metal** (`local-whisper-metal`)

### Linux

- Зависимости Tauri + Vulkan (см. [Tauri prerequisites](https://tauri.app/start/prerequisites/))
- Быстрая установка: `./scripts/install-linux-dependencies.sh`

---

## Windows (основная платформа)

### Разработка

```powershell
npm run tauri:dev
```

На Windows dev **запускается с правами администратора** (UAC): так Veyro может вводить текст в приложения, которые сами работают «от имени администратора». Подтвердите запрос UAC (из Git Bash он часто всплывает отдельно — смотрите панель задач). Сборка идёт в `C:\veyro-target`.

Если после hot-reload появляется **LNK1120 / unresolved external** (например `AudioPipeline::start_capture_pipeline`), остановите dev и один раз очистите артефакты:

```powershell
cargo clean -p veyro --manifest-path src-tauri/Cargo.toml
npm run tauri:dev
```

Скрипт dev по умолчанию ставит `CARGO_INCREMENTAL=0` на Windows; для более быстрых пересборок можно `VEYRO_CARGO_INCREMENTAL=1`.

**Silero TE (libtorch):** `npm run tauri:dev` / `tauri:build` вызывают `scripts/stage-libtorch-dlls.ps1` (скачивает Intel MKL redist при первом запуске, кладёт DLL рядом с `veyro.exe` и в `debug/deps/`). Вручную: `powershell -File scripts/stage-libtorch-dlls.ps1 -RepoRoot . -Profile debug`. Сборка с `silero-te` также копирует DLL из `src-tauri/binaries/` через `build.rs`. Если видите **Intel MKL FATAL ERROR** (`mkl_avx512.1.dll` / `mkl_def.1.dll`), убедитесь что `CARGO_TARGET_DIR=C:\veyro-target` (не другой каталог) и перезапустите dev после staging.

### Release + NSIS-инсталлятор

```powershell
npm run tauri:build
```

Скрипт автоматически:

1. Увеличивает версию (`bump-version.mjs`)
2. Выбирает features: `local-whisper-vulkan,local-llm-vulkan` (если есть Vulkan SDK)
3. Собирает llama.cpp, стейджит DLL, синхронизирует `tauri.windows.conf.json`
4. Создаёт portable EXE и NSIS installer

### Артефакты

| Файл | Путь |
|------|------|
| Portable EXE | `C:\veyro-target\release\veyro.exe` |
| NSIS installer | `C:\veyro-target\release\bundle\nsis\Veyro_<version>_x64-setup.exe` |
| Portable ZIP | `release\Veyro_x64-portable.zip` (если собран local-llm) |

Запуск release без установщика:

```powershell
npm run start:release
```

### Каталог сборки (Windows)

Dev и release **всегда** пишут артефакты в `C:\veyro-target` (без пробелов в пути — нужно для llama.cpp Vulkan).

Переопределение: `VEYRO_CARGO_TARGET_DIR` (не используйте пути с пробелами).

При ошибке `vulkan-shaders-gen` скрипт автоматически вызывает `finish-llama-cpp-build.ps1` и повторяет сборку.

### Ошибка «localhost refused to connect»

Открыт **debug** exe без Vite. Для production — установщик или `npm run start:release`.  
Для разработки — только `npm run tauri:dev`.

---

## Автовыбор GPU и features

При сборке скрипты выбирают **максимальный** backend:

| Приоритет | Windows / Linux | macOS |
|-----------|-----------------|-------|
| 1 | Vulkan (`local-whisper-vulkan` + `local-llm-vulkan`) | Metal (`local-whisper-metal`) + `local-llm` |
| 2 | CUDA (только при `VEYRO_ALLOW_CUDA=1`, NVIDIA) | — |
| 3 | CPU (`local-whisper` + `local-llm`) | CPU |

Portable Vulkan SDK: положите `vulkan_sdk.exe` в `.tools/` — распакуется в `.tools/vulkan`.

После первой GPU-сборки приложение **один раз** включает «Использовать GPU» в настройках (можно выключить вручную).

### Cargo features

| Feature | Назначение |
|---------|------------|
| `local-whisper` | Локальный Whisper (CPU) |
| `local-whisper-vulkan` | Whisper + Vulkan GPU |
| `local-whisper-cuda` | Whisper + CUDA GPU |
| `local-whisper-metal` | Whisper + Metal (macOS) |
| `local-llm` | Локальный LLM (CPU) |
| `local-llm-vulkan` | LLM + Vulkan GPU |
| `local-llm-cuda` | LLM + CUDA GPU |
| `local-sherpa-stt` | Parakeet / Qwen3-ASR (sherpa-onnx, CPU) |
| `local-sherpa-directml` | sherpa + DirectML (Windows) |
| `local-sherpa-cuda` | sherpa + CUDA |

В `Cargo.toml` **нет default features** — полный локальный стек включают `npm run tauri:dev` / `tauri:build` через `resolve-local-features.ps1` (`--no-default-features --features …`). GPU-варианты (Vulkan/DirectML) подбирает тот же скрипт.

Модели sherpa скачиваются по запросу (`.tar.bz2` из [релизов k2-fsa](https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models)) в `{models}/sherpa/…`. EP: CoreML (Apple Silicon), DirectML/CUDA при соответствующих features, иначе CPU.

---

## Отключение функционала (opt-out)

Переменные **отключают** части стека. Без них — полная сборка.

| Переменная | Эффект |
|------------|--------|
| `VEYRO_DISABLE_GPU=1` | Whisper и LLM на CPU (без Vulkan/CUDA) |
| `VEYRO_DISABLE_LOCAL_LLM=1` | Без локального LLM (нет «Оптимизация (ИИ)» offline) |
| `VEYRO_DISABLE_LOCAL_WHISPER=1` | Без локального Whisper (только OpenAI STT) |
| `VEYRO_DISABLE_SHERPA_STT=1` | Без sherpa-onnx (Parakeet / Qwen3); только Whisper локально |
| `VEYRO_DISABLE_VAD_SILERO=1` | Без Silero VAD (только WebRTC в детекторе речи) |
| `VEYRO_ALLOW_CUDA=1` | Разрешить авто-выбор CUDA вместо Vulkan (NVIDIA) |
| `VEYRO_CMAKE_PARALLEL` | Параллелизм cmake для llama.cpp (по умолчанию `1`) |
| `VEYRO_CARGO_TARGET_DIR` | Переопределить каталог сборки (по умолчанию `C:\veyro-target`) |

Устаревшие (не рекомендуются): `VEYRO_WHISPER_FEATURE`, `VEYRO_LLM_FEATURE` — жёстко задают feature; используйте opt-out флаги выше.

Примеры:

```powershell
# CPU-only, но локальный Whisper + LLM
$env:VEYRO_DISABLE_GPU = "1"
npm run tauri:build

# Только Whisper, без локального LLM
$env:VEYRO_DISABLE_LOCAL_LLM = "1"
npm run tauri:build

# Минимальная облачная сборка (OpenAI)
$env:VEYRO_DISABLE_LOCAL_WHISPER = "1"
$env:VEYRO_DISABLE_LOCAL_LLM = "1"
npm run tauri:build
```

---

## macOS

```bash
npm run tauri:dev:mac
CI=true npm run tauri:build:mac
```

По умолчанию: `local-whisper-metal,local-llm`. Opt-out — те же `VEYRO_DISABLE_*`.

Apple Silicon: автоматически `--target aarch64-apple-darwin`.

Подробности: DMG, подпись, разрешения, `CI=true` — см. исторические заметки в git или разделы ниже при необходимости.

---

## Linux

PowerShell-скрипты Windows не используются. Полный стек вручную:

```bash
npm run tauri dev -- --features local-whisper-vulkan,local-llm-vulkan
npm run tauri build -- --features local-whisper-vulkan,local-llm-vulkan
```

Нужны `libvulkan-dev`, `vulkan-tools`, драйверы. Проверка: `vulkaninfo --summary`.

---

## Модели

### Локальная модель речи (семейство + квантизация)

В настройках **Голос → Локальная модель речи** выбираются **семейство** (Whisper или Sherpa) и **квантизация** (Q4/Q5/Q8 для Whisper, INT8/FP16/FP32 для Parakeet). Характеристики (размер, RAM/VRAM, скорость) показываются под селектами.

| Семейство | Квант | Источник |
|-----------|-------|----------|
| Whisper * | Legacy | `ggml-{size}.bin` (HF ggerganov/whisper.cpp) |
| Whisper * | Q4/Q5/Q8 | `ggml-{size}-q4_0.bin`, `-q5_0`/`-q5_1`, `-q8_0` |
| Parakeet TDT 0.6B v3 | INT8 | k2-fsa `asr-models` (официально) |
| Parakeet TDT 0.6B v3 | FP16/FP32 | Hugging Face (Yiivgeny, third-party) |
| Qwen3-ASR 0.6B / 1.7B | INT8 | k2-fsa `asr-models` |

Sherpa-бандлы: `%AppData%\Veyro\models\sherpa\{family}\{int8|fp16|fp32}\`.  
Whisper-файлы: `%AppData%\Veyro\models\`.  
Старые установки Sherpa INT8 без подпапки quant по-прежнему распознаются.

Скачивание: **Download model** в том же блоке.

### LLM

Скачивание: **Advanced → локальная LLM → Download model**.  
Каталог задаётся в настройках (по умолчанию рядом с Whisper models).

---

## AI skills

Markdown в каталоге skills — system prompt для режима «Свой skill…».  
Пример: `example-business-ru.md`. Импорт через Advanced или копирование `.md` в папку skills.

---

## Полезные команды

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
npm run build
npx tauri icon src-tauri/icons/icon.png
```

---

## Переменные окружения (сводка)

| Переменная | Назначение |
|------------|------------|
| `RUST_LOG=debug` | Подробные логи |
| `VEYRO_DISABLE_GPU` | CPU вместо GPU |
| `VEYRO_DISABLE_LOCAL_LLM` | Без локального LLM |
| `VEYRO_DISABLE_LOCAL_WHISPER` | Без локального Whisper |
| `VEYRO_DISABLE_SHERPA_STT` | Без sherpa-onnx (Parakeet / Qwen3) |
| `VEYRO_DISABLE_VAD_SILERO` | Без Silero VAD (WebRTC-only) |
| `VEYRO_ALLOW_CUDA=1` | Auto-CUDA (NVIDIA) |
| `VULKAN_SDK` | Путь к Vulkan SDK |
| `CUDA_PATH` | Путь к CUDA Toolkit |
| `VEYRO_CARGO_TARGET_DIR` | Каталог сборки Rust (по умолчанию `C:\veyro-target`) |
| `TAURI_SIGNING_PRIVATE_KEY` | Подпись обновлений |

API key задаётся только через UI (OS keyring).

---

## CI

| Платформа | Команда |
|-----------|---------|
| Windows x64 | `npm run tauri:build` |
| macOS ARM | `CI=true npm run tauri:build:mac` |
| Linux x64 | `npm run tauri build -- --features local-whisper-vulkan,local-llm-vulkan` |

Каждая платформа собирается на своей ОС.
