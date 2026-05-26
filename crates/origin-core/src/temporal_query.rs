// SPDX-License-Identifier: Apache-2.0
//! Temporal cue extraction from natural-language queries.
//!
//! Plan B's L1 retrieval uses [`extract_cue`] to drive the temporal channel.
//! This module ships the yesterday/today/tomorrow patterns; Tasks 7-8 add
//! week/month/year, weekday, N-ago, quarter, and since patterns.

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
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
static RE_LAST_PERIOD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(this|last)\s+(week|month|year)\b").unwrap());

const WEEK_BOUNDARY_SECONDS: i64 = 12 * 3600;
const MONTH_BOUNDARY_SECONDS: i64 = 24 * 3600;
const YEAR_BOUNDARY_SECONDS: i64 = 72 * 3600;

fn full_day_range(d: NaiveDate) -> DateRange {
    let start = Utc
        .from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap())
        .timestamp();
    let end = Utc
        .from_utc_datetime(&d.and_hms_opt(23, 59, 59).unwrap())
        .timestamp();
    DateRange { start, end }
}

fn week_range_starting_monday(d: NaiveDate, this_or_last: &str) -> DateRange {
    let days_from_monday = d.weekday().num_days_from_monday() as i64;
    let this_monday = d - Duration::days(days_from_monday);
    let week_start = if this_or_last == "this" {
        this_monday
    } else {
        this_monday - Duration::days(7)
    };
    let week_end = week_start + Duration::days(6);
    DateRange {
        start: Utc
            .from_utc_datetime(&week_start.and_hms_opt(0, 0, 0).unwrap())
            .timestamp(),
        end: Utc
            .from_utc_datetime(&week_end.and_hms_opt(23, 59, 59).unwrap())
            .timestamp(),
    }
}

fn near_week_boundary(now: DateTime<Utc>) -> bool {
    let days_from_monday = now.weekday().num_days_from_monday() as i64;
    let this_monday = now.date_naive() - Duration::days(days_from_monday);
    let monday_ts = Utc
        .from_utc_datetime(&this_monday.and_hms_opt(0, 0, 0).unwrap())
        .timestamp();
    let next_monday_ts = monday_ts + 7 * 24 * 3600;
    let now_ts = now.timestamp();
    (now_ts - monday_ts).abs() < WEEK_BOUNDARY_SECONDS
        || (next_monday_ts - now_ts).abs() < WEEK_BOUNDARY_SECONDS
}

fn month_range(d: NaiveDate, this_or_last: &str) -> DateRange {
    let (year, month) = if this_or_last == "this" {
        (d.year(), d.month())
    } else if d.month() == 1 {
        (d.year() - 1, 12)
    } else {
        (d.year(), d.month() - 1)
    };
    let first = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let next_first = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1).unwrap()
    };
    let last_day = next_first - Duration::days(1);
    DateRange {
        start: Utc
            .from_utc_datetime(&first.and_hms_opt(0, 0, 0).unwrap())
            .timestamp(),
        end: Utc
            .from_utc_datetime(&last_day.and_hms_opt(23, 59, 59).unwrap())
            .timestamp(),
    }
}

fn near_month_boundary(now: DateTime<Utc>) -> bool {
    let d = now.date_naive();
    let first_of_month = NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap();
    let next_month_first = if d.month() == 12 {
        NaiveDate::from_ymd_opt(d.year() + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1).unwrap()
    };
    let now_ts = now.timestamp();
    let first_ts = Utc
        .from_utc_datetime(&first_of_month.and_hms_opt(0, 0, 0).unwrap())
        .timestamp();
    let next_ts = Utc
        .from_utc_datetime(&next_month_first.and_hms_opt(0, 0, 0).unwrap())
        .timestamp();
    (now_ts - first_ts).abs() < MONTH_BOUNDARY_SECONDS
        || (next_ts - now_ts).abs() < MONTH_BOUNDARY_SECONDS
}

fn year_range(d: NaiveDate, this_or_last: &str) -> DateRange {
    let year = if this_or_last == "this" {
        d.year()
    } else {
        d.year() - 1
    };
    let first = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
    let last = NaiveDate::from_ymd_opt(year, 12, 31).unwrap();
    DateRange {
        start: Utc
            .from_utc_datetime(&first.and_hms_opt(0, 0, 0).unwrap())
            .timestamp(),
        end: Utc
            .from_utc_datetime(&last.and_hms_opt(23, 59, 59).unwrap())
            .timestamp(),
    }
}

fn near_year_boundary(now: DateTime<Utc>) -> bool {
    let year = now.year();
    let first = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
    let next = NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap();
    let now_ts = now.timestamp();
    let first_ts = Utc
        .from_utc_datetime(&first.and_hms_opt(0, 0, 0).unwrap())
        .timestamp();
    let next_ts = Utc
        .from_utc_datetime(&next.and_hms_opt(0, 0, 0).unwrap())
        .timestamp();
    (now_ts - first_ts).abs() < YEAR_BOUNDARY_SECONDS
        || (next_ts - now_ts).abs() < YEAR_BOUNDARY_SECONDS
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
    if let Some(caps) = RE_LAST_PERIOD.captures(query) {
        let this_or_last = caps.get(1).unwrap().as_str().to_lowercase();
        let unit = caps.get(2).unwrap().as_str().to_lowercase();
        let (range, near_boundary) = match unit.as_str() {
            "week" => (
                week_range_starting_monday(today, &this_or_last),
                near_week_boundary(now),
            ),
            "month" => (month_range(today, &this_or_last), near_month_boundary(now)),
            "year" => (year_range(today, &this_or_last), near_year_boundary(now)),
            _ => unreachable!(),
        };
        let confidence = if near_boundary {
            CueConfidence::Low
        } else {
            CueConfidence::High
        };
        return Some(ExtractedCue { range, confidence });
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

    #[test]
    fn last_week_mid_week_is_high_confidence() {
        // Wednesday far from week boundary
        let now = "2026-05-27T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let cue = extract_cue("what did I work on last week", now).expect("should extract");
        assert_eq!(cue.confidence, CueConfidence::High);
    }

    #[test]
    fn last_week_at_week_boundary_is_low_confidence() {
        // Sunday 23:59 — within 12 hours of Monday 00:00 boundary
        let now = "2026-05-31T23:59:00Z".parse::<DateTime<Utc>>().unwrap();
        let cue = extract_cue("what did I do last week", now).expect("should extract");
        assert_eq!(cue.confidence, CueConfidence::Low);
    }

    #[test]
    fn last_month_mid_month_high() {
        let now = "2026-05-15T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let cue = extract_cue("notes from last month", now).expect("should extract");
        assert_eq!(cue.confidence, CueConfidence::High);
    }

    #[test]
    fn last_month_near_boundary_low() {
        // 12 hours after month start
        let now = "2026-05-01T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let cue = extract_cue("notes from last month", now).expect("should extract");
        assert_eq!(cue.confidence, CueConfidence::Low);
    }
}
