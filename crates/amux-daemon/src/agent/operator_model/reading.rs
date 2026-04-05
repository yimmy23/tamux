use super::*;

const MIN_SUMMARY_SIGNALS: u64 = 2;
const MIN_SKIP_REASONING_SIGNALS: u64 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadingSignal {
    None,
    SummaryRequest,
    SkipReasoning,
    DeepDetailRequest,
}

pub(crate) fn detect_reading_signal(content: &str) -> ReadingSignal {
    let lowered = content.trim().to_ascii_lowercase();
    if lowered.is_empty() {
        return ReadingSignal::None;
    }

    let skip_reasoning_markers = [
        "no reasoning",
        "skip the explanation",
        "skip explanation",
        "without explanation",
        "just the answer",
        "only the answer",
        "skip reasoning",
        "no explanation",
    ];
    if skip_reasoning_markers
        .iter()
        .any(|needle| lowered.contains(needle))
    {
        return ReadingSignal::SkipReasoning;
    }

    let summary_markers = [
        "tl;dr",
        "tldr",
        "summary",
        "summarize",
        "short version",
        "brief version",
        "concise version",
        "executive summary",
        "high level summary",
    ];
    if summary_markers
        .iter()
        .any(|needle| lowered.contains(needle))
    {
        return ReadingSignal::SummaryRequest;
    }

    let detail_markers = [
        "full trace",
        "step by step",
        "walk through",
        "full details",
        "deep dive",
        "show reasoning",
        "full reasoning",
        "root cause",
    ];
    if detail_markers.iter().any(|needle| lowered.contains(needle)) {
        return ReadingSignal::DeepDetailRequest;
    }

    ReadingSignal::None
}

pub(crate) fn reading_depth_for_profile(
    avg_words: f64,
    summary_requests: u64,
    detail_requests: u64,
    skip_reasoning_requests: u64,
) -> ReadingDepth {
    let mut score = match reading_depth_for_length(avg_words) {
        ReadingDepth::Skim => 0i32,
        ReadingDepth::Standard => 1,
        ReadingDepth::Deep => 2,
    };

    if summary_requests >= detail_requests + 2 {
        score -= 1;
    } else if detail_requests >= summary_requests + 2 {
        score += 1;
    }

    if skip_reasoning_requests >= MIN_SKIP_REASONING_SIGNALS {
        score -= 1;
    }

    match score.clamp(0, 2) {
        0 => ReadingDepth::Skim,
        1 => ReadingDepth::Standard,
        _ => ReadingDepth::Deep,
    }
}

pub(crate) fn refresh_reading_preferences(style: &mut CognitiveStyle) {
    style.prefers_summaries = style.summary_request_count >= MIN_SUMMARY_SIGNALS
        && style.summary_request_count >= style.detail_request_count + 1;
    style.skips_reasoning = style.reasoning_skip_request_count >= MIN_SKIP_REASONING_SIGNALS
        && style.reasoning_skip_request_count + style.summary_request_count
            >= style.detail_request_count + 2;
    style.reading_depth = reading_depth_for_profile(
        style.avg_message_length,
        style.summary_request_count,
        style.detail_request_count,
        style.reasoning_skip_request_count,
    );
}

pub(crate) fn record_reading_signal(style: &mut CognitiveStyle, signal: ReadingSignal) {
    match signal {
        ReadingSignal::SummaryRequest => {
            style.summary_request_count += 1;
        }
        ReadingSignal::SkipReasoning => {
            style.reasoning_skip_request_count += 1;
        }
        ReadingSignal::DeepDetailRequest => {
            style.detail_request_count += 1;
        }
        ReadingSignal::None => {}
    }
    refresh_reading_preferences(style);
}

pub(crate) fn reading_pattern_summary(style: &CognitiveStyle) -> Option<String> {
    if style.summary_request_count == 0
        && style.reasoning_skip_request_count == 0
        && style.detail_request_count == 0
    {
        return None;
    }

    let mut parts = vec![format!(
        "{} reading",
        reading_depth_label(style.reading_depth)
    )];
    if style.prefers_summaries {
        parts.push("summary-first".to_string());
    }
    if style.skips_reasoning {
        parts.push("reasoning on demand".to_string());
    }
    if style.detail_request_count > style.summary_request_count {
        parts.push("asks for full traces when needed".to_string());
    }

    Some(parts.join(", "))
}

fn reading_depth_label(value: ReadingDepth) -> &'static str {
    match value {
        ReadingDepth::Skim => "skim",
        ReadingDepth::Standard => "standard",
        ReadingDepth::Deep => "deep",
    }
}
