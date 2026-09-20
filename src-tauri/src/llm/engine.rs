use std::sync::{Arc, Mutex};

#[cfg(feature = "local-llm")]
use std::num::NonZeroU32;
#[cfg(feature = "local-llm")]
use std::thread::JoinHandle;
#[cfg(feature = "local-llm")]
use tracing::{info, warn};

use crate::llm::model_store::{needs_local_llm, selected_model_exists};
#[cfg(feature = "local-llm")]
use crate::llm::model_store::resolve_model_path;
#[cfg(feature = "local-llm")]
use crate::settings::local_llm_gpu_compiled;
use crate::settings::AppSettings;

#[cfg(feature = "local-llm")]
const DEFAULT_N_CTX: u32 = 4096;
#[cfg(feature = "local-llm")]
const GEC_N_CTX: u32 = 512;
const DEFAULT_MAX_TOKENS: u32 = 2048;
const GEC_MAX_TOKENS: u32 = 512;

#[derive(Debug, Clone)]
pub struct LlmCompletionParams {
    pub temperature: f32,
    pub max_tokens: u32,
    /// When true, pre-fill an empty reasoning block so Qwen3-style models skip thinking.
    pub disable_thinking: bool,
    /// Use the smaller GEC context window (512 tokens).
    pub use_gec_context: bool,
}

impl LlmCompletionParams {
    pub fn for_gec() -> Self {
        Self {
            temperature: 0.0,
            max_tokens: GEC_MAX_TOKENS,
            disable_thinking: false,
            use_gec_context: true,
        }
    }

    pub fn for_rewrite(temperature: f32) -> Self {
        Self {
            temperature,
            max_tokens: DEFAULT_MAX_TOKENS,
            disable_thinking: false,
            use_gec_context: false,
        }
    }

    pub fn for_optimization_rewrite(temperature: f32) -> Self {
        Self {
            temperature,
            max_tokens: 512,
            disable_thinking: true,
            use_gec_context: false,
        }
    }

    pub fn for_prewarm() -> Self {
        Self {
            temperature: 0.0,
            max_tokens: 8,
            disable_thinking: true,
            use_gec_context: false,
        }
    }

    #[cfg(feature = "local-llm")]
    pub fn context_n_ctx(&self) -> u32 {
        if self.use_gec_context {
            GEC_N_CTX
        } else {
            DEFAULT_N_CTX
        }
    }
}

#[derive(Clone)]
pub struct LlmEngine {
    inner: Arc<Mutex<LlmEngineState>>,
}

impl Drop for LlmEngine {
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) == 1 && self.is_ready() {
            self.unload();
        }
    }
}

enum LlmEngineState {
    Unloaded {
        reason: Option<String>,
    },
    #[cfg(feature = "local-llm")]
    Ready(LocalLlmWorker),
}

#[cfg(feature = "local-llm")]
struct LocalLlmWorker {
    tx: crossbeam_channel::Sender<WorkerCommand>,
    join: Mutex<Option<JoinHandle<()>>>,
}

#[cfg(feature = "local-llm")]
enum WorkerCommand {
    Complete {
        system: String,
        user: String,
        params: LlmCompletionParams,
        reply: tokio::sync::oneshot::Sender<Result<String, String>>,
    },
    Shutdown {
        reply: std::sync::mpsc::SyncSender<()>,
    },
}

#[cfg(feature = "local-llm")]
#[ouroboros::self_referencing]
struct RewriteSession {
    model: llama_cpp_2::model::LlamaModel,
    #[borrows(model)]
    #[not_covariant]
    context: llama_cpp_2::context::LlamaContext<'this>,
}

#[cfg(feature = "local-llm")]
static LLAMA_BACKEND: std::sync::OnceLock<llama_cpp_2::llama_backend::LlamaBackend> =
    std::sync::OnceLock::new();

