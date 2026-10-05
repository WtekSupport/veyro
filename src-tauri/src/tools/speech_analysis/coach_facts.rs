use super::config::SpeechAnalysisConfig;

use super::types::{

    DimensionStatus, OverallScoreMode, PitchExpressiveness, SpeechAnalysisReport, SummaryDimension,

};

use crate::settings::UiLocale;



#[derive(Debug, Clone)]

pub struct CoachFactsPayload {
    pub facts: Vec<String>,
}



pub fn build_coach_facts(report: &SpeechAnalysisReport, locale: UiLocale) -> CoachFactsPayload {

    let config = SpeechAnalysisConfig::default();

    let mut facts = Vec::new();

    for dim in &report.summary.dimensions {

        if let Some(line) = fact_for_dimension(dim, report, locale, &config) {

            facts.push(line);

        }

    }



    let axes_in_overall = report
        .summary
        .overall_coverage
        .as_ref()
        .map(|c| c.included)
        .unwrap_or(0);

    if let Some(coverage) = &report.summary.overall_coverage {
        if axes_in_overall >= 2 {
            facts.push(coverage_fact(coverage, report, locale));
        } else if axes_in_overall > 0 {
            facts.push(insufficient_overall_fact(report, locale));
        }
    }

    if axes_in_overall >= 2 {
        if let Some(score) = report.summary.overall_score {
            facts.push(overall_score_fact(
                score,
                report.summary.overall_score_mode,
                &report.summary.overall_coverage,
                locale,
            ));
        }

        if let Some(weak) = &report.summary.overall_weak_spot_id {
            facts.push(weak_spot_fact(weak, report, locale));
        }
    }



    facts.push(match locale {

        UiLocale::Ru => "Качество сигнала — отдельная карточка, не одна из пяти осей общего балла."

            .to_string(),

        UiLocale::En => "Signal quality is a separate card, not one of the five diction axes.".to_string(),

    });

    facts.push(match locale {

        UiLocale::Ru => {

            "«Voiced» — доля кадров с детектированным F0 (звонкие участки), это не «доля гласных»."

                .to_string()

        }

        UiLocale::En => {

            "'Voiced' is the share of frames with detected F0 (voiced regions), not 'vowel count'."

                .to_string()

        }

    });



    CoachFactsPayload { facts }
}

pub fn template_coach_from_facts(

    payload: &CoachFactsPayload,

    locale: UiLocale,

) -> super::types::SpeechAnalysisCoach {

    let summary = template_summary(&payload.facts, locale);

    super::types::SpeechAnalysisCoach {

        status: super::types::CoachStatus::Available,

        provider: "template".to_string(),

        summary,

        strengths: Vec::new(),

        improvements: Vec::new(),

        consistency_notes: Vec::new(),

        failed_message_key: None,

    }

}



fn template_summary(facts: &[String], locale: UiLocale) -> String {

    let take = facts.iter().take(4).cloned().collect::<Vec<_>>();

    if take.is_empty() {

        return match locale {

            UiLocale::Ru => "Недостаточно данных для пояснения.".to_string(),

            UiLocale::En => "Not enough data for coach notes.".to_string(),

        };

    }

    take.join(" ")

}

