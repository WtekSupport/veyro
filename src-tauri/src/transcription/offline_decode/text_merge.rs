const MIN_OVERLAP_WORDS: usize = 4;

pub fn merge_transcript_pieces(pieces: &[String]) -> String {
    if pieces.is_empty() {
        return String::new();
    }
    let mut merged = pieces[0].trim().to_string();
    for piece in pieces.iter().skip(1) {
        let next = piece.trim();
        if next.is_empty() {
            continue;
        }
        merged = merge_two_transcripts(&merged, next);
    }
    merged
}

fn merge_two_transcripts(left: &str, right: &str) -> String {
    let lw: Vec<&str> = left.split_whitespace().collect();
    let rw: Vec<&str> = right.split_whitespace().collect();
    if lw.is_empty() {
        return right.to_string();
    }
    if rw.is_empty() {
        return left.to_string();
    }
    let max_n = lw.len().min(rw.len());
    for n in (MIN_OVERLAP_WORDS..=max_n).rev() {
        if lw[lw.len() - n..] == rw[..n] {
            let tail = rw[n..].join(" ");
            if tail.is_empty() {
                return left.to_string();
            }
            return format!("{left} {tail}");
        }
    }
    format!("{left} {right}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupes_repeated_phrase_at_boundary() {
        let a = "alpha beta gamma delta";
        let b = "alpha beta gamma delta epsilon";
        let merged = merge_two_transcripts(a, b);
        assert_eq!(merged, "alpha beta gamma delta epsilon");
    }
}