#[cfg(feature = "local-llm")]
fn shared_llama_backend() -> Result<&'static llama_cpp_2::llama_backend::LlamaBackend, String> {
    use llama_cpp_2::llama_backend::LlamaBackend;
    use llama_cpp_2::LlamaCppError;

    if let Some(backend) = LLAMA_BACKEND.get() {
        return Ok(backend);
    }

    match LlamaBackend::init() {
        Ok(backend) => {
            let _ = LLAMA_BACKEND.set(backend);
            Ok(LLAMA_BACKEND.get().expect("llama backend initialized"))
        }
        Err(LlamaCppError::BackendAlreadyInitialized) => LLAMA_BACKEND.get().ok_or_else(|| {
            "llama backend marked initialized but singleton missing".to_string()
        }),
        Err(error) => Err(format!("llama backend init failed: {error}")),
    }
}

impl LlmEngine {
    pub fn unloaded(reason: Option<String>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(LlmEngineState::Unloaded { reason })),
        }
    }

    pub fn from_settings(settings: &AppSettings) -> Self {
        if !needs_local_llm(settings) {
            return Self::unloaded(Some(
                "local LLM is not required for current settings".to_string(),
            ));
        }

        if !cfg!(feature = "local-llm") {
            return Self::unloaded(Some("local LLM is not compiled into this build".to_string()));
        }

        if !selected_model_exists(settings) {
            return Self::unloaded(Some("selected local LLM model is not downloaded".to_string()));
        }

        #[cfg(feature = "local-llm")]
        {
            let model_path = match resolve_model_path(settings) {
                Ok(path) => path,
                Err(error) => {
                    return Self::unloaded(Some(error.to_string()));
                }
            };

            match Self::spawn_worker(model_path.clone(), settings.local_llm_use_gpu) {
                Ok(worker) => {
                    info!(
                        "local LLM loaded from {} (gpu={})",
                        model_path.display(),
                        settings.local_llm_use_gpu && local_llm_gpu_compiled()
                    );
                    Self {
                        inner: Arc::new(Mutex::new(LlmEngineState::Ready(worker))),
                    }
                }
                Err(error) => {
                    warn!("failed to load local LLM: {error}");
                    Self::unloaded(Some(error))
                }
            }
        }

        #[cfg(not(feature = "local-llm"))]
        {
            Self::unloaded(Some("local LLM is not compiled into this build".to_string()))
        }
    }

    pub fn ensure_loaded(settings: &AppSettings, engine: &Arc<std::sync::RwLock<LlmEngine>>) -> Result<(), String> {
        if !needs_local_llm(settings) {
            return Ok(());
        }

        if engine
            .read()
            .map(|guard| guard.is_ready())
            .unwrap_or(false)
        {
            return Ok(());
        }

        let loaded = LlmEngine::from_settings(settings);
        if !loaded.is_ready() {
            return Err(
                "local LLM is required but could not be loaded for the current settings".to_string(),
            );
        }

        if let Ok(mut guard) = engine.write() {
            *guard = loaded;
        }
        Ok(())
    }

    pub fn unload(&self) {
        let mut guard = match self.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };

        #[cfg(feature = "local-llm")]
        if let LlmEngineState::Ready(worker) = &*guard {
            worker.shutdown();
        }
        *guard = LlmEngineState::Unloaded {
            reason: Some("local LLM unloaded".to_string()),
        };
    }

    pub fn is_ready(&self) -> bool {
        self.inner.lock().ok().is_some_and(|state| match &*state {
            LlmEngineState::Unloaded { .. } => false,
            #[cfg(feature = "local-llm")]
            LlmEngineState::Ready(_) => true,
        })
    }

    pub async fn prewarm(&self) -> Result<(), String> {
        if !self.is_ready() {
            return Ok(());
        }

        self.complete("Reply with one word.", "OK", LlmCompletionParams::for_prewarm())
            .await?;
        #[cfg(feature = "local-llm")]
        info!("local LLM prewarmed");
        Ok(())
    }

    pub async fn complete(
        &self,
        system_prompt: &str,
        user_message: &str,
        params: LlmCompletionParams,
    ) -> Result<String, String> {
        #[cfg(feature = "local-llm")]
        {
            let worker = {
                let guard = self
                    .inner
                    .lock()
                    .map_err(|_| "local LLM engine lock poisoned".to_string())?;
                match &*guard {
                    LlmEngineState::Unloaded { reason } => {
                        return Err(reason
                            .clone()
                            .unwrap_or_else(|| "local LLM engine is not loaded".to_string()));
                    }
                    LlmEngineState::Ready(worker) => worker.clone_handle(),
                }
            };

            worker.complete(system_prompt, user_message, params).await
        }

        #[cfg(not(feature = "local-llm"))]
        {
            let _ = (system_prompt, user_message, params);
            let guard = self
                .inner
                .lock()
                .map_err(|_| "local LLM engine lock poisoned".to_string())?;
            match &*guard {
                LlmEngineState::Unloaded { reason } => Err(reason.clone().unwrap_or_else(|| {
                    "local LLM is not compiled into this build".to_string()
                })),
            }
        }
    }

    #[cfg(feature = "local-llm")]
    fn spawn_worker(model_path: std::path::PathBuf, use_gpu: bool) -> Result<LocalLlmWorker, String> {
        let (tx, rx) = crossbeam_channel::unbounded();
        let model_path_for_thread = model_path.clone();
        let join = std::thread::Builder::new()
            .name("veyro-llm".into())
            .spawn(move || worker_main(rx, model_path_for_thread, use_gpu))
            .map_err(|error| format!("failed to spawn local LLM worker: {error}"))?;

        Ok(LocalLlmWorker {
            tx,
            join: Mutex::new(Some(join)),
        })
    }
}

