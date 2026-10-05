# Speech analysis — roadmap

Implemented in the tool today: QC/reliability, fluency (tempo, pauses, fillers, repeats), prosody (F0 σ / voiced / range — not full intonation), STT word confidence, CTC self-consistency with **letter-level** alignment plus a **reduction heuristic** (`phonology.rs`: о↔а, е↔и, voicing, etc.) so PER/substitutions are less misleading until real G2P exists; problems list (uncertain words, repetitions with playback; letter outliers vs recording baseline in technical details).

## Planned (see `todo/diction_analysis.md`)

| Area | Notes |
|------|--------|
| Formants VSA/VAI, sibilant spectra | LPC on stressed vowels; long recordings |
| Voice quality CPPS, jitter, shimmer | Voiced segments; separate “Voice” card |
| Stress, intonation (IC), rhythm nPVI | Accent lexicon + syllable F0 |
| Dual-STT intelligibility | Two decoders + alignment proxy (no script WER) |
| Real G2P + phoneme GOP | Replace `phonemize.rs` (e.g. espeak-ng) |
| Time-series score + bootstrap CI | 10–30 s windows; sparkline in UI |
| Rule-based recommendations | From `SpeechAnalysisProblems` + i18n templates |
| Read-aloud mode | Reference text → honest WER/CER |
