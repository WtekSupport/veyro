use serde::{Deserialize, Serialize};

use crate::timed_text::TimedTextSegment;

use super::model_plan::{LocalSttVariantDto, SpeechAnalysisModelPolicy};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechAnalysisOptions {
    #[serde(default)]
    pub stt_language_override: Option<String>,
    #[serde(default)]
    pub model_policy: SpeechAnalysisModelPolicy,
    #[serde(default)]
    pub manual_variant: Option<LocalSttVariantDto>,
    #[serde(default)]
    pub fill_gaps_only: bool,
    #[serde(default)]
    pub auto_download_models: bool,
    #[serde(default)]
    pub cache: Option<SpeechAnalysisCache>,
    /// `None` = run coach when LLM is available (settings text rewrite provider).
    #[serde(default)]
    pub enable_llm_coach: Option<bool>,
    #[serde(default)]
    pub regenerate_coach: bool,
    #[serde(default)]
    pub speech_register_hint: Option<SpeechRegisterHint>,
    #[serde(default)]
    pub speaker_profile_id: Option<String>,
    #[serde(default)]
    pub accumulate_into_profile: bool,
    #[serde(default)]
    pub new_speaker_profile_label: Option<String>,
    #[serde(default)]
    pub analysis_mode: SpeechAnalysisMode,
    #[serde(default)]
    pub reference_text: Option<String>,
    #[serde(default)]
    pub reference_preset_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SpeechAnalysisMode {
    #[default]
    Free,
    ReadAloud,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechRegister {
    Reading,
    Spontaneous,
}

impl Default for SpeechRegister {
    fn default() -> Self {
        Self::Spontaneous
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SpeechRegisterHint {
    #[default]
    Auto,
    Reading,
    Spontaneous,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechAnalysisAccumulation {
    pub profile_id: String,
    pub profile_label: String,
    pub prior_net_speech_duration_ms: u64,
    pub prior_word_count: u64,
    pub combined_net_speech_duration_ms: u64,
    pub combined_word_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechAnalysisMeta {
    pub speech_register: SpeechRegister,
    pub speech_register_auto: bool,
    #[serde(default)]
    pub analysis_mode: SpeechAnalysisMode,
    #[serde(default)]
    pub read_aloud: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accumulation: Option<SpeechAnalysisAccumulation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceWordAlignment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hypothesis: Option<String>,
    pub matched: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceEvalReport {
    pub reference_word_count: usize,
    pub hypothesis_word_count: usize,
    pub wer_percent: f32,
    pub cer_percent: f32,
    pub substitutions: Vec<SubstitutionPair>,
    pub alignment: Vec<ReferenceWordAlignment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechAnalysisCache {
    pub transcript: String,
    pub timed_segments: Vec<TimedTextSegment>,
    pub detected_language: Option<String>,
    pub qc: QcReport,
    pub fluency: FluencyReport,
    pub prosody: ProsodyReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coach: Option<SpeechAnalysisCoach>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoachStatus {
    Available,
    SkippedUnavailable,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechAnalysisCoach {
    pub status: CoachStatus,
    pub provider: String,
    pub summary: String,
    #[serde(default)]
    pub strengths: Vec<String>,
    #[serde(default)]
    pub improvements: Vec<String>,
    #[serde(default)]
    pub consistency_notes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_message_key: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReliabilityLevel {
    High,
    Medium,
    Low,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QcReport {
    pub duration_ms: u64,
    pub speech_duration_ms: u64,
    pub sample_rate_hz: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_file_sample_rate_hz: Option<u32>,
    pub snr_db_estimate: Option<f32>,
    pub clip_ratio: f32,
    pub narrowband: bool,
    pub reliability: ReliabilityLevel,
    pub flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FluencyReport {
    pub word_count: usize,
    pub syllable_count: usize,
    /// VAD speech minus internal pauses (used for tempo and duration gates).
    #[serde(default)]
    pub net_speech_duration_ms: u64,
    #[serde(default)]
    pub speech_register: SpeechRegister,
    #[serde(default)]
    pub speech_register_auto: bool,
    pub wpm_overall: Option<f32>,
    pub wpm_phonation: Option<f32>,
    pub phonation_ratio: f32,
    pub pause_count: usize,
    pub mean_pause_ms: f32,
    #[serde(default)]
    pub median_pause_ms: f32,
    pub long_pause_count: usize,
    pub pause_total_ms: u64,
    pub min_pause_ms: u64,
    pub long_pause_ms: u64,
    pub very_long_pause_ms: u64,
    pub pause_count_punctuation: usize,
    pub pause_count_mid_phrase: usize,
    pub very_long_pause_count: usize,
    pub pause_measurement_reliable: bool,
    pub fillers_per_100_words: f32,
    pub filler_hits: Vec<FillerHit>,
    pub repetition_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct F0ContourPoint {
    pub time_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hz: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillerHit {
    pub phrase: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProsodyReport {
    pub f0_median_hz: Option<f32>,
    pub f0_range_semitones: Option<f32>,
    pub f0_std_semitones: f32,
    pub voiced_fraction: f32,
    pub expressiveness: PitchExpressiveness,
    pub low_confidence: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub f0_contour: Vec<F0ContourPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_quality: Option<VoiceQualityReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceQualityReport {
    pub jitter_local_percent: Option<f32>,
    pub shimmer_local_percent: Option<f32>,
    pub cpps_db: Option<f32>,
    pub low_confidence: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordGopHit {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub gop: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PitchExpressiveness {
    Monotone,
    Moderate,
    Expressive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntelligibilityReport {
    pub mean_word_confidence: Option<f32>,
    pub low_confidence_word_count: usize,
    pub low_confidence_words: Vec<LowConfidenceWord>,
    pub reliability: ReliabilityLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LowConfidenceWord {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SttVariantDownloadHint {
    pub family: String,
    pub quant: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeakSymbolSummary {
    pub symbol: String,
    pub token_count: usize,
    pub error_count: usize,
    pub error_rate_percent: f32,
    pub excess_vs_baseline_percent: f32,
    pub low_sample: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticulationReport {
    pub reliability: ReliabilityLevel,
    pub unavailable_reason_key: Option<String>,
    pub ctc_variant_used: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing_ctc_download: Option<SttVariantDownloadHint>,
    pub mean_gop: Option<f32>,
    pub per: Option<f32>,
    pub low_gop_token_count: usize,
    pub alignment_token_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub letter_baseline_error_rate_percent: Option<f32>,
    pub weak_symbols: Vec<WeakSymbolSummary>,
    pub top_substitutions: Vec<SubstitutionPair>,
    pub phoneme_segments: Vec<PhonemeSegment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub word_gop_hits: Vec<WordGopHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechAnalysisCaveat {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionStatus {
    Available,
    InsufficientData,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverallScoreMode {
    Hidden,
    Preliminary,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryDimension {
    pub id: String,
    pub status: DimensionStatus,
    pub score: Option<u8>,
    pub grade_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_required_sec: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_required_words: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_required_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechAnalysisReliability {
    pub level: ReliabilityLevel,
    pub score: u8,
    pub label_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechAnalysisCoverage {
    pub included: u8,
    pub total: u8,
    pub included_ids: Vec<String>,
    pub missing_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemWord {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub reason_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_hint: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepetitionExample {
    pub token: String,
    pub context: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub kind: RepetitionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepetitionKind {
    Stutter,
    Emphasis,
    PossibleDeliberate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechAnalysisProblems {
    pub source_key: String,
    pub problem_words: Vec<ProblemWord>,
    pub weak_symbols: Vec<WeakSymbolSummary>,
    pub substitutions: Vec<SubstitutionPair>,
    pub repetition_count: usize,
    pub repetition_examples: Vec<RepetitionExample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechAnalysisSummary {
    pub overall_score_mode: OverallScoreMode,
    pub overall_score: Option<u8>,
    pub overall_grade_key: String,
    pub overall_label_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overall_label_param: Option<String>,
    pub overall_show_grade: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overall_coverage: Option<SpeechAnalysisCoverage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overall_weak_spot_id: Option<String>,
    pub reliability: SpeechAnalysisReliability,
    pub dimensions: Vec<SummaryDimension>,
    pub caveats: Vec<SpeechAnalysisCaveat>,
    pub problems: SpeechAnalysisProblems,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubstitutionPair {
    pub expected: String,
    pub observed: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhonemeSegment {
    pub symbol: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub gop: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechAnalysisReport {
    pub path_key: String,
    pub file_name: String,
    pub transcript: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub timed_segments: Vec<TimedTextSegment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_language: Option<String>,
    pub qc: QcReport,
    pub fluency: FluencyReport,
    pub prosody: ProsodyReport,
    pub intelligibility: IntelligibilityReport,
    pub articulation: ArticulationReport,
    pub summary: SpeechAnalysisSummary,
    #[serde(default)]
    pub limitations: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coach: Option<SpeechAnalysisCoach>,
    #[serde(default)]
    pub meta: SpeechAnalysisMeta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_eval: Option<ReferenceEvalReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_text: Option<String>,
}

impl Default for SpeechAnalysisMeta {
    fn default() -> Self {
        Self {
            speech_register: SpeechRegister::Spontaneous,
            speech_register_auto: true,
            analysis_mode: SpeechAnalysisMode::Free,
            read_aloud: false,
            accumulation: None,
        }
    }
}
