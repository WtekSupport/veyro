/// Apply user-defined post-STT corrections (longest match first).
pub fn apply_corrections(text: &str, corrections: &[(String, String)]) -> String {
    if corrections.is_empty() || text.is_empty() {
        return text.to_string();
    }

    let mut result = text.to_string();
    for (from, to) in corrections {
        if from.is_empty() {
            continue;
        }
        result = replace_all_flexible(&result, from, to);
    }
    collapse_whitespace(&result)
}

fn is_dash(c: char) -> bool {
    matches!(c, '-' | '‑' | '–' | '—' | '−')
}

fn is_flex_separator(c: char) -> bool {
    is_dash(c)
        || c.is_whitespace()
        || matches!(c, ',' | '.' | ':' | ';' | '!' | '?')
}

struct NormalizedViews {
    flex: Vec<char>,
    flex_to_orig: Vec<usize>,
    compact: Vec<char>,
    compact_to_orig: Vec<usize>,
}

fn build_views(text: &str) -> NormalizedViews {
    let orig: Vec<char> = text.chars().collect();
    let mut flex = Vec::new();
    let mut flex_to_orig = Vec::new();
    let mut compact = Vec::new();
    let mut compact_to_orig = Vec::new();

    let mut index = 0usize;
    while index < orig.len() {
        let ch = orig[index];
        if is_flex_separator(ch) {
            if !flex.is_empty() && flex.last() != Some(&' ') {
                flex.push(' ');
                flex_to_orig.push(index);
            }
            index += 1;
            continue;
        }

        for lower in ch.to_lowercase() {
            flex.push(lower);
            flex_to_orig.push(index);
            compact.push(lower);
            compact_to_orig.push(index);
        }
        index += 1;
    }

    if flex.last() == Some(&' ') {
        flex.pop();
        flex_to_orig.pop();
    }

    NormalizedViews {
        flex,
        flex_to_orig,
        compact,
        compact_to_orig,
    }
}

fn replace_all_flexible(text: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return text.to_string();
    }

    let from_views = build_views(from);
    let text_views = build_views(text);
    let mut ranges = Vec::new();

    collect_matches(
        &text_views.flex,
        &text_views.flex_to_orig,
        &from_views.flex,
        &mut ranges,
    );
    collect_matches(
        &text_views.compact,
        &text_views.compact_to_orig,
        &from_views.compact,
        &mut ranges,
    );

    if ranges.is_empty() {
        return text.to_string();
    }

    ranges.sort_by_key(|(start, _)| *start);
    ranges.dedup();

    let orig_len = text.chars().count();
    let mut merged = Vec::new();
    for (start, end) in ranges {
        if start >= end || end > orig_len {
            continue;
        }
        if merged
            .last()
            .is_some_and(|(_last_start, last_end)| start < *last_end)
        {
            continue;
        }
        merged.push((start, end));
    }

    let mut result = String::with_capacity(text.len());
    let mut index = 0usize;
    for (start, end) in merged {
        let (byte_start, _byte_end) = char_range_to_bytes(text, start, end);
        result.push_str(&text[char_index_to_byte(text, index)..byte_start]);
        result.push_str(to);
        index = end;
    }
    result.push_str(&text[char_index_to_byte(text, index)..]);
    result
}

fn collect_matches(
    haystack: &[char],
    haystack_to_orig: &[usize],
    needle: &[char],
    ranges: &mut Vec<(usize, usize)>,
) {
    if needle.is_empty() || haystack.len() < needle.len() {
        return;
    }

    let mut search_from = 0usize;
    while search_from <= haystack.len().saturating_sub(needle.len()) {
        if haystack[search_from..search_from + needle.len()] == *needle {
            let end_index = search_from + needle.len() - 1;
            let start = haystack_to_orig[search_from];
            let end = haystack_to_orig[end_index] + 1;
            ranges.push((start, end));
            search_from += needle.len();
        } else {
            search_from += 1;
        }
    }
}

fn char_index_to_byte(text: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }
    text.char_indices()
        .nth(char_index)
        .map(|(byte_index, _)| byte_index)
        .unwrap_or(text.len())
}

fn char_range_to_bytes(text: &str, start_char: usize, end_char: usize) -> (usize, usize) {
    (
        char_index_to_byte(text, start_char),
        char_index_to_byte(text, end_char),
    )
}

