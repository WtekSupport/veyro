use std::collections::HashSet;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::llm::engine::{LlmCompletionParams, LlmEngine};
use crate::network::openai_error::OpenAiError;
use crate::network::retry::RetryConfig;
use crate::settings::secrets;
use crate::settings::{AppSettings, LlmModelKind, TextProcessingMode, TextRewriteProvider, UiLocale};
use crate::text::elevated_speech::{contains_profanity, STRONGER_ELEVATION_SUFFIX};
use crate::text::gec_prompt::gec_system_prompt;
use crate::text::normalize::apply_basic_cleanup;
use crate::text::optimization_prompt::{
    format_optimization_user_message, format_protected_terms_section, optimization_system_prompt,
    STRONGER_OPTIMIZATION_SUFFIX,
};
use crate::text::skill::load_skill_body;

const OPENAI_CHAT_COMPLETIONS_URL: &str = "https://api.openai.com/v1/chat/completions";
const AI_REWRITE_MODEL: &str = "gpt-4.1-mini";
const OPTIMIZATION_TEMPERATURE: f32 = 0.15;
const OPTIMIZATION_RETRY_TEMPERATURE: f32 = 0.25;
const CUSTOM_SKILL_TEMPERATURE: f32 = 0.15;
const CUSTOM_SKILL_RETRY_TEMPERATURE: f32 = 0.25;
const TOP_P: f32 = 1.0;
const SIMILARITY_RETRY_THRESHOLD: f64 = 0.82;
const STRONGER_CUSTOM_SKILL_SUFFIX: &str = r#"Предыдущий результат был слишком близок к сырой диктовке. Перепиши СИЛЬНЕЕ, строго следуя инструкциям выше."#;

