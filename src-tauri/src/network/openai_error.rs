use crate::settings::UiLocale;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAiErrorKind {
    ApiKeyMissing,
    Authentication,
    InsufficientQuota,
    RateLimit,
    Network,
    Server,
    BadRequest,
    ModelNotFound,
    EmptyResponse,
    CustomSkillMissing,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct OpenAiError {
    pub kind: OpenAiErrorKind,
    pub status: Option<u16>,
    pub api_message: Option<String>,
}

impl OpenAiError {
    pub fn api_key_missing() -> Self {
        Self {
            kind: OpenAiErrorKind::ApiKeyMissing,
            status: None,
            api_message: None,
        }
    }

    pub fn custom_skill_missing() -> Self {
        Self {
            kind: OpenAiErrorKind::CustomSkillMissing,
            status: None,
            api_message: None,
        }
    }

    pub fn empty_response() -> Self {
        Self {
            kind: OpenAiErrorKind::EmptyResponse,
            status: None,
            api_message: None,
        }
    }

    pub fn from_http(status: u16, body: &str) -> Self {
        let (api_code, api_message) = parse_error_body(body);
        let kind = classify_http_status(status, &api_code, &api_message);
        Self {
            kind,
            status: Some(status),
            api_message: non_empty(api_message),
        }
    }

    pub fn from_reqwest(error: &reqwest::Error) -> Self {
        let api_message = if error.is_timeout() {
            Some("timeout".to_string())
        } else if error.is_connect() {
            Some("connect".to_string())
        } else {
            Some(error.to_string())
        };

        Self {
            kind: OpenAiErrorKind::Network,
            status: error.status().map(|status| status.as_u16()),
            api_message,
        }
    }

    fn network_i18n_key(&self) -> &'static str {
        match self.api_message.as_deref() {
            Some(message) if message.contains("timeout") => "openai.network_timeout",
            Some(message) if message.contains("connect") => "openai.network_connect",
            _ => "openai.network",
        }
    }

    pub fn from_network_exhausted(error: &reqwest::Error, attempts: u32) -> Self {
        let mut parsed = Self::from_reqwest(error);
        parsed.api_message = Some(format!(
            "attempts={attempts}; detail={}",
            parsed.api_message.as_deref().unwrap_or("")
        ));
        parsed
    }

    pub fn retryable(&self) -> bool {
        matches!(
            self.kind,
            OpenAiErrorKind::Network | OpenAiErrorKind::Server | OpenAiErrorKind::RateLimit
        )
    }

    pub fn i18n_key(&self) -> &'static str {
        match self.kind {
            OpenAiErrorKind::ApiKeyMissing => "openai.api_key_missing",
            OpenAiErrorKind::Authentication => "openai.auth_failed",
            OpenAiErrorKind::InsufficientQuota => "openai.insufficient_quota",
            OpenAiErrorKind::RateLimit => "openai.rate_limit",
            OpenAiErrorKind::Network => "openai.network",
            OpenAiErrorKind::Server => "openai.server",
            OpenAiErrorKind::BadRequest => "openai.bad_request",
            OpenAiErrorKind::ModelNotFound => "openai.model_not_found",
            OpenAiErrorKind::EmptyResponse => "openai.empty_response",
            OpenAiErrorKind::CustomSkillMissing => "openai.custom_skill_missing",
            OpenAiErrorKind::Unknown => "openai.unknown",
        }
    }

    pub fn user_message(&self, locale: UiLocale) -> String {
        let status = self
            .status
            .map(|value| value.to_string())
            .unwrap_or_default();
        let detail = self.api_message.as_deref().unwrap_or("");
        let detail_suffix = if detail.is_empty() {
            String::new()
        } else {
            format!(" {detail}")
        };
        let key = if self.kind == OpenAiErrorKind::Network {
            self.network_i18n_key()
        } else {
            self.i18n_key()
        };

        crate::i18n::translate(
            locale,
            key,
            &[("status", &status), ("detail", &detail_suffix)],
        )
    }
}

