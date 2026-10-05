use crate::audio::segment::AudioSegment;
use crate::settings::stt_catalog::quants_for_family;
use crate::settings::{AppSettings, LocalSttFamily, LocalSttQuant, LocalSttVariant};
use crate::transcription::local_stt_model_store::bundle_ready_for_settings;
#[cfg(feature = "local-sherpa-stt")]
use crate::transcription::local_stt_model_store::resolve_variant_path;
#[cfg(feature = "local-sherpa-stt")]
use crate::transcription::models::{TranscriptionOptions, TranscriptionResult};
#[cfg(feature = "local-sherpa-stt")]
use crate::transcription::{TranscriptionError, TranscriptionProvider};

#[cfg(feature = "local-sherpa-stt")]
use crate::transcription::local_sherpa::LocalSherpaProvider;
#[cfg(feature = "local-sherpa-stt")]
use tokio_util::sync::CancellationToken;

pub struct CtcDecodeOutcome {
    pub hypothesis_text: String,
    #[allow(dead_code)]
    pub token_chars: Vec<String>,
}

pub fn gigaam_ctc_available(settings: &AppSettings) -> bool {
    gigaam_ctc_variant(settings).is_some()
}

pub fn gigaam_ctc_variant(settings: &AppSettings) -> Option<LocalSttVariant> {
    let active = settings.local_stt_variant();
    if active.family == LocalSttFamily::GigaAmV3E2eCtc
        && bundle_ready_for_settings(settings, active)
    {
        return Some(active);
    }
    for quant in quants_for_family(LocalSttFamily::GigaAmV3E2eCtc) {
        let candidate = LocalSttVariant::new(LocalSttFamily::GigaAmV3E2eCtc, *quant);
        if bundle_ready_for_settings(settings, candidate) {
            return Some(candidate);
        }
    }
    None
}

pub fn preferred_missing_gigaam_ctc_variant() -> LocalSttVariant {
    LocalSttVariant::new(LocalSttFamily::GigaAmV3E2eCtc, LocalSttQuant::Int8)
}

pub fn variant_download_hint(variant: LocalSttVariant) -> super::types::SttVariantDownloadHint {
    let id = variant.as_api_id();
    for quant in [
        "fp32", "fp16", "int8", "q8_0", "q5", "q4_0", "legacy",
    ] {
        let suffix = format!("_{quant}");
        if let Some(family) = id.strip_suffix(&suffix) {
            return super::types::SttVariantDownloadHint {
                family: family.to_string(),
                quant: quant.to_string(),
            };
        }
    }
    super::types::SttVariantDownloadHint {
        family: id,
        quant: "int8".to_string(),
    }
}

pub async fn decode_with_gigaam_ctc(
    settings: &AppSettings,
    audio: &AudioSegment,
    language: Option<String>,
) -> Result<CtcDecodeOutcome, String> {
    #[cfg(not(feature = "local-sherpa-stt"))]
    {
        let _ = (settings, audio, language);
        return Err("tools.speechAnalysis.ctcUnavailable".to_string());
    }

    #[cfg(feature = "local-sherpa-stt")]
    {
        let variant = gigaam_ctc_variant(settings)
            .ok_or_else(|| "tools.speechAnalysis.ctcUnavailable".to_string())?;
        let bundle_dir = resolve_variant_path(settings, variant)
            .map_err(|error| format!("tools.speechAnalysis.ctcUnavailable|{error}"))?;
        let mut align_settings = settings.clone();
        align_settings.set_local_stt_variant(variant);

        let provider = LocalSherpaProvider::new_with_bundle(
            align_settings.clone(),
            bundle_dir,
            CancellationToken::new(),
        );
        let options = TranscriptionOptions {
            language,
            ..TranscriptionOptions {
                model: align_settings.transcription_model.clone(),
                ..Default::default()
            }
        };
        let result: TranscriptionResult = provider
            .transcribe(audio.clone(), options)
            .await
            .map_err(map_ctc_error)?;

        let hypothesis = normalize_hypothesis(&result.text);
        let token_chars = hypothesis
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .map(|ch| ch.to_string())
            .collect();
        Ok(CtcDecodeOutcome {
            hypothesis_text: hypothesis,
            token_chars,
        })
    }
}

#[cfg(feature = "local-sherpa-stt")]
fn map_ctc_error(error: TranscriptionError) -> String {
    format!("tools.speechAnalysis.ctcUnavailable|{error}")
}

#[cfg(feature = "local-sherpa-stt")]
fn normalize_hypothesis(text: &str) -> String {
    text.trim().to_string()
}

pub fn align_char_tokens(
    reference: &[String],
    hypothesis: &[String],
) -> Vec<(String, String, bool)> {
    char_alignment_pairs(reference, hypothesis)
}

fn char_alignment_pairs(
    reference: &[String],
    hypothesis: &[String],
) -> Vec<(String, String, bool)> {
    let n = reference.len();
    let m = hypothesis.len();
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in 0..=n {
        dp[i][0] = i;
    }
    for j in 0..=m {
        dp[0][j] = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = if reference[i - 1] == hypothesis[j - 1] {
                0
            } else {
                1
            };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    let mut i = n;
    let mut j = m;
    let mut out = Vec::new();
    while i > 0 || j > 0 {
        if i > 0 && j > 0 {
            let matched = reference[i - 1] == hypothesis[j - 1];
            if dp[i][j] == dp[i - 1][j - 1] + if matched { 0 } else { 1 } {
                out.push((
                    reference[i - 1].clone(),
                    hypothesis[j - 1].clone(),
                    matched,
                ));
                i -= 1;
                j -= 1;
                continue;
            }
        }
        if i > 0 && dp[i][j] == dp[i - 1][j] + 1 {
            out.push((reference[i - 1].clone(), String::new(), false));
            i -= 1;
        } else if j > 0 {
            out.push((String::new(), hypothesis[j - 1].clone(), false));
            j -= 1;
        } else {
            break;
        }
    }
    out.reverse();
    out
}
