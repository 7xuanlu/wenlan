// SPDX-License-Identifier: Apache-2.0
//! Temporal cue extraction from natural-language queries.
//!
//! Plan B's L1 retrieval uses [`extract_cue`] to drive the temporal channel.
//! This module ships the yesterday/today/tomorrow patterns; Tasks 7-8 add
//! week/month/year, weekday, N-ago, quarter, and since patterns.

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use regex::Regex;
use std::sync::LazyLock;

/// A UTC time range. Both endpoints are Unix timestamps (seconds since epoch).
/// Start is inclusive; end is inclusive (23:59:59 of the day, not midnight).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateRange {
    pub start: i64,
    pub end: i64,
}

/// Confidence in the extracted cue. `Low` fires near period boundaries where
/// the user's intent is ambiguous (e.g. "last week" at Sunday 23:59).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CueConfidence {
    High,
    Low,
}

/// A temporal cue extracted from a query string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractedCue {
    pub range: DateRange,
    pub confidence: CueConfidence,
}

static RE_YESTERDAY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\byesterday\b").unwrap());
static RE_TODAY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\btoday\b").unwrap());
static RE_TOMORROW: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\btomorrow\b").unwrap());

fn full_day_range(d: NaiveDate) -> DateRange {
    let start = Utc
        .from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap())
        .timestamp();
    let end = Utc
        .from_utc_datetime(&d.and_hms_opt(23, 59, 59).unwrap())
        .timestamp();
    DateRange { start, end }
}

/// Extract a temporal cue from a natural-language query.
///
/// Returns `None` if the query does not match any known temporal pattern.
/// The composite scorer that consumes [`ExtractedCue`] ships in Plan B.
pub fn extract_cue(query: &str, now: DateTime<Utc>) -> Option<ExtractedCue> {
    let today = now.date_naive();
    if RE_YESTERDAY.is_match(query) {
        return Some(ExtractedCue {
            range: full_day_range(today - Duration::days(1)),
            confidence: CueConfidence::High,
        });
    }
    if RE_TODAY.is_match(query) {
        return Some(ExtractedCue {
            range: full_day_range(today),
            confidence: CueConfidence::High,
        });
    }
    if RE_TOMORROW.is_match(query) {
        return Some(ExtractedCue {
            range: full_day_range(today + Duration::days(1)),
            confidence: CueConfidence::High,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_returns_none() {
        assert_eq!(extract_cue("", Utc::now()), None);
    }

    #[test]
    fn yesterday_extracts_full_utc_day_high_confidence() {
        // now = 2026-05-26T15:00:00Z  →  yesterday = 2026-05-25
        let now = "2026-05-26T15:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let cue = extract_cue("what did I do yesterday?", now).expect("should extract");
        assert_eq!(cue.confidence, CueConfidence::High);
        // 2026-05-25 00:00:00 UTC
        assert_eq!(cue.range.start, 1_779_667_200);
        // 2026-05-25 23:59:59 UTC
        assert_eq!(cue.range.end, 1_779_753_599);
    }

    #[test]
    fn today_extracts_current_day_high_confidence() {
        // now = 2026-05-26T15:00:00Z  →  today = 2026-05-26
        let now = "2026-05-26T15:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let cue = extract_cue("notes today", now).expect("should extract");
        assert_eq!(cue.confidence, CueConfidence::High);
        // 2026-05-26 00:00:00 UTC
        assert_eq!(cue.range.start, 1_779_753_600);
        // 2026-05-26 23:59:59 UTC
        assert_eq!(cue.range.end, 1_779_839_999);
    }

    #[test]
    fn tomorrow_extracts_next_day_high_confidence() {
        // now = 2026-05-26T15:00:00Z  →  tomorrow = 2026-05-27
        let now = "2026-05-26T15:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let cue = extract_cue("meeting tomorrow", now).expect("should extract");
        assert_eq!(cue.confidence, CueConfidence::High);
        // 2026-05-27 00:00:00 UTC
        assert_eq!(cue.range.start, 1_779_840_000);
        // 2026-05-27 23:59:59 UTC
        assert_eq!(cue.range.end, 1_779_926_399);
    }

    #[test]
    fn case_insensitive_match() {
        let now = "2026-05-26T15:00:00Z".parse::<DateTime<Utc>>().unwrap();
        assert!(extract_cue("YESTERDAY I saw it", now).is_some());
        assert!(extract_cue("TODAY is the day", now).is_some());
        assert!(extract_cue("TOMORROW morning", now).is_some());
    }

    #[test]
    fn no_partial_word_match() {
        let now = "2026-05-26T15:00:00Z".parse::<DateTime<Utc>>().unwrap();
        // "everyday" must not fire on "today" pattern
        assert_eq!(extract_cue("everyday habit", now), None);
    }
}