#[derive(Debug, thiserror::Error)]
pub enum RewriteError {
    #[error("{0}")]
    OpenAi(#[from] OpenAiError),
    #[error("{0}")]
    Local(String),
}

impl RewriteError {
    pub fn user_message(&self, locale: UiLocale) -> String {
        match self {
            Self::OpenAi(error) => error.user_message(locale),
            Self::Local(message) => message.clone(),
        }
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    temperature: f32,
    top_p: f32,
    messages: Vec<ChatMessage<'a>>,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Deserialize)]
struct ChatChoiceMessage {
    content: Option<String>,
}

struct RewriteRequestConfig {
    model: &'static str,
    temperature: f32,
    system_prompt: String,
    use_gec_params: bool,
}

pub struct RewriteOutcome {
    pub text: String,
    pub used_fallback: bool,
    pub fallback_reason: Option<String>,
}

pub async fn rewrite_transcription(
    raw: &str,
    mode: TextProcessingMode,
    settings: &AppSettings,
    ai_rewrite_skill: Option<&str>,
    http: &Client,
    llm_engine: &LlmEngine,
    protected_terms: &[String],
) -> Result<RewriteOutcome, RewriteError> {
    let ui_locale = settings.ui_locale;
    let result = match settings.text_rewrite_provider {
        TextRewriteProvider::Openai => {
            rewrite_with_openai(
                raw,
                mode,
                ai_rewrite_skill,
                settings.local_llm_model,
                settings.emulate_enter,
                protected_terms,
                http,
            )
            .await
        }
        TextRewriteProvider::Local => {
            rewrite_with_local_llm(
                raw,
                mode,
                ai_rewrite_skill,
                settings,
                llm_engine,
                protected_terms,
            )
            .await
        }
    };

    match result {
        Ok(text) => Ok(RewriteOutcome {
            text,
            used_fallback: false,
            fallback_reason: None,
        }),
        Err(error) => {
            warn!("text rewrite failed, falling back to basic cleanup: {error}");
            Ok(RewriteOutcome {
                text: apply_basic_cleanup(raw),
                used_fallback: true,
                fallback_reason: Some(error.user_message(ui_locale)),
            })
        }
    }
}

async fn rewrite_with_local_llm(
    raw: &str,
    mode: TextProcessingMode,
    ai_rewrite_skill: Option<&str>,
    settings: &AppSettings,
    llm_engine: &LlmEngine,
    protected_terms: &[String],
) -> Result<String, RewriteError> {
    let skill_body = if mode.requires_custom_skill() {
        Some(
            load_skill_body(ai_rewrite_skill)
                .map_err(|error| RewriteError::Local(error.to_string()))?
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| RewriteError::Local("custom skill is missing".to_string()))?,
        )
    } else {
        None
    };

    let config = rewrite_request_config(
        mode,
        skill_body.as_deref(),
        settings.local_llm_model,
        settings.emulate_enter,
        protected_terms,
    )?;
    let user_message = format_local_rewrite_user_message(raw, mode);
    let system_prompt = local_llm_system_prompt(&config.system_prompt, settings.local_llm_model);
    let mut completion_params = if config.use_gec_params {
        LlmCompletionParams::for_gec()
    } else if mode == TextProcessingMode::Optimization {
        LlmCompletionParams::for_optimization_rewrite(config.temperature)
    } else {
        LlmCompletionParams::for_rewrite(config.temperature)
    };
    if settings.local_llm_model.supports_hybrid_thinking() {
        completion_params.disable_thinking = true;
    }

    let first = llm_engine
        .complete(&system_prompt, &user_message, completion_params)
        .await
        .map_err(RewriteError::Local)?;
    let first = sanitize_rewrite_output(&first);
    let mut result = first.clone();

    if !config.use_gec_params
        && is_ai_rewrite_mode(mode)
        && needs_mode_retry(mode, raw, &first)
    {
        if mode == TextProcessingMode::Optimization {
            warn!("local LLM optimization rewrite off-contract, retrying with stronger prompt");
        } else {
            warn!("local LLM rewrite too close to raw dictation, retrying with stronger prompt");
        }
        let retry_config = RewriteRequestConfig {
            temperature: retry_temperature(mode),
            system_prompt: stronger_rewrite_prompt(mode, &config.system_prompt),
            model: config.model,
            use_gec_params: false,
        };
        let retry_system_prompt =
            local_llm_system_prompt(&retry_config.system_prompt, settings.local_llm_model);
        let mut retry_params = if mode == TextProcessingMode::Optimization {
            LlmCompletionParams::for_optimization_rewrite(retry_config.temperature)
        } else {
            LlmCompletionParams::for_rewrite(retry_config.temperature)
        };
        if settings.local_llm_model.supports_hybrid_thinking() {
            retry_params.disable_thinking = true;
        }
        let retry = llm_engine
            .complete(&retry_system_prompt, &user_message, retry_params)
            .await
            .map_err(RewriteError::Local)?;
        let retry = sanitize_rewrite_output(&retry);
        if rewrite_improved_for_mode(mode, raw, &first, &retry) {
            info!(
                "local LLM rewrite retry applied (raw {} chars -> {} chars)",
                raw.chars().count(),
                retry.chars().count()
            );
            result = retry;
        } else if token_jaccard_similarity(raw, &retry) > token_jaccard_similarity(raw, &first) {
            result = retry;
        }
    }

    let finalized = finalize_mode_output(mode, raw, &result);
    info!(
        "local text rewrite completed (raw {} chars -> {} chars)",
        raw.chars().count(),
        finalized.chars().count()
    );
    Ok(finalized)
}

async fn rewrite_with_openai(
    raw: &str,
    mode: TextProcessingMode,
    ai_rewrite_skill: Option<&str>,
    _local_model: LlmModelKind,
    emulate_enter: bool,
    protected_terms: &[String],
    http: &Client,
) -> Result<String, RewriteError> {
    let api_key = secrets::load_api_key().map_err(|_| OpenAiError::api_key_missing())?;
    let skill_body = if mode.requires_custom_skill() {
        Some(
            load_skill_body(ai_rewrite_skill)
                .map_err(|error| OpenAiError {
                    kind: crate::network::openai_error::OpenAiErrorKind::BadRequest,
                    status: None,
                    api_message: Some(error.to_string()),
                })?
                .filter(|value| !value.trim().is_empty())
                .ok_or(OpenAiError::custom_skill_missing())?,
        )
    } else {
        None
    };
    let config = rewrite_request_config(
        mode,
        skill_body.as_deref(),
        LlmModelKind::Qwen3_4B,
        emulate_enter,
        protected_terms,
    )?;
    let user_message = if mode == TextProcessingMode::Optimization {
        format_optimization_user_message(raw)
    } else {
        format_rewrite_user_message(raw)
    };

    let first = request_rewrite(http, &api_key, &config, &user_message).await?;
    let first = sanitize_rewrite_output(&first);
    let mut result = first.clone();

    if is_ai_rewrite_mode(mode) && needs_mode_retry(mode, raw, &first) {
        warn!("AI rewrite off-contract, retrying with stronger prompt");
        let retry_config = RewriteRequestConfig {
            temperature: retry_temperature(mode),
            system_prompt: stronger_rewrite_prompt(mode, &config.system_prompt),
            model: config.model,
            use_gec_params: false,
        };
        let retry = request_rewrite(http, &api_key, &retry_config, &user_message).await?;
        let retry = sanitize_rewrite_output(&retry);
        if rewrite_improved_for_mode(mode, raw, &first, &retry) {
            info!(
                "AI rewrite retry applied (raw {} chars -> {} chars)",
                raw.chars().count(),
                retry.chars().count()
            );
            result = retry;
        } else if token_jaccard_similarity(raw, &retry) > token_jaccard_similarity(raw, &first) {
            result = retry;
        }
    }

    let finalized = finalize_mode_output(mode, raw, &result);
    info!(
        "text rewrite completed (raw {} chars -> {} chars)",
        raw.chars().count(),
        finalized.chars().count()
    );
    Ok(finalized)
}

async fn request_rewrite(
    http: &Client,
    api_key: &str,
    config: &RewriteRequestConfig,
    user_message: &str,
) -> Result<String, RewriteError> {
    let retry = RetryConfig::default();
    let mut delay = retry.initial_delay_ms;

    for attempt in 0..retry.max_attempts {
        let request = ChatRequest {
            model: config.model,
            temperature: config.temperature,
            top_p: TOP_P,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: &config.system_prompt,
                },
                ChatMessage {
                    role: "user",
                    content: user_message,
                },
            ],
        };

