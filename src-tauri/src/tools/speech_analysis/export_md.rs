use std::path::Path;

use super::types::SpeechAnalysisReport;

pub fn write_markdown_report(path: &Path, report: &SpeechAnalysisReport) -> Result<(), String> {
    let body = render_markdown(report);
    std::fs::write(path, body).map_err(|error| format!("tools.speechAnalysis.exportFailed|{error}"))
}

pub fn render_markdown(report: &SpeechAnalysisReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Speech analysis — {}\n\n", report.file_name));
    out.push_str("## Summary\n\n");
    out.push_str(&format!(
        "- Score mode: {:?}\n",
        report.summary.overall_score_mode
    ));
    out.push_str(&format!("- Label: {}\n", report.summary.overall_label_key));
    if let Some(score) = report.summary.overall_score {
        out.push_str(&format!("- Overall score: {score}/100\n"));
    }
    out.push_str(&format!(
        "- Recording reliability: {} ({}/100)\n",
        report.summary.reliability.label_key, report.summary.reliability.score
    ));
    out.push_str(&format!(
        "- Duration: {:.1}s (speech {:.1}s)\n",
        report.qc.duration_ms as f32 / 1000.0,
        report.qc.speech_duration_ms as f32 / 1000.0
    ));
    if let Some(snr) = report.qc.snr_db_estimate {
        out.push_str(&format!("- SNR estimate: {snr:.1} dB\n"));
    }
    out.push_str(&format!(
        "- Clip ratio: {:.3}%\n",
        report.qc.clip_ratio * 100.0
    ));

    out.push_str("\n## Fluency\n\n");
    if let Some(wpm) = report.fluency.wpm_phonation {
        out.push_str(&format!("- Speech rate: {wpm:.1} wpm (phonation)\n"));
    }
    out.push_str(&format!(
        "- Pauses: {} (long: {})\n",
        report.fluency.pause_count, report.fluency.long_pause_count
    ));
    out.push_str(&format!(
        "- Fillers per 100 words: {:.1}\n",
        report.fluency.fillers_per_100_words
    ));

    out.push_str("\n## Prosody (F0)\n\n");
    if let Some(median) = report.prosody.f0_median_hz {
        out.push_str(&format!("- Median F0: {median:.0} Hz\n"));
    }
    out.push_str(&format!(
        "- F0 variability (semitones): {:.2}\n",
        report.prosody.f0_std_semitones
    ));

    out.push_str("\n## Articulation (CTC self-consistency)\n\n");
    if let Some(per) = report.articulation.per {
        out.push_str(&format!("- PER proxy: {:.1}%\n", per * 100.0));
    }
    if let Some(gop) = report.articulation.mean_gop {
        out.push_str(&format!("- Mean GOP proxy: {gop:.2}\n"));
    }

    out.push_str("\n## Transcript\n\n");
    out.push_str(&report.transcript);
    out.push('\n');

    if let Some(coach) = &report.coach {
        out.push_str("\n## Coach\n\n");
        out.push_str(&format!("- Status: {:?}\n", coach.status));
        out.push_str(&format!("- Provider: {}\n", coach.provider));
        if !coach.summary.is_empty() {
            out.push_str(&format!("\n{}\n", coach.summary));
        }
        if !coach.strengths.is_empty() {
            out.push_str("\n### Strengths\n\n");
            for item in &coach.strengths {
                out.push_str(&format!("- {item}\n"));
            }
        }
        if !coach.improvements.is_empty() {
            out.push_str("\n### Improvements\n\n");
            for item in &coach.improvements {
                out.push_str(&format!("- {item}\n"));
            }
        }
        if !coach.consistency_notes.is_empty() {
            out.push_str("\n### Consistency\n\n");
            for item in &coach.consistency_notes {
                out.push_str(&format!("- {item}\n"));
            }
        }
    }

    if !report.summary.caveats.is_empty() {
        out.push_str("\n## Notes\n\n");
        for caveat in &report.summary.caveats {
            out.push_str(&format!(
                "- tools.speechAnalysis.caveat.{}\n",
                caveat.code
            ));
        }
    } else if !report.limitations.is_empty() {
        out.push_str("\n## Notes\n\n");
        for item in &report.limitations {
            out.push_str(&format!("- {item}\n"));
        }
    }

    out
}