#[cfg(feature = "local-llm")]
impl LocalLlmWorker {
    fn clone_handle(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            join: Mutex::new(None),
        }
    }

    async fn complete(
        &self,
        system_prompt: &str,
        user_message: &str,
        params: LlmCompletionParams,
    ) -> Result<String, String> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(WorkerCommand::Complete {
                system: system_prompt.to_string(),
                user: user_message.to_string(),
                params,
                reply: reply_tx,
            })
            .map_err(|_| "local LLM worker channel closed".to_string())?;

        reply_rx
            .await
            .map_err(|_| "local LLM worker dropped the response".to_string())?
    }

    fn shutdown(&self) {
        let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
        if self
            .tx
            .send(WorkerCommand::Shutdown { reply: reply_tx })
            .is_ok()
        {
            let _ = reply_rx.recv_timeout(std::time::Duration::from_secs(120));
        }

        if let Ok(mut join) = self.join.lock() {
            if let Some(handle) = join.take() {
                let _ = handle.join();
            }
        }
    }
}

#[cfg(feature = "local-llm")]
fn worker_main(
    rx: crossbeam_channel::Receiver<WorkerCommand>,
    model_path: std::path::PathBuf,
    use_gpu: bool,
) {
    let mut session = match build_rewrite_session(&model_path, use_gpu) {
        Ok(session) => session,
        Err(error) => {
            warn!("local LLM worker failed to initialize: {error}");
            while let Ok(command) = rx.recv() {
                if let WorkerCommand::Complete { reply, .. } = command {
                    let _ = reply.send(Err(error.clone()));
                }
            }
            return;
        }
    };

    while let Ok(command) = rx.recv() {
        match command {
            WorkerCommand::Complete {
                system,
                user,
                params,
                reply,
            } => {
                let result = run_completion(&mut session, &system, &user, params);
                let _ = reply.send(result);
            }
            WorkerCommand::Shutdown { reply } => {
                let _ = reply.send(());
                break;
            }
        }
    }
}

#[cfg(feature = "local-llm")]
fn build_rewrite_session(
    model_path: &std::path::Path,
    prefer_gpu: bool,
) -> Result<RewriteSession, String> {
    if prefer_gpu && local_llm_gpu_compiled() {
        match try_build_rewrite_session(model_path, true) {
            Ok(session) => {
                info!("local LLM model loaded with GPU acceleration");
                return Ok(session);
            }
            Err(error) => {
                warn!("local LLM GPU load failed ({error}); retrying on CPU");
            }
        }
    }

    try_build_rewrite_session(model_path, false)
}