        let response = http
            .post(OPENAI_CHAT_COMPLETIONS_URL)
            .bearer_auth(api_key)
            .json(&request)
            .send()
            .await;

        match response {
            Ok(response) => {
                if response.status().is_success() {
                    let payload: ChatResponse = response.json().await.map_err(|error| {
                        RewriteError::OpenAi(OpenAiError {
                            kind: crate::network::openai_error::OpenAiErrorKind::Network,
                            status: None,
                            api_message: Some(error.to_string()),
                        })
                    })?;

                    let text = payload
                        .choices
                        .first()
                        .and_then(|choice| choice.message.content.as_deref())
                        .map(str::trim)
                        .filter(|text| !text.is_empty())
                        .ok_or(RewriteError::OpenAi(OpenAiError::empty_response()))?
                        .to_string();

                    return Ok(text);
                }

                let status = response.status().as_u16();
                let body = response.text().await.unwrap_or_default();
                let parsed = OpenAiError::from_http(status, &body);
                if !parsed.retryable() || attempt + 1 >= retry.max_attempts {
                    return Err(RewriteError::OpenAi(parsed));
                }

                warn!(
                    "OpenAI rewrite attempt {} received retryable HTTP {status}: {}",
                    attempt + 1,
                    parsed.api_message.as_deref().unwrap_or("")
                );
            }
            Err(error) => {
                if attempt + 1 >= retry.max_attempts {
                    return Err(RewriteError::OpenAi(OpenAiError::from_network_exhausted(
                        &error,
                        retry.max_attempts,
                    )));
                }
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        delay = (delay * 2).min(retry.max_delay_ms);
    }

    Err(RewriteError::OpenAi(OpenAiError {
        kind: crate::network::openai_error::OpenAiErrorKind::Network,
        status: None,
        api_message: Some(format!("failed after {} attempts", retry.max_attempts)),
    }))
}

fn format_rewrite_user_message(raw: &str) -> String {
    raw.trim().to_string()
}

fn format_local_rewrite_user_message(raw: &str, mode: TextProcessingMode) -> String {
    if mode == TextProcessingMode::Optimization {
        format_optimization_user_message(raw)
    } else {
        format_rewrite_user_message(raw)
    }
}

fn local_llm_system_prompt(base: &str, model: LlmModelKind) -> String {
    if model.supports_hybrid_thinking() {
        format!("{base}\n/no_think")
    } else {
        base.to_string()
    }
}

fn strip_model_reasoning(text: &str) -> String {
    const TAG_PAIRS: [(&str, &str); 2] = [
        (concat!("<", "think", ">"), concat!("<", "/", "think", ">")),
        (
            concat!("<", "redacted_thinking", ">"),
            concat!("<", "/", "redacted_thinking", ">"),
        ),
    ];

    let mut cleaned = text.to_string();

    for (open, close) in TAG_PAIRS {
        while let Some(start) = cleaned.find(open) {
            if let Some(rel_end) = cleaned[start..].find(close) {
                let end = start + rel_end + close.len();
                cleaned.replace_range(start..end, "");
            } else {
                cleaned.truncate(start);
                break;
            }
        }
    }

    cleaned.trim().to_string()
}

fn sanitize_rewrite_output(text: &str) -> String {
    let mut cleaned = strip_model_reasoning(text);

    if (cleaned.starts_with('"') && cleaned.ends_with('"'))
        || (cleaned.starts_with('«') && cleaned.ends_with('»'))
    {
        cleaned = cleaned
            .trim_matches(|c| c == '"' || c == '«' || c == '»')
            .trim()
            .to_string();
    }

    for prefix in [
        "Исправленный текст:",
        "Отредактированный текст:",
        "Выход:",
        "Corrected:",
        "Edited:",
    ] {
        if cleaned
            .to_ascii_lowercase()
            .starts_with(&prefix.to_ascii_lowercase())
        {
            cleaned = cleaned[prefix.len()..].trim().to_string();
            break;
        }
    }

    cleaned
}

fn is_ai_rewrite_mode(mode: TextProcessingMode) -> bool {
    matches!(
        mode,
        TextProcessingMode::Optimization | TextProcessingMode::CustomSkill
    )
}

fn retry_temperature(mode: TextProcessingMode) -> f32 {
    match mode {
        TextProcessingMode::Optimization => OPTIMIZATION_RETRY_TEMPERATURE,
        TextProcessingMode::CustomSkill => CUSTOM_SKILL_RETRY_TEMPERATURE,
        TextProcessingMode::Original | TextProcessingMode::Basic => OPTIMIZATION_RETRY_TEMPERATURE,
    }
}

fn stronger_rewrite_prompt(mode: TextProcessingMode, base_prompt: &str) -> String {
    match mode {
        TextProcessingMode::Optimization => format!(
            "{base_prompt}\n\n{STRONGER_OPTIMIZATION_SUFFIX}\n\n{STRONGER_ELEVATION_SUFFIX}"
        ),
        TextProcessingMode::CustomSkill => format!(
            "{base_prompt}\n\n{STRONGER_CUSTOM_SKILL_SUFFIX}\n\n{STRONGER_ELEVATION_SUFFIX}"
        ),
        TextProcessingMode::Original | TextProcessingMode::Basic => base_prompt.to_string(),
    }
}

fn needs_mode_retry(mode: TextProcessingMode, input: &str, output: &str) -> bool {
    match mode {
        TextProcessingMode::Optimization => needs_optimization_retry(input, output),
        TextProcessingMode::CustomSkill => needs_stronger_rewrite(input, output),
        TextProcessingMode::Original | TextProcessingMode::Basic => false,
    }
}

fn needs_stronger_rewrite(input: &str, output: &str) -> bool {
    if contains_profanity(input) && contains_profanity(output) {
        return true;
    }

    if contains_profanity(input) && token_jaccard_similarity(input, output) > 0.65 {
        return true;
    }

    let similarity = token_jaccard_similarity(input, output);
    if similarity > SIMILARITY_RETRY_THRESHOLD {
        return true;
    }

    looks_like_raw_dictation(input) && similarity > 0.72
}

/// Optimization-only: retry when output is too raw, not expanded, or answers instead of editing.
fn needs_optimization_retry(input: &str, output: &str) -> bool {
    if optimization_answered_instead_of_editing(input, output) {
        return true;
    }

    let input_trim = input.trim();
    let output_trim = output.trim();
    if input_trim.is_empty() || output_trim.is_empty() {
        return false;
    }

    if optimization_is_verbatim_polish(input_trim, output_trim) {
        return false;
    }

    if contains_profanity(input_trim) && contains_profanity(output_trim) {
        return true;
    }

    if contains_profanity(input_trim)
        && token_jaccard_similarity(input_trim, output_trim) > 0.65
    {
        return true;
    }

    let similarity = token_jaccard_similarity(input_trim, output_trim);
    let input_words = input_trim.split_whitespace().count();
    let output_words = output_trim.split_whitespace().count();

    if similarity >= SIMILARITY_RETRY_THRESHOLD {
        return true;
    }

    if input_words >= 3 && output_words <= input_words {
        return true;
    }

    looks_like_raw_dictation(input_trim) && similarity > 0.72
}

fn finalize_mode_output(mode: TextProcessingMode, raw: &str, output: &str) -> String {
    if mode == TextProcessingMode::Optimization
        && optimization_answered_instead_of_editing(raw, output)
    {
        warn!("optimization output looks like an answer, using dictation fallback");
        apply_optimization_dictation_fallback(raw)
    } else {
        output.to_string()
    }
}

fn apply_optimization_dictation_fallback(raw: &str) -> String {
    let mut text = apply_basic_cleanup(raw);
    if text.is_empty() {
        return text;
    }

    if optimization_input_looks_like_question(raw) && !text.ends_with('?') {
        text.push('?');
    }

    text
}

fn optimization_is_verbatim_polish(input: &str, output: &str) -> bool {
    fn letters_only_lower(text: &str) -> String {
        text.to_lowercase()
            .chars()
            .filter(|ch| ch.is_alphanumeric())
            .collect()
    }

    letters_only_lower(input) == letters_only_lower(output)
}

fn optimization_input_looks_like_question(text: &str) -> bool {
    let lower = text.to_lowercase();
    if text.contains('?') {
        return true;
    }

    [
        "сколько",
        "какой",
        "какая",
        "какие",
        "как ",
        "когда",
        "где",
        "почему",
        "зачем",
        "кто",
        "what ",
        "how ",
        "when ",
        "where ",
        "why ",
        "who ",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

const RU_NUMBER_WORDS: &[(&str, u8)] = &[
    ("ноль", 0),
    ("один", 1),
    ("одна", 1),
    ("одно", 1),
    ("два", 2),
    ("две", 2),
    ("три", 3),
    ("четыре", 4),
    ("пять", 5),
    ("шесть", 6),
    ("семь", 7),
    ("восемь", 8),
    ("девять", 9),
    ("десять", 10),
];

const OPTIMIZATION_CLARIFICATION_MARKERS: &[&str] = &[
    "позволь уточнить",
    "позвольте уточнить",
    "имели в виду",
    "имел в виду",
    "или что-то другое",
    "уточните",
    "could you clarify",
    "did you mean",
    "let me clarify",
    "just to clarify",
];

const OPTIMIZATION_ASSISTANT_OPENERS: &[&str] = &[
    "я могу ",
    "я умею ",
    "конечно",
    "разумеется",
    "столица ",
    "i can ",
    "sure,",
    "certainly",
];

fn token_to_number(token: &str) -> Option<u8> {
    if token.chars().all(|ch| ch.is_ascii_digit()) {
        return token.parse().ok();
    }

    RU_NUMBER_WORDS
        .iter()
        .find_map(|(word, value)| (*word == token).then_some(*value))
}

fn extract_number_sequence(text: &str) -> Vec<u8> {
    let lower = text.to_lowercase();
    let mut nums = Vec::new();

    for part in lower.split(|ch: char| ch.is_whitespace() || matches!(ch, ',' | ';' | '/')) {
        let token = part
            .trim()
            .trim_matches(|ch: char| !ch.is_alphanumeric());
        if token.is_empty() {
            continue;
        }
        if let Some(value) = token_to_number(token) {
            nums.push(value);
        }
    }

    nums
}

fn optimization_input_is_number_sequence(text: &str) -> bool {
    let nums = extract_number_sequence(text);
    if nums.len() < 2 {
        return false;
    }

    let token_count = text
        .split(|ch: char| ch.is_whitespace() || matches!(ch, ',' | ';' | '/'))
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .count();

    nums.len() * 2 >= token_count
}

fn output_distorts_input_numbers(input: &str, output: &str) -> bool {
    if !optimization_input_is_number_sequence(input) {
        return false;
    }

    let input_nums = extract_number_sequence(input);
    let output_nums = extract_number_sequence(output);
    if output_nums.is_empty() {
        return false;
    }

    input_nums != output_nums
}

fn optimization_output_is_clarification_response(output: &str) -> bool {
    let lower = output.to_lowercase();
    OPTIMIZATION_CLARIFICATION_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

/// Enforces the Optimization prompt contract — not used for Custom Skill.
fn optimization_answered_instead_of_editing(input: &str, output: &str) -> bool {
    let input_trim = input.trim();
    let output_trim = output.trim();
    if input_trim.is_empty() || output_trim.is_empty() {
        return false;
    }

    if optimization_output_is_clarification_response(output_trim) {
        return true;
    }

    if output_distorts_input_numbers(input_trim, output_trim) {
        return true;
    }

    let input_l = input_trim.to_lowercase();
    let output_l = output_trim.to_lowercase();

    let input_has_equals = input_trim.contains('=');
    let output_has_equals = output_trim.contains('=');
    let input_has_ravno = input_l.contains("равно");
    let output_has_ravno = output_l.contains("равно");

    if (output_has_equals || output_has_ravno) && !input_has_equals && !input_has_ravno {
        return true;
    }

    if optimization_input_looks_like_question(input_trim)
        && (output_has_equals || output_has_ravno)
        && !output_l.contains('?')
    {
        return true;
    }

    let similarity = token_jaccard_similarity(input_trim, output_trim);
    let input_words = input_trim.split_whitespace().count();
    let output_words = output_trim.split_whitespace().count();

    if input_words >= 3 && similarity < 0.45 && output_words >= input_words {
        let lower = output_l;
        if OPTIMIZATION_ASSISTANT_OPENERS
            .iter()
            .any(|marker| lower.starts_with(marker))
        {
            return true;
        }
    }

    if optimization_input_is_number_sequence(input_trim)
        && output_trim.ends_with('?')
        && !input_trim.contains('?')
    {
        return true;
    }

    false
}

fn rewrite_improved_for_mode(
    mode: TextProcessingMode,
    input: &str,
    first: &str,
    retry: &str,
) -> bool {
    if rewrite_improved(input, first, retry) {
        return true;
    }

    if mode == TextProcessingMode::Optimization
        && needs_optimization_retry(input, first)
        && !needs_optimization_retry(input, retry)
    {
        return true;
    }

    if mode == TextProcessingMode::Optimization && needs_optimization_retry(input, first) {
        return token_jaccard_similarity(input, retry) > token_jaccard_similarity(input, first);
    }

    false
}

fn rewrite_improved(input: &str, first: &str, retry: &str) -> bool {
    if retry.trim().is_empty() {
        return false;
    }

    if retry.eq_ignore_ascii_case(first) {
        return false;
    }

    token_jaccard_similarity(input, retry) < token_jaccard_similarity(input, first)
}

fn looks_like_raw_dictation(text: &str) -> bool {
    let lower = text.to_lowercase();
    const FILLERS: &[&str] = &[
        " и так далее",
        " так далее",
        " в общем",
        " всякие",
        " там ",
        " ну ",
        " как бы",
        " типа",
        " короче",
        " значит",
        " в частности",
        " слова паразит",
        " слова-паразит",
        " всякие там",
    ];

    if FILLERS.iter().any(|marker| lower.contains(marker)) {
        return true;
    }

    let words = text.split_whitespace().count();
    let sentence_marks = text
        .chars()
        .filter(|ch| matches!(ch, '.' | '!' | '?'))
        .count();

    words > 12 && sentence_marks <= 1
}

fn token_jaccard_similarity(left: &str, right: &str) -> f64 {
    let left_tokens = normalized_tokens(left);
    let right_tokens = normalized_tokens(right);

    if left_tokens.is_empty() && right_tokens.is_empty() {
        return 1.0;
    }

    let left_set: HashSet<_> = left_tokens.iter().collect();
    let right_set: HashSet<_> = right_tokens.iter().collect();
    let intersection = left_set.intersection(&right_set).count();
    let union = left_set.union(&right_set).count();

    if union == 0 {
        1.0
    } else {
        intersection as f64 / union as f64
    }
}

fn normalized_tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split_whitespace()
        .map(|token| {
            token
                .trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '-')
                .to_string()
        })
        .filter(|token| !token.is_empty())
        .collect()
}

fn rewrite_request_config(
    mode: TextProcessingMode,
    skill_body: Option<&str>,
    local_model: LlmModelKind,
    emulate_enter: bool,
    protected_terms: &[String],
) -> Result<RewriteRequestConfig, RewriteError> {
    match mode {
        TextProcessingMode::Optimization => {
            let use_gec = local_model.is_gec();
            let mut system_prompt = if use_gec {
                gec_system_prompt()
            } else {
                optimization_system_prompt(emulate_enter, protected_terms)
            };
            if use_gec {
                system_prompt.push_str(&format_protected_terms_section(protected_terms));
            }
            Ok(RewriteRequestConfig {
                model: AI_REWRITE_MODEL,
                temperature: if use_gec {
                    0.0
                } else {
                    OPTIMIZATION_TEMPERATURE
                },
                system_prompt,
                use_gec_params: use_gec,
            })
        }
        TextProcessingMode::CustomSkill => {
            let skill_body = skill_body
                .filter(|value| !value.trim().is_empty())
                .ok_or(OpenAiError::custom_skill_missing())?;
            Ok(RewriteRequestConfig {
                model: AI_REWRITE_MODEL,
                temperature: if local_model.is_gec() {
                    0.0
                } else {
                    CUSTOM_SKILL_TEMPERATURE
                },
                system_prompt: skill_body.trim().to_string(),
                use_gec_params: local_model.is_gec(),
            })
        }
        TextProcessingMode::Original | TextProcessingMode::Basic => {
            panic!("rewrite_request_config must not be called for non-AI mode: {mode:?}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optimization_uses_dedicated_prompt_and_model() {
        let config = rewrite_request_config(
            TextProcessingMode::Optimization,
            None,
            LlmModelKind::Qwen3_4B,
            false,
            &[],
        )
        .unwrap();
        assert_eq!(config.model, AI_REWRITE_MODEL);
        assert_eq!(config.temperature, OPTIMIZATION_TEMPERATURE);
        assert!(config.system_prompt.contains("литературный редактор"));
        assert!(config.system_prompt.contains("ИЕРАРХИЯ ПРАВИЛ"));
        assert!(config.system_prompt.contains("Развёртывание без выдумки"));
    }

    #[test]
    fn optimization_prompt_skips_newline_commands_when_emulate_enter() {
        let config = rewrite_request_config(
            TextProcessingMode::Optimization,
            None,
            LlmModelKind::Qwen3_4B,
            true,
            &[],
        )
        .unwrap();
        assert!(config
            .system_prompt
            .contains("Команды перевода строки"));
        assert!(!config
            .system_prompt
            .contains("«новая строка» / «абзац» → перевод строки"));
    }

    #[test]
    fn custom_skill_uses_skill_body_as_full_prompt() {
        let config = rewrite_request_config(
            TextProcessingMode::CustomSkill,
            Some("Custom prompt instructions"),
            LlmModelKind::Qwen3_4B,
            false,
            &[],
        )
        .unwrap();
        assert_eq!(config.model, AI_REWRITE_MODEL);
        assert_eq!(config.temperature, CUSTOM_SKILL_TEMPERATURE);
        assert_eq!(config.system_prompt, "Custom prompt instructions");
    }

    #[test]
    fn custom_skill_requires_body() {
        assert!(rewrite_request_config(
            TextProcessingMode::CustomSkill,
            None,
            LlmModelKind::Qwen3_4B,
            false,
            &[],
        )
        .is_err());
    }

    #[test]
    fn gec_model_uses_gec_prompt_for_optimization() {
        let config = rewrite_request_config(
            TextProcessingMode::Optimization,
            None,
            LlmModelKind::Gec08B,
            false,
            &["ClipBoss".to_string()],
        )
        .unwrap();
        assert!(config.use_gec_params);
        assert_eq!(config.temperature, 0.0);
        assert!(config.system_prompt.contains("корректор"));
        assert!(config.system_prompt.contains("ClipBoss"));
    }

    #[test]
    fn detects_raw_dictation_with_fillers() {
        let raw = "задача моделей всех и режимов чтобы исключить слова паразиты и в общем всякие там дублирование так далее";
        assert!(looks_like_raw_dictation(raw));
    }

    #[test]
    fn requests_stronger_rewrite_when_output_too_close() {
        let raw = "задача моделей всех и режимов чтобы исключить слова паразиты и в общем всякие там дублирование так далее";
        assert!(needs_stronger_rewrite(raw, raw));
    }

    #[test]
    fn requests_stronger_rewrite_for_clean_dictation_left_unchanged() {
        let raw = "в любом случае я использую облачное решение потому что у меня включена не базовая чистка а оптимизация";
        assert!(needs_stronger_rewrite(raw, raw));
    }

    #[test]
    fn sanitize_strips_wrapping_quotes_and_prefix() {
        assert_eq!(
            sanitize_rewrite_output("«Исправленный текст: привет мир»"),
            "привет мир"
        );
    }

    #[test]
    fn sanitize_strips_thinking_blocks() {
        const THINK_OPEN: &str = concat!("<", "think", ">");
        const THINK_CLOSE: &str = concat!("<", "/", "think", ">");
        let raw = format!(
            "{THINK_OPEN}long reasoning{THINK_CLOSE}Раз, два, три, четыре, пять."
        );
        assert_eq!(
            sanitize_rewrite_output(&raw),
            "Раз, два, три, четыре, пять."
        );
    }

    #[test]
    fn sanitize_drops_incomplete_thinking_block() {
        const THINK_OPEN: &str = concat!("<", "think", ">");
        let raw = format!("{THINK_OPEN}still reasoning without closing tag");
        assert_eq!(sanitize_rewrite_output(&raw), "");
    }

    #[test]
    fn rewrite_improved_prefers_less_similar_retry() {
        let raw = "ну в общем задача исключить слова паразиты и так далее";
        let first = "ну в общем задача исключить слова паразиты и так далее";
        let retry = "Задача — исключить слова-паразиты.";
        assert!(rewrite_improved(raw, first, retry));
    }

    #[test]
    fn requests_stronger_rewrite_when_profanity_survives() {
        let raw = "это же пиздец какой-то";
        assert!(needs_stronger_rewrite(raw, raw));
        assert!(needs_stronger_rewrite(
            raw,
            "это же пиздец какой-то, если честно",
        ));
    }

    #[test]
    fn optimization_retry_when_model_answers_math_question() {
        let raw = "сколько будет два плюс два";
        let answer = "Два плюс два равно четыре.";
        assert!(needs_optimization_retry(raw, answer));
        assert!(!needs_optimization_retry(
            raw,
            "Сколько будет два плюс два?"
        ));
    }

    #[test]
    fn optimization_detects_word_math_answer() {
        let raw = "три плюс четыре сколько";
        let answer = "3 + 4 равно 7.";
        assert!(optimization_answered_instead_of_editing(raw, answer));
        assert_eq!(
            finalize_mode_output(TextProcessingMode::Optimization, raw, answer),
            "Три плюс четыре сколько?"
        );
    }

    #[test]
    fn custom_skill_does_not_use_optimization_retry_heuristics() {
        let raw = "сколько будет два плюс два";
        let answer = "Два плюс два равно четыре.";
        assert!(!needs_mode_retry(TextProcessingMode::CustomSkill, raw, answer));
        assert_eq!(
            finalize_mode_output(TextProcessingMode::CustomSkill, raw, answer),
            answer
        );
    }

    #[test]
    fn optimization_detects_clarification_on_number_sequence() {
        let raw = "1,2,3,4,5";
        let answer =
            "Позвольте уточнить: вы имели в виду последовательность чисел от двух до девяти или что-то другое?";
        assert!(optimization_answered_instead_of_editing(raw, answer));
        assert!(needs_optimization_retry(raw, answer));
        assert_eq!(
            finalize_mode_output(TextProcessingMode::Optimization, raw, answer),
            "1, 2, 3, 4, 5"
        );
    }

    #[test]
    fn optimization_accepts_formatted_number_sequence() {
        let raw = "один два три четыре пять";
        let edited = "Один, два, три, четыре, пять.";
        assert!(!optimization_answered_instead_of_editing(raw, edited));
        assert_eq!(
            finalize_mode_output(TextProcessingMode::Optimization, raw, edited),
            edited
        );
    }

    #[test]
    fn optimization_prompt_preserves_number_lists() {
        let prompt = optimization_system_prompt(false, &[]);
        assert!(prompt.contains("Числа, списки, код и команды"));
        assert!(prompt.contains("1,2,3,4,5"));
        assert!(prompt.contains("Позвольте уточнить"));
    }

}