fn fact_for_dimension(

    dim: &SummaryDimension,

    report: &SpeechAnalysisReport,

    locale: UiLocale,

    config: &SpeechAnalysisConfig,

) -> Option<String> {

    let title = dimension_title(&dim.id, locale);

    match dim.id.as_str() {

        "signalQuality" => {

            let score = dim.score?;

            Some(match locale {

                UiLocale::Ru => format!(

                    "{title}: {score}/100 — вердикт «{}».",

                    grade_word(dim.grade_key.as_str(), locale)

                ),

                UiLocale::En => format!(

                    "{title}: {score}/100 — verdict «{}».",

                    grade_word(dim.grade_key.as_str(), locale)

                ),

            })

        }

        "fluency" => {
            let f = &report.fluency;
            if !f.pause_measurement_reliable {
                return Some(match locale {
                    UiLocale::Ru => format!(
                        "{title}: замер пауз по энергии ненадёжен — не упоминайте «минимальные паузы» и не хвалите беглость баллом."
                    ),
                    UiLocale::En => format!(
                        "{title}: pause measurement from energy is unreliable — do not praise fluency score or minimal pauses."
                    ),
                });
            }

            let score = dim.score?;

            let speech_sec = (report.qc.speech_duration_ms as f32 / 1000.0).max(0.1);

            let syl_per_sec = f.syllable_count as f32 / speech_sec;

            let wpm = f

                .wpm_phonation

                .map(|v| format!("{v:.0}"))

                .unwrap_or_else(|| "—".to_string());

            Some(match locale {

                UiLocale::Ru => format!(

                    "{title}: {score}/100 — {}. Темп ~{syl_per_sec:.1} сл/с, ~{wpm} сл/мин (phonation). Паузы от {min_pause} мс: {pause_count} шт., средняя {mean_pause:.0} мс; после знаков {punct_pauses}, внутри фразы {mid_pauses}; длинных ≥{long_pause} мс: {long_count}, очень длинных ≥{very_long} мс: {very_long_count}. Паразиты: {fillers:.1} на 100 слов. Запинки: {reps}.",

                    grade_word(dim.grade_key.as_str(), locale),

                    min_pause = config.min_pause_ms,

                    pause_count = f.pause_count,

                    mean_pause = f.mean_pause_ms,

                    long_pause = config.long_pause_ms,

                    long_count = f.long_pause_count,
                    punct_pauses = f.pause_count_punctuation,
                    mid_pauses = f.pause_count_mid_phrase,
                    very_long = config.very_long_pause_ms,
                    very_long_count = f.very_long_pause_count,

                    fillers = f.fillers_per_100_words,

                    reps = f.repetition_count,

                ),

                UiLocale::En => format!(

                    "{title}: {score}/100 — {}. Pace ~{syl_per_sec:.1} syl/s, ~{wpm} wpm (phonation). Pauses from {min_pause} ms: {pause_count}, mean {mean_pause:.0} ms; after punctuation {punct_pauses}, mid-phrase {mid_pauses}; long ≥{long_pause} ms: {long_count}, very long ≥{very_long} ms: {very_long_count}. Fillers: {fillers:.1} per 100 words. Hesitation repeats: {reps}.",

                    grade_word(dim.grade_key.as_str(), locale),

                    min_pause = config.min_pause_ms,

                    pause_count = f.pause_count,

                    mean_pause = f.mean_pause_ms,

                    long_pause = config.long_pause_ms,

                    long_count = f.long_pause_count,
                    punct_pauses = f.pause_count_punctuation,
                    mid_pauses = f.pause_count_mid_phrase,
                    very_long = config.very_long_pause_ms,
                    very_long_count = f.very_long_pause_count,

                    fillers = f.fillers_per_100_words,

                    reps = f.repetition_count,

                ),

            })

        }

        "prosody" => {

            let score = dim.score?;

            let p = &report.prosody;

            let express = expressiveness_word(p.expressiveness, locale);

            Some(match locale {

                UiLocale::Ru => format!(

                    "{title}: {score}/100 — {} ({express}). Разброс F0 p10–p90: {spread:.2} st (ориентир ~2–5). Диапазон p5–p95: {range}. Voiced среди речевых кадров: {voiced:.0}%.",

                    grade_word(dim.grade_key.as_str(), locale),

                    express = express,

                    spread = p.f0_std_semitones,

                    range = p

                        .f0_range_semitones

                        .map(|r| format!("{r:.1} st"))

                        .unwrap_or_else(|| "—".to_string()),

                    voiced = p.voiced_fraction * 100.0,

                ),

                UiLocale::En => format!(

                    "{title}: {score}/100 — {} ({express}). F0 spread p10–p90: {spread:.2} st (target ~2–5). p5–p95 range: {range}. Voiced among speech frames: {voiced:.0}%.",

                    grade_word(dim.grade_key.as_str(), locale),

                    express = express,

                    spread = p.f0_std_semitones,

                    range = p

                        .f0_range_semitones

                        .map(|r| format!("{r:.1} st"))

                        .unwrap_or_else(|| "—".to_string()),

                    voiced = p.voiced_fraction * 100.0,

                ),

            })

        }

        "articulation" => {

            if dim.status == DimensionStatus::InsufficientData {

                let speech_sec = report.qc.speech_duration_ms / 1000;

                let need_sec = config.min_speech_ms_articulation / 1000;

                let remain = need_sec.saturating_sub(speech_sec);

                return Some(match locale {

                    UiLocale::Ru => format!(

                        "{title}: для полной оценки нужно ещё {remain} с речи (сейчас {speech_sec} из {need_sec} с). Это черновая согласованность CTC, не клиническая артикуляция.",

                    ),

                    UiLocale::En => format!(

                        "{title}: need {remain}s more speech for full score ({speech_sec}s of {need_sec}s). Draft CTC consistency, not clinical articulation.",

                    ),

                });

            }

            let score = dim.score?;

            Some(match locale {

                UiLocale::Ru => format!(

                    "{title}: {score}/100 — {} (proxy CTC/PER).",

                    grade_word(dim.grade_key.as_str(), locale)

                ),

                UiLocale::En => format!(

                    "{title}: {score}/100 — {} (CTC/PER proxy).",

                    grade_word(dim.grade_key.as_str(), locale)

                ),

            })

        }

        "confidence" => {

            let score = dim.score?;

            Some(match locale {

                UiLocale::Ru => format!(

                    "{title}: {score}/100 — {}.",

                    grade_word(dim.grade_key.as_str(), locale)

                ),

                UiLocale::En => format!(

                    "{title}: {score}/100 — {}.",

                    grade_word(dim.grade_key.as_str(), locale)

                ),

            })

        }

        "intelligibility" => {

            let score = dim.score?;

            let mean = report

                .intelligibility

                .mean_word_confidence

                .map(|c| (c * 100.0).round() as u32);

            Some(match locale {

                UiLocale::Ru => format!(

                    "{title}: {score}/100 — {}.{}",

                    grade_word(dim.grade_key.as_str(), locale),

                    mean.map(|m| format!(" Средняя уверенность слов: {m}%."))

                        .unwrap_or_default()

                ),

                UiLocale::En => format!(

                    "{title}: {score}/100 — {}.{}",

                    grade_word(dim.grade_key.as_str(), locale),

                    mean.map(|m| format!(" Mean word confidence: {m}%."))

                        .unwrap_or_default()

                ),

            })

        }

        _ => dim.score.map(|score| {

            format!(

                "{}: {}/100 — {}",

                title,

                score,

                grade_word(dim.grade_key.as_str(), locale)

            )

        }),

    }

}