#[cfg(feature = "local-llm")]
fn try_build_rewrite_session(
    model_path: &std::path::Path,
    use_gpu: bool,
) -> Result<RewriteSession, String> {
    use llama_cpp_2::context::params::LlamaContextParams;
    use llama_cpp_2::model::params::LlamaModelParams;
    use llama_cpp_2::model::LlamaModel;

    let backend = shared_llama_backend()?;

    let model_params = if use_gpu {
        LlamaModelParams::default().with_n_gpu_layers(1000)
    } else {
        LlamaModelParams::default()
    };

    let model = LlamaModel::load_from_file(backend, model_path, &model_params)
        .map_err(|error| format!("failed to load GGUF model: {error}"))?;

    RewriteSession::try_new(
        model,
        |model| {
            let n_ctx = NonZeroU32::new(DEFAULT_N_CTX).expect("non-zero context");
            let ctx_params = LlamaContextParams::default().with_n_ctx(Some(n_ctx));
            model
                .new_context(shared_llama_backend()?, ctx_params)
                .map_err(|error| format!("failed to create llama context: {error}"))
        },
    )
    .map_err(|error| format!("failed to build llama session: {error}"))
}

#[cfg(feature = "local-llm")]
fn run_completion(
    session: &mut RewriteSession,
    system_prompt: &str,
    user_message: &str,
    params: LlmCompletionParams,
) -> Result<String, String> {
    use llama_cpp_2::context::params::LlamaContextParams;

    let n_ctx = params.context_n_ctx();

    if params.use_gec_context {
        let n_ctx_nonzero = NonZeroU32::new(n_ctx).expect("non-zero context");
        let ctx_params = LlamaContextParams::default().with_n_ctx(Some(n_ctx_nonzero));
        let model = session.borrow_model();
        let backend = shared_llama_backend()?;
        let mut ctx = model
            .new_context(backend, ctx_params)
            .map_err(|error| format!("failed to create GEC llama context: {error}"))?;
        return complete_with_context(
            model,
            &mut ctx,
            system_prompt,
            user_message,
            params,
            n_ctx,
        );
    }

    session.with_mut(|fields| {
        fields.context.clear_kv_cache();
        complete_with_context(
            fields.model,
            fields.context,
            system_prompt,
            user_message,
            params,
            n_ctx,
        )
    })
}

#[cfg(feature = "local-llm")]
fn complete_with_context(
    model: &llama_cpp_2::model::LlamaModel,
    ctx: &mut llama_cpp_2::context::LlamaContext<'_>,
    system_prompt: &str,
    user_message: &str,
    params: LlmCompletionParams,
    n_ctx: u32,
) -> Result<String, String> {
    use llama_cpp_2::llama_batch::LlamaBatch;
    use llama_cpp_2::model::LlamaChatMessage;
    use llama_cpp_2::sampling::LlamaSampler;

    let template = model
        .chat_template(None)
        .map_err(|error| format!("model chat template unavailable: {error}"))?;

    let messages = [
        LlamaChatMessage::new("system".to_string(), system_prompt.to_string())
            .map_err(|error| format!("invalid system message: {error}"))?,
        LlamaChatMessage::new("user".to_string(), user_message.to_string())
            .map_err(|error| format!("invalid user message: {error}"))?,
    ];

    let mut prompt = model
        .apply_chat_template(&template, &messages, true)
        .map_err(|error| format!("failed to apply chat template: {error}"))?;

    if params.disable_thinking {
        append_no_think_prefill(&mut prompt);
    }

    let tokens = model
        .str_to_token(&prompt, llama_cpp_2::model::AddBos::Always)
        .map_err(|error| format!("failed to tokenize prompt: {error}"))?;

    if tokens.len() as u32 >= n_ctx {
        return Err(format!(
            "prompt exceeds local LLM context window ({n_ctx} tokens)"
        ));
    }

    let mut batch = LlamaBatch::new(n_ctx as usize, 1);
    let mut n_cur = decode_prompt_in_batches(ctx, &mut batch, &tokens)?;

    let seed = 1234;
    let mut sampler = if params.temperature <= 0.0 {
        LlamaSampler::chain_simple([LlamaSampler::greedy()])
    } else {
        LlamaSampler::chain_simple([
            LlamaSampler::dist(seed),
            LlamaSampler::temp(params.temperature),
            LlamaSampler::greedy(),
        ])
    };

    let mut decoder = encoding_rs::UTF_8.new_decoder();
    let mut output = String::new();

    for _ in 0..params.max_tokens {
        let token = sampler.sample(ctx, batch.n_tokens() - 1);
        sampler.accept(token);

        if model.is_eog_token(token) {
            break;
        }

        let piece = model
            .token_to_piece(token, &mut decoder, true, None)
            .map_err(|error| format!("failed to decode token: {error}"))?;
        output.push_str(&piece);

        batch.clear();
        batch
            .add(token, n_cur, &[0], true)
            .map_err(|error| format!("failed to append generated token: {error}"))?;
        n_cur += 1;

        ctx.decode(&mut batch)
            .map_err(|error| format!("llama decode failed during generation: {error}"))?;
    }

    Ok(output.trim().to_string())
}