impl std::fmt::Display for OpenAiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.user_message(UiLocale::En))
    }
}

impl std::error::Error for OpenAiError {}

fn parse_error_body(body: &str) -> (String, String) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return (String::new(), trim_body(body));
    };

    let Some(error) = value.get("error") else {
        return (String::new(), trim_body(body));
    };

    let message = error
        .get("message")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    let kind = error
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let code = error
        .get("code")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let api_code = if !code.is_empty() { code } else { kind };

    (api_code, message)
}

fn classify_http_status(status: u16, api_code: &str, api_message: &str) -> OpenAiErrorKind {
    let code = api_code.to_ascii_lowercase();
    let message = api_message.to_ascii_lowercase();

    if is_insufficient_quota(&code, &message) {
        return OpenAiErrorKind::InsufficientQuota;
    }

    match status {
        401 => OpenAiErrorKind::Authentication,
        403 if message.contains("country") || message.contains("region") => {
            OpenAiErrorKind::BadRequest
        }
        403 => OpenAiErrorKind::Authentication,
        404 if code.contains("model") || message.contains("model") => OpenAiErrorKind::ModelNotFound,
        404 => OpenAiErrorKind::BadRequest,
        408 | 500..=599 => OpenAiErrorKind::Server,
        429 if is_rate_limit(&code, &message) => OpenAiErrorKind::RateLimit,
        429 => OpenAiErrorKind::InsufficientQuota,
        400..=499 => OpenAiErrorKind::BadRequest,
        _ => OpenAiErrorKind::Unknown,
    }
}

fn is_insufficient_quota(code: &str, message: &str) -> bool {
    code.contains("insufficient_quota")
        || code.contains("billing")
        || message.contains("insufficient quota")
        || message.contains("exceeded your current quota")
        || message.contains("check your plan and billing")
        || message.contains("billing details")
}

fn is_rate_limit(code: &str, message: &str) -> bool {
    code.contains("rate_limit")
        || message.contains("rate limit")
        || message.contains("requests per minute")
        || message.contains("tokens per min")
}

fn trim_body(body: &str) -> String {
    body.trim().chars().take(240).collect()
}

fn non_empty(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_insufficient_quota_from_429_body() {
        let body = r#"{"error":{"message":"You exceeded your current quota, please check your plan and billing details.","type":"insufficient_quota","code":"insufficient_quota"}}"#;
        let error = OpenAiError::from_http(429, body);
        assert_eq!(error.kind, OpenAiErrorKind::InsufficientQuota);
        assert!(!error.retryable());
    }

    #[test]
    fn detects_rate_limit_from_429_body() {
        let body = r#"{"error":{"message":"Rate limit reached for requests","type":"rate_limit_exceeded","code":"rate_limit_exceeded"}}"#;
        let error = OpenAiError::from_http(429, body);
        assert_eq!(error.kind, OpenAiErrorKind::RateLimit);
        assert!(error.retryable());
    }

    #[test]
    fn classifies_invalid_api_key() {
        let body = r#"{"error":{"message":"Incorrect API key provided","type":"invalid_request_error","code":"invalid_api_key"}}"#;
        let error = OpenAiError::from_http(401, body);
        assert_eq!(error.kind, OpenAiErrorKind::Authentication);
    }

    #[test]
    fn classifies_model_not_found() {
        let body = r#"{"error":{"message":"The model `gpt-unknown` does not exist","type":"invalid_request_error","code":"model_not_found"}}"#;
        let error = OpenAiError::from_http(404, body);
        assert_eq!(error.kind, OpenAiErrorKind::ModelNotFound);
    }

    #[test]
    fn user_message_is_localized() {
        let error = OpenAiError {
            kind: OpenAiErrorKind::InsufficientQuota,
            status: Some(429),
            api_message: Some("quota exceeded".to_string()),
        };
        assert!(error.user_message(UiLocale::Ru).contains("баланс"));
        assert!(error.user_message(UiLocale::En).contains("balance"));
    }
}