fn collapse_whitespace(text: &str) -> String {
    text.split("\n\n")
        .map(|block| block.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|block| !block.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items
            .iter()
            .map(|(from, to)| (from.to_string(), to.to_string()))
            .collect()
    }

    #[test]
    fn collapse_whitespace_preserves_paragraph_breaks() {
        assert_eq!(
            collapse_whitespace("Первый абзац.\n\nВторой абзац."),
            "Первый абзац.\n\nВторой абзац."
        );
    }

    #[test]
    fn longest_correction_wins_when_sorted() {
        let corrections = pairs(&[
            ("число с лать", ""),
            ("с лать", "послать"),
            ("лать", "X"),
        ]);
        let sorted = sort_corrections(corrections);
        assert_eq!(
            apply_corrections("раз два три, число с лать", &sorted),
            "раз два три,"
        );
    }

    #[test]
    fn case_insensitive_replacement() {
        let corrections = sort_corrections(pairs(&[("veyro", "Veyro")]));
        assert_eq!(
            apply_corrections("welcome to veyro", &corrections),
            "welcome to Veyro"
        );
    }

    #[test]
    fn fixes_whisper_mishearing_for_brand_names() {
        let corrections = sort_corrections(pairs(&[
            ("клебос", "ClipBoss"),
            ("протокол инициации", "Protocol Initiation"),
            ("сайклозер", "PsyCloser"),
        ]));
        assert_eq!(
            apply_corrections("Клебос. Протокол инициации: Сайклозер.", &corrections),
            "ClipBoss. Protocol Initiation: PsyCloser."
        );
    }

    #[test]
    fn fixes_llm_invented_russian_words() {
        let corrections = sort_corrections(pairs(&[
            ("клитус", "ClipBoss"),
            ("паркакулы", "Protocol Initiation"),
            ("свирфин", "PsyCloser"),
            ("санк лозер", "PsyCloser"),
        ]));
        assert_eq!(
            apply_corrections("Клитус, Паркакулы, Свирфин, Санк Лозер.", &corrections),
            "ClipBoss, Protocol Initiation, PsyCloser, PsyCloser."
        );
    }

    #[test]
    fn fixes_amnestic_protocol_and_clip_boz() {
        let corrections = sort_corrections(pairs(&[
            ("псайклозер", "PsyCloser"),
            ("протокол и амнициационный", "Protocol Initiation"),
            ("клип боз", "ClipBoss"),
        ]));
        assert_eq!(
            apply_corrections(
                "Псайклозер, протокол и амнициационный клип БОЗ.",
                &corrections
            ),
            "PsyCloser, Protocol Initiation ClipBoss."
        );
    }

    #[test]
    fn matches_comma_separated_whisper_output() {
        let corrections = sort_corrections(pairs(&[
            ("клейболос", "ClipBoss"),
            ("протокол донансэйшн", "Protocol Initiation"),
            ("псайк лоузер", "PsyCloser"),
        ]));
        assert_eq!(
            apply_corrections("Псайк, лоузер, клейболос, протокол донансэйшн.", &corrections),
            "PsyCloser, ClipBoss, Protocol Initiation."
        );
    }

    #[test]
    fn matches_hyphenated_whisper_output() {
        let corrections = sort_corrections(pairs(&[
            ("лип босс", "ClipBoss"),
            ("протокол и сессия", "Protocol Initiation"),
            ("псайклозер", "PsyCloser"),
        ]));
        assert_eq!(
            apply_corrections("Лип-босс. Протокол и сессия. Псайк-лозер.", &corrections),
            "ClipBoss. Protocol Initiation. PsyCloser."
        );
    }

    #[test]
    fn empty_replacement_removes_fragment() {
        let corrections = sort_corrections(pairs(&[("чисто с лать", "")]));
        assert_eq!(
            apply_corrections("Раз, два, три — чисто с лать.", &corrections),
            "Раз, два, три — ."
        );
    }
}

/// Sort corrections longest-first so multi-word patterns match before substrings.
pub fn sort_corrections(mut corrections: Vec<(String, String)>) -> Vec<(String, String)> {
    corrections.sort_by(|(left_from, _), (right_from, _)| {
        right_from
            .chars()
            .count()
            .cmp(&left_from.chars().count())
    });
    corrections
}
