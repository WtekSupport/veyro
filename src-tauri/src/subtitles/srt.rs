use crate::timed_text::{TimedTextSegment, TimedWord};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubtitleLineEnding {
    CrLf,
    Lf,
}

impl SubtitleLineEnding {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CrLf => "\r\n",
            Self::Lf => "\n",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SubtitleOptions {
    pub max_line_length: usize,
    pub max_lines_per_cue: u8,
    pub max_cue_duration_ms: u64,
    pub min_cue_duration_ms: u64,
    pub pause_split_ms: u64,
    /// Post-speech display tail (`todo/TZ_audio_to_srt.md` §11.5.3).
    pub reading_tail_ms: u64,
    pub smart_split: bool,
}

impl Default for SubtitleOptions {
    fn default() -> Self {
        Self {
            max_line_length: 42,
            max_lines_per_cue: 2,
            max_cue_duration_ms: 7_000,
            min_cue_duration_ms: 1_000,
            pause_split_ms: 400,
            reading_tail_ms: 200,
            smart_split: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtitleCue {
    pub start_ms: u64,
    pub end_ms: u64,
    pub lines: Vec<String>,
}

pub fn format_srt_timestamp(ms: i64) -> String {
    let ms = ms.max(0) as u64;
    let hours = ms / 3_600_000;
    let minutes = (ms % 3_600_000) / 60_000;
    let seconds = (ms % 60_000) / 1_000;
    let millis = ms % 1_000;
    format!("{hours:02}:{minutes:02}:{seconds:02},{millis:03}")
}

/// After `.`/`?`/`!`, require at least this gap before a new cue (ms).
const SOFT_PAUSE_AFTER_SENTENCE_MS: u64 = 180;

/// Build SRT cues from word-level alignment output (see `todo/word-alignment-spec.md`, stage 2).
pub fn build_subtitle_cues_from_aligned_words(
    words: &[crate::word_align::WordTiming],
    options: &SubtitleOptions,
) -> Vec<SubtitleCue> {
    if words.is_empty() {
        return Vec::new();
    }

    let timed: Vec<TimedWord> = words
        .iter()
        .map(|word| TimedWord {
            text: word.text.clone(),
            start_ms: word.start_ms,
            end_ms: word.end_ms,
        })
        .collect();

    let mut cues = if options.smart_split {
        build_smart_cues_pause_first(&timed, options)
    } else {
        let mut out = Vec::new();
        for (start, end) in group_aligned_word_ranges_uniform(&timed, options) {
            let slice = &timed[start..end];
            out.extend(cues_from_timed_words(slice, options, None));
        }
        out
    };

    if !options.smart_split {
        merge_short_cues_uniform(&mut cues, options);
    }
    apply_cue_end_preservation(&mut cues, options);
    tighten_overlapping_cues(&mut cues);
    cues
}

/// Smart mode: long acoustic pauses first, then sentence boundaries, then line/duration limits.
fn build_smart_cues_pause_first(words: &[TimedWord], options: &SubtitleOptions) -> Vec<SubtitleCue> {
    let pause_ms = smart_acoustic_pause_ms(options);
    let mut cues = Vec::new();

    for (start, end) in split_word_ranges_on_pauses(words, pause_ms, true) {
        let phrase = &words[start..end];
        for (sub_start, sub_end) in split_phrase_on_sentence_boundaries(phrase, pause_ms) {
            let slice = &phrase[sub_start..sub_end];
            cues.extend(cues_from_timed_words(slice, options, Some(pause_ms)));
        }
    }

    cues
}

/// Split only on `.`/`?`/`!`/`…` when there is a real pause after the sentence (not every comma).
fn split_phrase_on_sentence_boundaries(
    words: &[TimedWord],
    pause_ms: u64,
) -> Vec<(usize, usize)> {
    if words.is_empty() {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    let mut block_start = 0usize;

    for index in 0..words.len().saturating_sub(1) {
        if !word_ends_sentence_boundary(words[index].text.as_str()) {
            continue;
        }
        let pause_before_next = inter_word_pause_ms(words, index + 1);
        if pause_before_next >= SOFT_PAUSE_AFTER_SENTENCE_MS
            || pause_before_next >= pause_ms.saturating_mul(2) / 5
        {
            ranges.push((block_start, index + 1));
            block_start = index + 1;
        }
    }
    ranges.push((block_start, words.len()));
    ranges
}

fn smart_acoustic_pause_ms(options: &SubtitleOptions) -> u64 {
    options.pause_split_ms.max(250)
}

fn split_word_ranges_on_pauses(
    words: &[TimedWord],
    pause_ms: u64,
    smart_semantic: bool,
) -> Vec<(usize, usize)> {
    if words.is_empty() {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    let mut block_start = 0usize;

    for index in 1..words.len() {
        let break_here = if smart_semantic {
            should_break_before_word(words, index, pause_ms)
        } else {
            inter_word_pause_ms(words, index) >= pause_ms
        };
        if break_here {
            ranges.push((block_start, index));
            block_start = index;
        }
    }
    ranges.push((block_start, words.len()));
    ranges
}

fn inter_word_pause_ms(words: &[TimedWord], word_index: usize) -> u64 {
    if word_index == 0 || word_index >= words.len() {
        return 0;
    }
    words[word_index]
        .start_ms
        .saturating_sub(words[word_index - 1].end_ms)
}

fn group_aligned_word_ranges_uniform(
    words: &[TimedWord],
    options: &SubtitleOptions,
) -> Vec<(usize, usize)> {
    split_word_ranges_on_pauses(words, options.pause_split_ms, false)
}

fn word_ends_sentence_boundary(word: &str) -> bool {
    word.trim()
        .chars()
        .last()
        .is_some_and(|ch| matches!(ch, '.' | '!' | '?' | '…'))
}

/// Pause-first, then boundary before a new clause (e.g. «…что?» | «И через…»).
fn should_break_before_word(words: &[TimedWord], word_index: usize, pause_ms: u64) -> bool {
    if word_index == 0 || word_index >= words.len() {
        return false;
    }

    let pause = inter_word_pause_ms(words, word_index);
    if pause >= pause_ms {
        return true;
    }

    let previous = words[word_index - 1].text.as_str();
    let next = words[word_index].text.as_str();
    if word_ends_sentence_boundary(previous) {
        if pause >= SOFT_PAUSE_AFTER_SENTENCE_MS {
            return true;
        }
        if is_hanging_word(next) {
            return true;
        }
    }

    false
}

pub fn build_subtitle_cues(
    segments: &[TimedTextSegment],
    options: &SubtitleOptions,
) -> Vec<SubtitleCue> {
    let mut cues = if options.smart_split {
        build_subtitle_cues_smart(segments, options)
    } else {
        build_subtitle_cues_uniform(segments, options)
    };
    cues.sort_by_key(|cue| cue.start_ms);
    apply_cue_end_preservation(&mut cues, options);
    tighten_overlapping_cues(&mut cues);
    cues
}

fn build_subtitle_cues_uniform(
    segments: &[TimedTextSegment],
    options: &SubtitleOptions,
) -> Vec<SubtitleCue> {
    let mut cues = Vec::new();
    let mut buffer: Vec<TimedTextSegment> = Vec::new();

    for segment in segments {
        let text = segment.text.trim();
        if text.is_empty() {
            continue;
        }
        let normalized = TimedTextSegment {
            text: text.to_string(),
            start_ms: segment.start_ms,
            end_ms: segment.end_ms.max(segment.start_ms),
            words: segment.words.clone(),
        };

        if let Some(last) = buffer.last() {
            let gap = normalized.start_ms.saturating_sub(last.end_ms);
            if gap >= options.pause_split_ms {
                flush_buffer_uniform(&mut buffer, options, &mut cues);
            }
        }
        buffer.push(normalized);
    }
    flush_buffer_uniform(&mut buffer, options, &mut cues);
    merge_short_cues_uniform(&mut cues, options);
    split_long_duration_cues_uniform(&mut cues, options);
    cues
}

fn build_subtitle_cues_smart(
    segments: &[TimedTextSegment],
    options: &SubtitleOptions,
) -> Vec<SubtitleCue> {
    let mut cues = Vec::new();

    for segment in segments {
        let text = segment.text.trim();
        if text.is_empty() {
            continue;
        }
        let normalized = TimedTextSegment {
            text: text.to_string(),
            start_ms: segment.start_ms,
            end_ms: segment.end_ms.max(segment.start_ms),
            words: segment.words.clone(),
        };
        cues.extend(cues_from_stt_segment(&normalized, options));
    }

    cues
}

/// One Whisper/STT segment = authoritative speech interval. Sub-cues only subdivide *within* it.
fn cues_from_stt_segment(
    segment: &TimedTextSegment,
    options: &SubtitleOptions,
) -> Vec<SubtitleCue> {
    if segment.has_word_timings() {
        return cues_from_timed_words(&segment.words, options, None);
    }

    let words: Vec<&str> = segment.text.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }

    let seg_start = segment.start_ms;
    let seg_end = segment.end_ms.max(seg_start);
    let total_words = words.len();
    let duration = seg_end.saturating_sub(seg_start).max(1);

    let max_lines = options.max_lines_per_cue.max(1) as usize;
    let max_len = options.max_line_length.max(10);

    let mut cues = Vec::new();
    let mut word_start = 0usize;

    while word_start < total_words {
        let mut word_end = word_start + 1;
        let mut best_end = word_end;

        while word_end <= total_words {
            let slice = &words[word_start..word_end];
            let lines = wrap_lines_smart(slice, max_len, max_lines);
            if lines.len() > max_lines {
                break;
            }

            let word_count = (word_end - word_start) as u64;
            let chunk_duration = duration.saturating_mul(word_count) / total_words as u64;
            if chunk_duration > options.max_cue_duration_ms && word_end > word_start + 1 {
                break;
            }

            best_end = word_end;
            if word_end == total_words {
                break;
            }
            word_end += 1;
        }

        if best_end <= word_start {
            best_end = word_start + 1;
        }

        let slice = &words[word_start..best_end];
        let lines = wrap_lines_smart(slice, max_len, max_lines);
        if !lines.is_empty() {
            let cue_start = seg_start + duration * word_start as u64 / total_words as u64;
            let cue_end = if best_end == total_words {
                seg_end
            } else {
                seg_start + duration * best_end as u64 / total_words as u64
            };
            cues.push(SubtitleCue {
                start_ms: cue_start,
                end_ms: cue_end.max(cue_start),
                lines,
            });
        }
        word_start = best_end;
    }

    cues
}

fn cues_from_timed_words(
    words: &[TimedWord],
    options: &SubtitleOptions,
    pause_break_ms: Option<u64>,
) -> Vec<SubtitleCue> {
    if words.is_empty() {
        return Vec::new();
    }

    let max_lines = options.max_lines_per_cue.max(1) as usize;
    let max_len = options.max_line_length.max(10);
    let mut cues = Vec::new();
    let mut start_index = 0usize;

    while start_index < words.len() {
        let mut end_index = start_index + 1;
        let mut best_end = end_index;

        while end_index <= words.len() {
            if end_index > start_index + 1 {
                if let Some(threshold) = pause_break_ms {
                    if should_break_before_word(words, end_index, threshold) {
                        break;
                    }
                }
            }

            let slice = &words[start_index..end_index];
            let token_strs: Vec<&str> = slice.iter().map(|w| w.text.as_str()).collect();
            let lines = if options.smart_split {
                wrap_lines_smart(&token_strs, max_len, max_lines)
            } else {
                wrap_words_to_lines(&token_strs, max_len)
            };
            if lines.len() > max_lines {
                break;
            }
            let duration = slice
                .last()
                .map(|w| w.end_ms)
                .unwrap_or(0)
                .saturating_sub(slice.first().map(|w| w.start_ms).unwrap_or(0));
            if duration > options.max_cue_duration_ms && end_index > start_index + 1 {
                break;
            }
            best_end = end_index;
            if end_index == words.len() {
                break;
            }
            end_index += 1;
        }

        if best_end <= start_index {
            best_end = start_index + 1;
        }

        let slice = &words[start_index..best_end];
        let token_strs: Vec<&str> = slice.iter().map(|w| w.text.as_str()).collect();
        let lines = wrap_lines_smart(&token_strs, max_len, max_lines);
        if !lines.is_empty() {
            let cue_start = slice.first().map(|w| w.start_ms).unwrap_or(0);
            let cue_end = slice.last().map(|w| w.end_ms).unwrap_or(cue_start);
            cues.push(SubtitleCue {
                start_ms: cue_start,
                end_ms: cue_end.max(cue_start),
                lines,
            });
        }
        start_index = best_end;
    }

    cues
}

fn flush_buffer_uniform(
    buffer: &mut Vec<TimedTextSegment>,
    options: &SubtitleOptions,
    cues: &mut Vec<SubtitleCue>,
) {
    if buffer.is_empty() {
        return;
    }

    let start_ms = buffer.first().map(|s| s.start_ms).unwrap_or(0);
    let end_ms = buffer.iter().map(|s| s.end_ms).max().unwrap_or(start_ms);
    let words: Vec<&str> = buffer
        .iter()
        .flat_map(|s| s.text.split_whitespace())
        .collect();

    if words.is_empty() {
        buffer.clear();
        return;
    }

    let max_lines = options.max_lines_per_cue.max(1) as usize;
    let max_len = options.max_line_length.max(10);
    let wrapped_lines = wrap_words_to_lines(&words, max_len);
    let line_groups = group_lines_for_cues(&wrapped_lines, max_lines);

    cues.extend(distribute_cues_over_time(
        line_groups,
        start_ms,
        end_ms,
        options,
    ));

    buffer.clear();
}

/// Preserve visible silence between cues (`todo/TZ_audio_to_srt.md` §11.5.3).
fn apply_cue_end_preservation(cues: &mut [SubtitleCue], options: &SubtitleOptions) {
    let min_gap = options.pause_split_ms;
    let tail = options.reading_tail_ms;

    for index in 0..cues.len().saturating_sub(1) {
        let real_end = cues[index].end_ms;
        let next_start = cues[index + 1].start_ms;
        let gap = next_start.saturating_sub(real_end);

        if gap >= min_gap {
            let extended = real_end.saturating_add(tail);
            let cap = next_start.saturating_sub(min_gap);
            cues[index].end_ms = extended.min(cap).max(cues[index].start_ms.saturating_add(1));
        } else {
            cues[index].end_ms = real_end;
        }
    }
}

/// Only trim overlaps when cues are back-to-back (no pause to preserve).
fn tighten_overlapping_cues(cues: &mut [SubtitleCue]) {
    for index in 0..cues.len().saturating_sub(1) {
        let next_start = cues[index + 1].start_ms;
        if cues[index].end_ms > next_start {
            cues[index].end_ms = next_start.saturating_sub(1);
        }
        if cues[index].end_ms <= cues[index].start_ms {
            cues[index].end_ms = cues[index].start_ms.saturating_add(1);
        }
    }
}

fn is_hanging_word(word: &str) -> bool {
    let lower = word.to_lowercase();
    const PARTICLES: &[&str] = &[
        "и", "в", "во", "на", "с", "со", "к", "ко", "у", "о", "об", "от", "до", "за", "из", "по",
        "а", "но", "не", "ни", "бы", "ли", "же", "a", "an", "the", "in", "on", "at", "to", "of",
        "for", "or", "and", "but",
    ];
    word.chars().count() <= 2 || PARTICLES.contains(&lower.as_str())
}

fn wrap_lines_smart(words: &[&str], max_len: usize, max_lines: usize) -> Vec<String> {
    let mut lines = wrap_words_to_lines(words, max_len);
    if lines.len() <= 1 {
        return lines;
    }

    while lines.len() > max_lines {
        let overflow = lines.pop().unwrap_or_default();
        if let Some(last) = lines.last_mut() {
            if !last.is_empty() {
                last.push(' ');
            }
            last.push_str(&overflow);
        } else {
            lines.push(overflow);
            break;
        }
    }

    if lines.len() == 2 && max_lines >= 2 {
        fix_orphan_second_line(&mut lines, max_len);
    }

    lines.truncate(max_lines);
    lines
}

fn fix_orphan_second_line(lines: &mut [String], max_len: usize) {
    if lines.len() != 2 {
        return;
    }
    let second_words: Vec<&str> = lines[1].split_whitespace().collect();
    if second_words.len() != 1 {
        return;
    }
    let orphan = second_words[0];
    if !is_hanging_word(orphan) && orphan.chars().count() > 4 {
        return;
    }

    let first_words: Vec<&str> = lines[0].split_whitespace().collect();
    if first_words.len() <= 1 {
        return;
    }

    let moved = first_words[first_words.len() - 1];
    let first_without: Vec<&str> = first_words[..first_words.len() - 1].to_vec();
    let new_first = first_without.join(" ");
    let new_second = format!("{moved} {orphan}");

    if new_first.len() <= max_len && new_second.len() <= max_len {
        lines[0] = new_first;
        lines[1] = new_second;
    }
}

fn distribute_cues_over_time(
    line_groups: Vec<Vec<String>>,
    start_ms: u64,
    end_ms: u64,
    options: &SubtitleOptions,
) -> Vec<SubtitleCue> {
    if line_groups.is_empty() {
        return Vec::new();
    }

    let group_count = line_groups.len();
    let total_duration = end_ms.saturating_sub(start_ms).max(1);
    let mut chunk_start = start_ms;
    let mut out = Vec::with_capacity(group_count);

    for (index, lines) in line_groups.into_iter().enumerate() {
        if lines.is_empty() {
            continue;
        }
        let cue_end = if index + 1 == group_count {
            end_ms
        } else {
            let share = total_duration / group_count as u64;
            (chunk_start + share.max(options.min_cue_duration_ms)).min(end_ms)
        };
        let cue_start = chunk_start;
        chunk_start = cue_end;
        out.push(SubtitleCue {
            start_ms: cue_start,
            end_ms: cue_end.max(cue_start + options.min_cue_duration_ms),
            lines,
        });
    }

    out
}

fn group_lines_for_cues(lines: &[String], max_lines: usize) -> Vec<Vec<String>> {
    if lines.is_empty() {
        return Vec::new();
    }
    let mut groups = Vec::new();
    let mut chunk = Vec::new();
    for line in lines {
        chunk.push(line.clone());
        if chunk.len() >= max_lines {
            groups.push(chunk);
            chunk = Vec::new();
        }
    }
    if !chunk.is_empty() {
        groups.push(chunk);
    }
    groups
}

fn wrap_words_to_lines(words: &[&str], max_len: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in words {
        if current.is_empty() {
            current = word.to_string();
            continue;
        }
        if current.len() + 1 + word.len() <= max_len {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn merge_short_cues_uniform(cues: &mut Vec<SubtitleCue>, options: &SubtitleOptions) {
    merge_short_cues_with_redistribute(cues, options);
}

fn merge_short_cues_with_redistribute(cues: &mut Vec<SubtitleCue>, options: &SubtitleOptions) {
    let mut index = 0;
    while index < cues.len() {
        let duration = cues[index].end_ms.saturating_sub(cues[index].start_ms);
        if duration >= options.min_cue_duration_ms || index + 1 >= cues.len() {
            index += 1;
            continue;
        }
        let next = cues.remove(index + 1);
        let current = &mut cues[index];
        current.end_ms = current.end_ms.max(next.end_ms);
        if !next.lines.is_empty() {
            current.lines.extend(next.lines);
        }
        if current.lines.len() > options.max_lines_per_cue as usize {
            let max_lines = options.max_lines_per_cue as usize;
            let words: Vec<&str> = current
                .lines
                .iter()
                .flat_map(|l| l.split_whitespace())
                .collect();
            let wrapped = wrap_words_to_lines(&words, options.max_line_length.max(10));
            let groups = group_lines_for_cues(&wrapped, max_lines);
            let start = current.start_ms;
            let end = current.end_ms;
            let replacement = distribute_cues_over_time(groups, start, end, options);
            if replacement.len() == 1 {
                *current = replacement.into_iter().next().unwrap();
            } else if !replacement.is_empty() {
                cues.splice(index..=index, replacement);
            }
        }
    }
}

fn split_long_duration_cues_uniform(cues: &mut Vec<SubtitleCue>, options: &SubtitleOptions) {
    let mut index = 0;
    while index < cues.len() {
        let duration = cues[index].end_ms.saturating_sub(cues[index].start_ms);
        if duration <= options.max_cue_duration_ms || cues[index].lines.is_empty() {
            index += 1;
            continue;
        }

        let cue = cues.remove(index);
        let words: Vec<&str> = cue.lines.iter().flat_map(|l| l.split_whitespace()).collect();
        let wrapped = wrap_words_to_lines(&words, options.max_line_length.max(10));
        let max_lines = options.max_lines_per_cue.max(1) as usize;
        let groups = group_lines_for_cues(&wrapped, max_lines);
        let split = distribute_cues_over_time(groups, cue.start_ms, cue.end_ms, options);
        if split.is_empty() {
            cues.insert(index, cue);
            index += 1;
        } else {
            let inserted = split.len();
            cues.splice(index..index, split);
            index += inserted;
        }
    }
}

pub fn render_srt(
    cues: &[SubtitleCue],
    line_ending: SubtitleLineEnding,
    global_offset_ms: i64,
    utf8_bom: bool,
) -> String {
    let eol = line_ending.as_str();
    let mut out = String::new();
    if utf8_bom {
        out.push('\u{feff}');
    }

    for (index, cue) in cues.iter().enumerate() {
        if cue.lines.is_empty() {
            continue;
        }
        let start = format_srt_timestamp(global_offset_ms + cue.start_ms as i64);
        let end = format_srt_timestamp(global_offset_ms + cue.end_ms as i64);
        out.push_str(&(index + 1).to_string());
        out.push_str(eol);
        out.push_str(&start);
        out.push_str(" --> ");
        out.push_str(&end);
        out.push_str(eol);
        for line in &cue.lines {
            out.push_str(line);
            out.push_str(eol);
        }
        out.push_str(eol);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(text: &str, start: u64, end: u64) -> TimedTextSegment {
        TimedTextSegment {
            text: text.to_string(),
            start_ms: start,
            end_ms: end,
            words: Vec::new(),
        }
    }

    fn seg_with_words(text: &str, start: u64, end: u64, words: Vec<TimedWord>) -> TimedTextSegment {
        TimedTextSegment {
            text: text.to_string(),
            start_ms: start,
            end_ms: end,
            words,
        }
    }

    fn all_words(cues: &[SubtitleCue]) -> Vec<String> {
        cues
            .iter()
            .flat_map(|cue| cue.lines.iter().flat_map(|l| l.split_whitespace()))
            .map(str::to_string)
            .collect()
    }

    fn options_uniform() -> SubtitleOptions {
        SubtitleOptions {
            smart_split: false,
            ..Default::default()
        }
    }

    fn options_smart() -> SubtitleOptions {
        SubtitleOptions {
            smart_split: true,
            ..Default::default()
        }
    }

    #[test]
    fn timestamp_format() {
        assert_eq!(format_srt_timestamp(0), "00:00:00,000");
        assert_eq!(format_srt_timestamp(2_340), "00:00:02,340");
        assert_eq!(format_srt_timestamp(3_661_001), "01:01:01,001");
    }

    #[test]
    fn render_srt_numbering_and_block() {
        let cues = vec![SubtitleCue {
            start_ms: 0,
            end_ms: 2340,
            lines: vec!["Hello world".to_string()],
        }];
        let srt = render_srt(&cues, SubtitleLineEnding::Lf, 0, false);
        assert!(srt.starts_with("1\n00:00:00,000 --> 00:00:02,340\nHello world\n"));
    }

    #[test]
    fn global_offset_applied() {
        let cues = vec![SubtitleCue {
            start_ms: 1000,
            end_ms: 2000,
            lines: vec!["Hi".to_string()],
        }];
        let srt = render_srt(&cues, SubtitleLineEnding::Lf, 500, false);
        assert!(srt.contains("00:00:01,500 --> 00:00:02,500"));
    }

    #[test]
    fn merge_short_cues() {
        let options = SubtitleOptions {
            smart_split: false,
            min_cue_duration_ms: 1000,
            ..Default::default()
        };
        let segments = vec![seg("one", 0, 200), seg("two", 250, 400)];
        let cues = build_subtitle_cues(&segments, &options);
        assert_eq!(cues.len(), 1);
    }

    #[test]
    fn pause_splits_blocks() {
        let options = SubtitleOptions {
            smart_split: false,
            pause_split_ms: 300,
            ..Default::default()
        };
        let segments = vec![
            seg("first phrase", 0, 1000),
            seg("second phrase", 2000, 3000),
        ];
        let cues = build_subtitle_cues(&segments, &options);
        assert_eq!(cues.len(), 2);
    }

    #[test]
    fn long_speech_uniform_without_dropping_words() {
        let words: Vec<String> = (0..80).map(|i| format!("word{i}")).collect();
        let text = words.join(" ");
        let options = options_uniform();
        let segments = vec![seg(&text, 0, 60_000)];
        let cues = build_subtitle_cues(&segments, &options);
        assert!(cues.len() > 1, "expected multiple cues, got {}", cues.len());
        assert_eq!(all_words(&cues), words);
    }

    #[test]
    fn smart_long_speech_keeps_all_words() {
        let words: Vec<String> = (0..60).map(|i| format!("word{i}")).collect();
        let text = words.join(" ");
        let segments = vec![seg(&text, 0, 30_000)];
        let cues = build_subtitle_cues(&segments, &options_smart());
        assert!(cues.len() > 1);
        assert_eq!(all_words(&cues), words);
    }

    #[test]
    fn smart_uses_whisper_segment_timings() {
        let options = options_smart();
        let segments = vec![
            seg("first block", 6_387, 9_100),
            seg("second block", 9_600, 12_820),
        ];
        let cues = build_subtitle_cues(&segments, &options);
        assert!(cues.iter().any(|c| c.start_ms == 6_387 && c.end_ms >= 9_100));
        assert!(cues.iter().any(|c| c.start_ms == 9_600 && c.end_ms == 12_820));
        assert!(cues[0].end_ms < cues[1].start_ms);
    }

    #[test]
    fn regression_must_not_set_end_equal_to_next_start_when_gap_exists() {
        let segments = vec![
            seg("line one", 0, 2_000),
            seg("line two", 3_500, 5_000),
        ];
        let options = SubtitleOptions::default();
        let cues = build_subtitle_cues(&segments, &options);
        assert_eq!(cues.len(), 2);
        assert_ne!(
            cues[0].end_ms, cues[1].start_ms,
            "glued end==next.start is the §11.5 bug"
        );
    }

    #[test]
    fn tz_pause_between_replicas_preserved() {
        let options = SubtitleOptions {
            pause_split_ms: 150,
            reading_tail_ms: 200,
            smart_split: false,
            ..Default::default()
        };
        let segments = vec![seg("Replica A", 0, 2_000), seg("Replica B", 3_500, 5_000)];
        let cues = build_subtitle_cues(&segments, &options);
        assert_eq!(cues.len(), 2);
        let gap = cues[1].start_ms.saturating_sub(cues[0].end_ms);
        assert!(
            gap >= 150,
            "expected visible pause between cues, gap={gap}ms (A end {}, B start {})",
            cues[0].end_ms,
            cues[1].start_ms
        );
    }

    #[test]
    fn aligned_words_pipeline_keeps_pause() {
        use crate::word_align::WordTiming;

        let words = vec![
            WordTiming {
                text: "one".to_string(),
                start_ms: 100,
                end_ms: 400,
            },
            WordTiming {
                text: "two".to_string(),
                start_ms: 900,
                end_ms: 1200,
            },
        ];
        let cues = build_subtitle_cues_from_aligned_words(&words, &options_smart());
        assert_eq!(cues.len(), 2);
        assert!(cues[0].end_ms < cues[1].start_ms);
    }

    #[test]
    fn smart_breaks_before_conjunction_after_question() {
        use crate::word_align::WordTiming;

        let words = vec![
            WordTiming {
                text: "пишу".to_string(),
                start_ms: 6_470,
                end_ms: 6_800,
            },
            WordTiming {
                text: "или".to_string(),
                start_ms: 6_820,
                end_ms: 7_000,
            },
            WordTiming {
                text: "что?".to_string(),
                start_ms: 7_020,
                end_ms: 9_255,
            },
            WordTiming {
                text: "И".to_string(),
                start_ms: 9_260,
                end_ms: 9_350,
            },
            WordTiming {
                text: "через".to_string(),
                start_ms: 9_465,
                end_ms: 9_800,
            },
            WordTiming {
                text: "телеграм".to_string(),
                start_ms: 9_820,
                end_ms: 10_235,
            },
        ];
        let cues = build_subtitle_cues_from_aligned_words(&words, &options_smart());
        let idx = cues
            .iter()
            .position(|cue| cue.lines.iter().any(|l| l.contains("что?")))
            .expect("cue with что?");
        let text = cues[idx].lines.join(" ");
        assert!(
            !text.contains(" И") && !text.ends_with('И'),
            "conjunction should start next cue, got: {text}"
        );
        let next_text = cues[idx + 1].lines.join(" ");
        assert!(
            next_text.starts_with("И") || next_text.contains("И через"),
            "next cue should start with И, got: {next_text}"
        );
    }

    #[test]
    fn smart_splits_on_moderate_pause_before_semantics() {
        use crate::word_align::WordTiming;

        let words = vec![
            WordTiming {
                text: "hello".to_string(),
                start_ms: 0,
                end_ms: 300,
            },
            WordTiming {
                text: "world".to_string(),
                start_ms: 780,
                end_ms: 900,
            },
        ];
        let options = SubtitleOptions {
            smart_split: true,
            pause_split_ms: 400,
            ..Default::default()
        };
        let cues = build_subtitle_cues_from_aligned_words(&words, &options);
        assert_eq!(cues.len(), 2, "480ms gap should split with pause_split_ms 400");
    }

    #[test]
    fn smart_keeps_comma_phrase_in_one_cue_without_long_pause() {
        use crate::word_align::WordTiming;

        let words = vec![
            WordTiming {
                text: "Привет,".to_string(),
                start_ms: 0,
                end_ms: 400,
            },
            WordTiming {
                text: "мир".to_string(),
                start_ms: 410,
                end_ms: 800,
            },
        ];
        let cues = build_subtitle_cues_from_aligned_words(&words, &options_smart());
        assert_eq!(cues.len(), 1, "short gap after comma should stay one cue");
    }

    #[test]
    fn smart_word_timings_keep_silence_between_cues() {
        let options = SubtitleOptions {
            max_cue_duration_ms: 500,
            reading_tail_ms: 0,
            ..options_smart()
        };
        let words = vec![
            TimedWord {
                text: "alpha".to_string(),
                start_ms: 1_000,
                end_ms: 1_400,
            },
            TimedWord {
                text: "beta".to_string(),
                start_ms: 1_900,
                end_ms: 2_300,
            },
        ];
        let segments = vec![seg_with_words("alpha beta", 1_000, 2_300, words)];
        let cues = build_subtitle_cues(&segments, &options);
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].end_ms, 1_400);
        assert_eq!(cues[1].start_ms, 1_900);
        assert!(cues[0].end_ms < cues[1].start_ms);
    }

    #[test]
    fn smart_preserves_silence_between_stt_segments() {
        let options = options_smart();
        let segments = vec![
            seg("first block ends", 6_000, 9_000),
            seg("second block starts", 9_600, 12_800),
        ];
        let cues = build_subtitle_cues(&segments, &options);
        assert!(cues.len() >= 2);
        assert!(
            cues[0].end_ms < cues[1].start_ms,
            "expected silence gap, got {} -> {}",
            cues[0].end_ms,
            cues[1].start_ms
        );
    }

    #[test]
    fn smart_wrap_avoids_lone_particle_line() {
        let words = vec![
            "мы", "идём", "в", "магазин", "за", "хлебом", "и", "молоком", "сегодня", "вечером",
        ];
        let lines = wrap_lines_smart(&words.iter().map(|s| *s).collect::<Vec<_>>(), 12, 2);
        assert_eq!(lines.len(), 2);
        let second_count = lines[1].split_whitespace().count();
        assert!(
            second_count > 1 || !is_hanging_word(lines[1].split_whitespace().next().unwrap()),
            "second line should not be a lone particle: {:?}",
            lines
        );
    }
}
