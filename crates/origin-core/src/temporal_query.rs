// SPDX-License-Identifier: Apache-2.0
//! Temporal cue extraction from natural-language queries.
//!
//! Plan B's L1 retrieval uses [`extract_cue`] to drive the temporal channel.
//! This module ships the yesterday/today/tomorrow patterns; Tasks 7-8 add
//! week/month/year, weekday, N-ago, quarter, and since patterns.
//
// The module is pub(crate) for Plan A. Plan B promotes it and wires extract_cue
// into the composite scorer; until then the query-side helpers are stubs.
#![allow(dead_code)]

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc, Weekday};
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
static RE_LAST_WEEKDAY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\blast\s+(monday|tuesday|wednesday|thursday|friday|saturday|sunday)\b")
        .unwrap()
});
static RE_N_AGO: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(\d+)\s+(day|week|month)s?\s+ago\b").unwrap());
static RE_QUARTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(?:in|during|throughout)\s+(Q[1-4])\s*(\d{4})\b").unwrap());
static RE_SINCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(?:since|after)\s+(\d{4})\b").unwrap());
static RE_BEFORE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bbefore\s+(\d{4})\b").unwrap());

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

fn weekday_from_str(s: &str) -> Option<Weekday> {
    match s.to_ascii_lowercase().as_str() {
        "monday" => Some(Weekday::Mon),
        "tuesday" => Some(Weekday::Tue),
        "wednesday" => Some(Weekday::Wed),
        "thursday" => Some(Weekday::Thu),
        "friday" => Some(Weekday::Fri),
        "saturday" => Some(Weekday::Sat),
        "sunday" => Some(Weekday::Sun),
        _ => None,
    }
}

fn last_weekday_range(today: NaiveDate, target: Weekday) -> DateRange {
    let today_wd = today.weekday().num_days_from_monday();
    let target_wd = target.num_days_from_monday();
    // Days to go back; if same weekday go back 7
    let days_back = if today_wd > target_wd {
        (today_wd - target_wd) as i64
    } else {
        (7 - (target_wd - today_wd)) as i64
    };
    let target_date = today - Duration::days(days_back);
    full_day_range(target_date)
}

fn quarter_range(q: u32, year: i32) -> DateRange {
    let (start_month, end_month) = match q {
        1 => (1u32, 3u32),
        2 => (4u32, 6u32),
        3 => (7u32, 9u32),
        4 => (10u32, 12u32),
        _ => unreachable!(),
    };
    let first = NaiveDate::from_ymd_opt(year, start_month, 1).unwrap();
    // Last day of end_month
    let next_first = if end_month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(year, end_month + 1, 1).unwrap()
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
    if let Some(caps) = RE_LAST_WEEKDAY.captures(query) {
        let weekday_str = caps.get(1).unwrap().as_str();
        if let Some(target) = weekday_from_str(weekday_str) {
            return Some(ExtractedCue {
                range: last_weekday_range(today, target),
                confidence: CueConfidence::High,
            });
        }
    }
    if let Some(caps) = RE_N_AGO.captures(query) {
        let n: i64 = caps.get(1).unwrap().as_str().parse().unwrap_or(1);
        let unit = caps.get(2).unwrap().as_str().to_ascii_lowercase();
        let days = match unit.as_str() {
            "day" => n,
            "week" => n * 7,
            "month" => n * 30,
            _ => n,
        };
        let point = now - Duration::days(days);
        let start = (point - Duration::days(1)).timestamp();
        let end = (point + Duration::days(1)).timestamp();
        return Some(ExtractedCue {
            range: DateRange { start, end },
            confidence: CueConfidence::High,
        });
    }
    if let Some(caps) = RE_QUARTER.captures(query) {
        let q_str = caps.get(1).unwrap().as_str();
        let year: i32 = caps.get(2).unwrap().as_str().parse().unwrap_or(2000);
        let q: u32 = q_str[1..].parse().unwrap_or(1);
        return Some(ExtractedCue {
            range: quarter_range(q, year),
            confidence: CueConfidence::High,
        });
    }
    if let Some(caps) = RE_SINCE.captures(query) {
        let year: i32 = caps.get(1).unwrap().as_str().parse().unwrap_or(2000);
        let first = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
        let start = Utc
            .from_utc_datetime(&first.and_hms_opt(0, 0, 0).unwrap())
            .timestamp();
        return Some(ExtractedCue {
            range: DateRange {
                start,
                end: now.timestamp(),
            },
            confidence: CueConfidence::High,
        });
    }
    if let Some(caps) = RE_BEFORE.captures(query) {
        let year: i32 = caps.get(1).unwrap().as_str().parse().unwrap_or(2000);
        let last = NaiveDate::from_ymd_opt(year - 1, 12, 31).unwrap();
        let end = Utc
            .from_utc_datetime(&last.and_hms_opt(23, 59, 59).unwrap())
            .timestamp();
        return Some(ExtractedCue {
            range: DateRange { start: 0, end },
            confidence: CueConfidence::High,
        });
    }
    None
}

