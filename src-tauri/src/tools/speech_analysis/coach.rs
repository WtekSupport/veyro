use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::app::context::AppContext;
use crate::llm::engine::{LlmCompletionParams, LlmEngine};
use crate::network::openai_error::OpenAiError;
use crate::network::retry::RetryConfig;
use crate::settings::secrets;
use crate::settings::{AppSettings, TextRewriteProvider, UiLocale};

use super::coach_facts::{build_coach_facts, template_coach_from_facts};
use super::types::{CoachStatus, SpeechAnalysisCoach, SpeechAnalysisReport};

const OPENAI_CHAT_COMPLETIONS_URL: &str = "https://api.openai.com/v1/chat/completions";
const COACH_OPENAI_MODEL: &str = "gpt-4.1-mini";
const TRANSCRIPT_EXCERPT_MAX: usize = 4000;

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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoachLlmPayload {
    summary: String,
    #[serde(default)]
    strengths: Vec<String>,
    #[serde(default)]
    improvements: Vec<String>,
    #[serde(default, alias = "consistencyNotes")]
    consistency_notes: Vec<String>,
}

pub fn should_run_coach(options: &super::types::SpeechAnalysisOptions, _settings: &AppSettings) -> bool {
    options.enable_llm_coach != Some(false)
}

pub async fn generate_coach(
    ctx: &AppContext,
    settings: &AppSettings,
    options: &super::types::SpeechAnalysisOptions,
    report: &SpeechAnalysisReport,
) -> SpeechAnalysisCoach {
    if !should_run_coach(options, settings) {
        return SpeechAnalysisCoach {
            status: CoachStatus::SkippedUnavailable,
            provider: "template".to_string(),
            summary: String::new(),
            strengths: Vec::new(),
            improvements: Vec::new(),
            consistency_notes: Vec::new(),
            failed_message_key: None,
        };
    }

    let facts_payload = build_coach_facts(report, settings.ui_locale);
    let _ = (ctx, options);
    template_coach_from_facts(&facts_payload, settings.ui_locale)
}

#[allow(dead_code)]
fn provider_label(settings: &AppSettings) -> String {
    match settings.text_rewrite_provider {
        TextRewriteProvider::Openai => "openai".to_string(),
        TextRewriteProvider::Local => "local".to_string(),
    }
}

#[allow(dead_code)]
fn system_prompt(locale: UiLocale) -> String {
    match locale {
        UiLocale::Ru => r#"Ты редактор пояснений. На вход JSON: facts, rules, transcript_excerpt, preliminary (bool), short_clip (bool).
Если short_clip=true — максимум 2 коротких предложения: что учтено, что нет, сколько речи не хватает; не пересказывай все карточки и не повторяй строку итога.
Иначе напиши 2–3 коротких предложения summary по facts. Без списков strengths/improvements — пустые массивы.
Не добавляй числа в текст (ни одной цифры). Не используй camelCase и имена полей.
Не говорите «монотонно», если разброс F0 высокий; voiced — не «гласные».
Если preliminary=true — предварительный балл, не «полная оценка дикции».
Ответ — ТОЛЬКО JSON: {"summary":"...","strengths":[],"improvements":[],"consistencyNotes":[]}"#
            .to_string(),
        UiLocale::En => r#"You rewrite coach notes. Input: facts, rules, transcript_excerpt, preliminary (bool), short_clip (bool).
If short_clip=true, at most 2 short sentences: what is scored, what is not, how much speech is missing; do not recap every card or repeat the overall line.
Otherwise write 2–3 short sentences from facts. Leave strengths/improvements as empty arrays.
Do not include any digits in the text. No camelCase or field names.
Do not say 'monotone' when F0 spread is high; voiced is not 'vowels'.
If preliminary=true, it is a preliminary score, not a full diction grade.
Reply with JSON only: {"summary":"...","strengths":[],"improvements":[],"consistencyNotes":[]}"#
            .to_string(),
    }
}

#[allow(dead_code)]
fn transcript_excerpt(transcript: &str) -> String {
    let trimmed = transcript.trim();
    if trimmed.len() <= TRANSCRIPT_EXCERPT_MAX {
        return trimmed.to_string();
    }
    format!("{}…", &trimmed[..TRANSCRIPT_EXCERPT_MAX])
}