#[cfg(feature = "local-llm")]
fn decode_prompt_in_batches(
    ctx: &mut llama_cpp_2::context::LlamaContext<'_>,
    batch: &mut llama_cpp_2::llama_batch::LlamaBatch,
    tokens: &[llama_cpp_2::token::LlamaToken],
) -> Result<i32, String> {
    let n_batch = ctx.n_batch().max(1) as usize;
    let prompt_len = tokens.len();
    let last_prompt_index = prompt_len.saturating_sub(1);
    let mut processed = 0usize;

    while processed < prompt_len {
        let chunk_end = (processed + n_batch).min(prompt_len);
        batch.clear();
        for (chunk_idx, token) in tokens[processed..chunk_end].iter().enumerate() {
            let global_index = processed + chunk_idx;
            batch
                .add(
                    *token,
                    global_index as i32,
                    &[0],
                    global_index == last_prompt_index,
                )
                .map_err(|error| format!("failed to build llama batch: {error}"))?;
        }
        ctx.decode(batch)
            .map_err(|error| format!("llama decode failed: {error}"))?;
        processed = chunk_end;
    }

    Ok(prompt_len as i32)
}

#[cfg(feature = "local-llm")]
fn append_no_think_prefill(prompt: &mut String) {
    const THINK_OPEN: &str = concat!("<", "think", ">");
    const THINK_CLOSE: &str = concat!("<", "/", "think", ">");
    const EMPTY_THINK_BLOCK: &str = concat!("\n\n<", "think", ">\n\n<", "/", "think", ">\n\n");

    if prompt.trim_end().ends_with(THINK_CLOSE) {
        return;
    }

    if let Some(open_idx) = prompt.rfind(THINK_OPEN) {
        let tail = &prompt[open_idx + THINK_OPEN.len()..];
        if !tail.contains(THINK_CLOSE) {
            prompt.push_str(THINK_CLOSE);
            return;
        }
    }

    prompt.push_str(EMPTY_THINK_BLOCK);
}

#[cfg(all(test, feature = "local-llm"))]
mod tests {
    use super::append_no_think_prefill;

    const THINK_OPEN: &str = concat!("<", "think", ">");
    const THINK_CLOSE: &str = concat!("<", "/", "think", ">");

    #[test]
    fn appends_empty_think_block_when_missing() {
        let mut prompt = "<|im_start|>assistant\n".to_string();
        append_no_think_prefill(&mut prompt);
        assert!(prompt.contains(THINK_OPEN));
        assert!(prompt.contains(THINK_CLOSE));
    }

    #[test]
    fn closes_open_think_block_without_duplicating() {
        let mut prompt = format!("<|im_start|>assistant\n{THINK_OPEN}\n");
        append_no_think_prefill(&mut prompt);
        assert!(prompt.ends_with(THINK_CLOSE));
        assert_eq!(prompt.matches(THINK_OPEN).count(), 1);
    }
}