/// Extract a temporal cue from memory *content* for ingest backfill.
///
/// Unlike [`extract_cue`] (designed for search queries), this function only
/// matches calendar-explicit patterns that carry an unambiguous date independent
/// of "now". Anaphoric patterns such as "since YYYY", "before YYYY", "N weeks ago",
/// "last week/month", and weekday references are all relative to the time of query
/// and produce misleading ranges when applied to stored content whose reference
/// clock is the memory's `last_modified` timestamp.
///
/// Currently only matches explicit calendar quarters (`in Q2 2024`). Yesterday /
/// today / tomorrow are also included because they record when an event occurred
/// relative to the moment the memory was written (i.e. they anchor to
/// `last_modified`, which is what Pass A passes as `now`).
pub(crate) fn extract_cue_for_content(content: &str, now: DateTime<Utc>) -> Option<ExtractedCue> {
    let today = now.date_naive();
    // Relative-to-now day patterns are safe when `now` = memory's last_modified.
    if RE_YESTERDAY.is_match(content) {
        return Some(ExtractedCue {
            range: full_day_range(today - Duration::days(1)),
            confidence: CueConfidence::High,
        });
    }
    if RE_TODAY.is_match(content) {
        return Some(ExtractedCue {
            range: full_day_range(today),
            confidence: CueConfidence::High,
        });
    }
    if RE_TOMORROW.is_match(content) {
        return Some(ExtractedCue {
            range: full_day_range(today + Duration::days(1)),
            confidence: CueConfidence::High,
        });
    }
    // Explicit calendar quarter ("in Q2 2024") — the only pattern that anchors to
    // a calendar period rather than "now".
    if let Some(caps) = RE_QUARTER.captures(content) {
        let q_str = caps.get(1).unwrap().as_str();
        let year: i32 = caps.get(2).unwrap().as_str().parse().unwrap_or(2000);
        let q: u32 = q_str[1..].parse().unwrap_or(1);
        return Some(ExtractedCue {
            range: quarter_range(q, year),
            confidence: CueConfidence::High,
        });
    }
    // All other patterns (since YYYY, before YYYY, N ago, last week/month/year,
    // last <weekday>) are anaphoric — their meaning depends on when the query is
    // asked, not when the event occurred. Skipping them prevents a 10-year window
    // from "I have used Rust since 2015" polluting the event_date column.
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

    #[test]
    fn last_tuesday_extracts_most_recent_past_tuesday() {
        let now = "2026-05-27T12:00:00Z".parse::<DateTime<Utc>>().unwrap(); // Wed
        let cue = extract_cue("notes from last tuesday", now).expect("extract");
        assert_eq!(cue.confidence, CueConfidence::High);
        // 2026-05-26 = Tuesday
        let expected_start = "2026-05-26T00:00:00Z"
            .parse::<DateTime<Utc>>()
            .unwrap()
            .timestamp();
        assert_eq!(cue.range.start, expected_start);
    }

    #[test]
    fn three_weeks_ago_is_point_with_plus_minus_one_day() {
        let now = "2026-05-27T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let cue = extract_cue("3 weeks ago", now).expect("extract");
        // ~2026-05-06; +/- 1 day window
        assert!(cue.range.end - cue.range.start >= 2 * 86400);
    }

    #[test]
    fn quarter_pattern_q2_2024() {
        let now = Utc::now();
        let cue = extract_cue("in Q2 2024", now).expect("extract");
        let q2_start = "2024-04-01T00:00:00Z"
            .parse::<DateTime<Utc>>()
            .unwrap()
            .timestamp();
        let q2_end = "2024-06-30T23:59:59Z"
            .parse::<DateTime<Utc>>()
            .unwrap()
            .timestamp();
        assert_eq!(cue.range.start, q2_start);
        assert_eq!(cue.range.end, q2_end);
    }

    #[test]
    fn since_year_is_halfbound_capped_at_now() {
        let now = "2026-05-27T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let cue = extract_cue("notes since 2024", now).expect("extract");
        let y2024 = "2024-01-01T00:00:00Z"
            .parse::<DateTime<Utc>>()
            .unwrap()
            .timestamp();
        assert_eq!(cue.range.start, y2024);
        assert_eq!(cue.range.end, now.timestamp());
    }

    #[test]
    fn malformed_range_does_not_panic() {
        let now = Utc::now();
        // Implementation may return None OR a valid range; must not panic.
        let _ = extract_cue("from March to January", now);
    }
}