#[allow(dead_code)]
async fn coach_with_local(
    settings: &AppSettings,
    llm_engine: &std::sync::Arc<std::sync::RwLock<LlmEngine>>,
    system: &str,
    brief: &str,
) -> Result<String, String> {
    LlmEngine::ensure_loaded_for_text_rewrite(settings, llm_engine)
        .map_err(|error| format!("tools.speechAnalysis.coach.failed|{error}"))?;
    let engine = llm_engine
        .read()
        .map_err(|_| "llm engine lock poisoned".to_string())?
        .clone();
    let params = LlmCompletionParams {
        temperature: 0.2,
        max_tokens: 900,
        disable_thinking: true,
        use_gec_context: false,
    };
    engine
        .complete(system, brief, params)
        .await
        .map_err(|error| format!("tools.speechAnalysis.coach.failed|{error}"))
}

#[allow(dead_code)]
async fn coach_with_openai(
    settings: &AppSettings,
    http: &Client,
    system: &str,
    brief: &str,
) -> Result<String, String> {
    let api_key = secrets::load_api_key().map_err(|_| "tools.speechAnalysis.coach.noApiKey".to_string())?;
    let retry = RetryConfig::default();
    let mut delay = retry.initial_delay_ms;
    for attempt in 0..retry.max_attempts {
        let request = ChatRequest {
            model: COACH_OPENAI_MODEL,
            temperature: 0.2,
            top_p: 1.0,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: system,
                },
                ChatMessage {
                    role: "user",
                    content: brief,
                },
            ],
        };
        let response = http
            .post(OPENAI_CHAT_COMPLETIONS_URL)
            .bearer_auth(&api_key)
            .json(&request)
            .send()
            .await;
        match response {
            Ok(response) if response.status().is_success() => {
                let body: ChatResponse = response
                    .json()
                    .await
                    .map_err(|error| format!("tools.speechAnalysis.coach.failed|{error}"))?;
                return body
                    .choices
                    .into_iter()
                    .next()
                    .and_then(|choice| choice.message.content)
                    .ok_or_else(|| "tools.speechAnalysis.coach.empty".to_string());
            }
            Ok(response) => {
                let status = response.status().as_u16();
                let body = response.text().await.unwrap_or_default();
                let openai = OpenAiError::from_http(status, &body);
                if !openai.retryable() || attempt + 1 >= retry.max_attempts {
                    return Err(format!(
                        "tools.speechAnalysis.coach.failed|{}",
                        openai.user_message(settings.ui_locale)
                    ));
                }
            }
            Err(error) => {
                if attempt + 1 >= retry.max_attempts {
                    return Err(format!("tools.speechAnalysis.coach.failed|{error}"));
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        delay = (delay * 2).min(retry.max_delay_ms);
    }
    Err("tools.speechAnalysis.coach.failed".to_string())
}

#[allow(dead_code)]
fn parse_coach_response(settings: &AppSettings, raw: &str) -> SpeechAnalysisCoach {
    let trimmed = strip_markdown_fence(raw.trim());
    let json_str = extract_json_object(trimmed);
    match serde_json::from_str::<CoachLlmPayload>(json_str) {
        Ok(parsed) => SpeechAnalysisCoach {
            status: CoachStatus::Available,
            provider: provider_label(settings),
            summary: parsed.summary.trim().to_string(),
            strengths: parsed.strengths,
            improvements: parsed.improvements,
            consistency_notes: parsed.consistency_notes,
            failed_message_key: None,
        },
        Err(_) if !trimmed.is_empty() => SpeechAnalysisCoach {
            status: CoachStatus::Failed,
            provider: provider_label(settings),
            summary: trimmed.chars().take(500).collect(),
            strengths: Vec::new(),
            improvements: Vec::new(),
            consistency_notes: Vec::new(),
            failed_message_key: Some("tools.speechAnalysis.coach.parseFailed".to_string()),
        },
        Err(_) => SpeechAnalysisCoach {
            status: CoachStatus::Failed,
            provider: provider_label(settings),
            summary: String::new(),
            strengths: Vec::new(),
            improvements: Vec::new(),
            consistency_notes: Vec::new(),
            failed_message_key: Some("tools.speechAnalysis.coach.empty".to_string()),
        },
    }
}

fn strip_markdown_fence(raw: &str) -> &str {
    let trimmed = raw.trim();
    let Some(mut body) = trimmed.strip_prefix("```") else {
        return trimmed;
    };
    body = body.trim_start();
    body = body
        .strip_prefix("json")
        .or_else(|| body.strip_prefix("JSON"))
        .unwrap_or(body);
    body = body.trim_start();
    if let Some(end) = body.rfind("```") {
        return body[..end].trim();
    }
    body.trim()
}

fn extract_json_object(raw: &str) -> &str {
    if let Some(start) = raw.find('{') {
        if let Some(end) = raw.rfind('}') {
            if end > start {
                return &raw[start..=end];
            }
        }
    }
    raw
}