fn insufficient_overall_fact(report: &SpeechAnalysisReport, locale: UiLocale) -> String {
    let speech_sec = (report.fluency.net_speech_duration_ms / 1000).max(1);
    match locale {
        UiLocale::Ru => format!(
            "Недостаточно данных для общего балла (речь ~{speech_sec} с): учтена меньше двух осей дикции."
        ),
        UiLocale::En => format!(
            "Not enough data for an overall score (~{speech_sec}s speech): fewer than two diction axes included."
        ),
    }
}

fn overall_score_fact(
    score: u8,
    mode: OverallScoreMode,
    coverage: &Option<super::types::SpeechAnalysisCoverage>,
    locale: UiLocale,
) -> String {
    let axes = coverage
        .as_ref()
        .map(|c| {
            c.included_ids
                .iter()
                .map(|id| dimension_title(id, locale))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();

    match (mode, locale) {
        (OverallScoreMode::Full, UiLocale::Ru) => {
            format!("Общий балл: {score}/100 (оси: {axes}).")
        }
        (OverallScoreMode::Full, UiLocale::En) => {
            format!("Overall diction score: {score}/100 (axes: {axes}).")
        }
        (OverallScoreMode::Preliminary, UiLocale::Ru) => format!(
            "Предварительный балл по осям ({axes}): {score}/100 — полная оценка дикции ещё недоступна."
        ),
        (OverallScoreMode::Preliminary, UiLocale::En) => format!(
            "Preliminary score on axes ({axes}): {score}/100 — full diction grade not available yet."
        ),
        (_, UiLocale::Ru) => "Балл дикции недоступен.".to_string(),
        (_, UiLocale::En) => "Diction score unavailable.".to_string(),
    }
}



fn coverage_fact(

    coverage: &super::types::SpeechAnalysisCoverage,

    report: &SpeechAnalysisReport,

    locale: UiLocale,

) -> String {

    let included: Vec<String> = coverage

        .included_ids

        .iter()

        .map(|id| dimension_title(id, locale))

        .collect();

    let missing: Vec<String> = coverage

        .missing_ids

        .iter()

        .map(|id| missing_axis_label(id, report, locale))

        .collect();

    match locale {

        UiLocale::Ru => format!(

            "В общем балле учтено {} из {} осей дикции. Учтено: {}. Не учтено: {}.",

            coverage.included,

            coverage.total,

            if included.is_empty() {

                "—".to_string()

            } else {

                included.join(", ")

            },

            if missing.is_empty() {

                "—".to_string()

            } else {

                missing.join(", ")

            }

        ),

        UiLocale::En => format!(

            "Overall score uses {} of {} diction axes. Included: {}. Excluded: {}.",

            coverage.included,

            coverage.total,

            if included.is_empty() {

                "—".to_string()

            } else {

                included.join(", ")

            },

            if missing.is_empty() {

                "—".to_string()

            } else {

                missing.join(", ")

            }

        ),

    }

}



fn missing_axis_label(id: &str, report: &SpeechAnalysisReport, locale: UiLocale) -> String {

    let title = dimension_title(id, locale);

    match id {

        "confidence" if report

            .summary

            .overall_coverage

            .as_ref()

            .is_some_and(|c| c.included_ids.iter().any(|x| x == "intelligibility")) =>

        {

            match locale {

                UiLocale::Ru => format!("{title} (объединена с разборчивостью)"),

                UiLocale::En => format!("{title} (merged with intelligibility)"),

            }

        }

        "articulation" => {

            let short = report.summary.dimensions.iter().find(|d| d.id == "articulation");

            if short.is_some_and(|d| d.status == DimensionStatus::InsufficientData) {

                match locale {

                    UiLocale::Ru => format!("{title} (мало речи)"),

                    UiLocale::En => format!("{title} (not enough speech)"),

                }

            } else {

                title

            }

        }

        _ => title,

    }

}



fn weak_spot_fact(weak_id: &str, report: &SpeechAnalysisReport, locale: UiLocale) -> String {

    let title = dimension_title(weak_id, locale);

    let score = report

        .summary

        .dimensions

        .iter()

        .find(|d| d.id == weak_id)

        .and_then(|d| d.score)

        .unwrap_or(0);

    match locale {

        UiLocale::Ru => format!("Слабое место среди учтённых осей: {title} ({score}/100)."),

        UiLocale::En => format!("Weakest included axis: {title} ({score}/100)."),

    }

}



fn expressiveness_word(level: PitchExpressiveness, locale: UiLocale) -> &'static str {

    match (level, locale) {

        (PitchExpressiveness::Monotone, UiLocale::Ru) => "по F0 — монотонно",

        (PitchExpressiveness::Moderate, UiLocale::Ru) => "по F0 — умеренно",

        (PitchExpressiveness::Expressive, UiLocale::Ru) => "по F0 — выразительно",

        (PitchExpressiveness::Monotone, UiLocale::En) => "F0: monotone",

        (PitchExpressiveness::Moderate, UiLocale::En) => "F0: moderate",

        (PitchExpressiveness::Expressive, UiLocale::En) => "F0: expressive",

    }

}



fn dimension_title(id: &str, locale: UiLocale) -> String {

    match (id, locale) {

        ("signalQuality", UiLocale::Ru) => "Качество сигнала".to_string(),

        ("confidence", UiLocale::Ru) => "Уверенность STT".to_string(),

        ("intelligibility", UiLocale::Ru) => "Разборчивость".to_string(),

        ("articulation", UiLocale::Ru) => "Согласованность CTC (черновая)".to_string(),

        ("fluency", UiLocale::Ru) => "Беглость".to_string(),

        ("prosody", UiLocale::Ru) => "Просодия".to_string(),

        ("signalQuality", UiLocale::En) => "Signal quality".to_string(),

        ("confidence", UiLocale::En) => "STT confidence".to_string(),

        ("intelligibility", UiLocale::En) => "Intelligibility".to_string(),

        ("articulation", UiLocale::En) => "CTC consistency (draft)".to_string(),

        ("fluency", UiLocale::En) => "Fluency".to_string(),

        ("prosody", UiLocale::En) => "Prosody".to_string(),

        _ => id.to_string(),

    }

}



fn grade_word(grade_key: &str, locale: UiLocale) -> &'static str {

    let key = grade_key.rsplit('.').next().unwrap_or("");

    match (key, locale) {

        ("excellent", UiLocale::Ru) => "отлично",

        ("good", UiLocale::Ru) => "хорошо",

        ("fair", UiLocale::Ru) => "средне",

        ("weak", UiLocale::Ru) => "слабо",

        ("unavailable", UiLocale::Ru) => "недостаточно данных",

        ("excellent", UiLocale::En) => "excellent",

        ("good", UiLocale::En) => "good",

        ("fair", UiLocale::En) => "fair",

        ("weak", UiLocale::En) => "weak",

        ("unavailable", UiLocale::En) => "insufficient data",

        _ => "—",

    }

}

