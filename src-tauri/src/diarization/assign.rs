//! Assign ASR / subtitle intervals to diarization speaker turns by max time overlap.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeakerInterval {
    pub speaker_id: u32,
    pub start_ms: u64,
    pub end_ms: u64,
}

/// Inclusive-exclusive overlap length in milliseconds.
pub fn overlap_ms(a_start: u64, a_end: u64, b_start: u64, b_end: u64) -> u64 {
    let start = a_start.max(b_start);
    let end = a_end.min(b_end);
    end.saturating_sub(start)
}

/// Pick the speaker with the largest time overlap. Ties keep the lower speaker id.
/// Returns `None` when there is no positive overlap.
pub fn speaker_by_max_overlap(
    start_ms: u64,
    end_ms: u64,
    intervals: &[SpeakerInterval],
) -> Option<u32> {
    let end_ms = end_ms.max(start_ms);
    let mut best: Option<(u32, u64)> = None;
    for interval in intervals {
        let overlap = overlap_ms(start_ms, end_ms, interval.start_ms, interval.end_ms);
        if overlap == 0 {
            continue;
        }
        match best {
            None => best = Some((interval.speaker_id, overlap)),
            Some((id, best_overlap)) => {
                if overlap > best_overlap
                    || (overlap == best_overlap && interval.speaker_id < id)
                {
                    best = Some((interval.speaker_id, overlap));
                }
            }
        }
    }
    best.map(|(id, _)| id)
}

pub fn assign_speakers_by_max_overlap(
    ranges: &[(u64, u64)],
    intervals: &[SpeakerInterval],
) -> Vec<Option<u32>> {
    ranges
        .iter()
        .map(|(start, end)| speaker_by_max_overlap(*start, *end, intervals))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(id: u32, start_ms: u64, end_ms: u64) -> SpeakerInterval {
        SpeakerInterval {
            speaker_id: id,
            start_ms,
            end_ms,
        }
    }

    #[test]
    fn picks_larger_overlap() {
        let intervals = [turn(0, 0, 1_000), turn(1, 800, 2_000)];
        assert_eq!(speaker_by_max_overlap(900, 1_500, &intervals), Some(1));
        assert_eq!(speaker_by_max_overlap(0, 500, &intervals), Some(0));
    }

    #[test]
    fn tie_prefers_lower_id() {
        let intervals = [turn(2, 0, 1_000), turn(1, 0, 1_000)];
        assert_eq!(speaker_by_max_overlap(0, 1_000, &intervals), Some(1));
    }

    #[test]
    fn no_overlap_returns_none() {
        let intervals = [turn(0, 0, 100)];
        assert_eq!(speaker_by_max_overlap(200, 300, &intervals), None);
    }

    #[test]
    fn assigns_batch() {
        let intervals = [turn(0, 0, 1_000), turn(1, 1_000, 2_000)];
        let ranges = [(0, 500), (1_200, 1_800), (5_000, 6_000)];
        assert_eq!(
            assign_speakers_by_max_overlap(&ranges, &intervals),
            vec![Some(0), Some(1), None]
        );
    }
}
