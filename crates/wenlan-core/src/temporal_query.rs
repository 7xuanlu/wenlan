// SPDX-License-Identifier: Apache-2.0
//! Temporal cue extraction from natural-language queries.
//!
//! Plan B's L1 retrieval uses [`extract_cue`] to drive the temporal channel.
//! This module ships the yesterday/today/tomorrow patterns; Tasks 7-8 add
//! week/month/year, weekday, N-ago, quarter, and since patterns.
//
// `extract_cue` is wired into `db::search_memory_temporal` (T4a).
// `extract_cue_for_content` is called from migration 55 backfill.

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

/// Rewrite relative date phrases in stored memory prose to include absolute dates.
///
/// This is the deterministic write-time prose grounder for T11. It is called at
/// store time (inside `upsert_documents`) when `WENLAN_ENABLE_TEMPORAL_GROUNDING`
/// is set to a truthy value, **before** the content is embedded or inserted into
/// the database, so the embedder and FTS index both see the grounded text.
///
/// ## Contract
///
/// - **APPEND, not replace**: `"met her yesterday"` -> `"met her yesterday (2026-04-30)"`.
///   The original phrasing is preserved, making idempotency trivial.
/// - **IDEMPOTENT**: if a phrase is already followed by `(YYYY-MM-DD)` it is
///   skipped. Running `ground_relative_dates` twice produces the same output.
/// - **Observation-anchored**: the reference clock is `observation_date`, which is
///   the memory's `last_modified` timestamp. For realtime ingest, `last_modified`
///   is set to `Utc::now()` at request time, so it ≈ the event time and grounding
///   is event-accurate. For delayed / backfill / import ingest, `last_modified` is
///   the IMPORT time, not the time the event was originally written, so "yesterday"
///   anchors to import time — grounding is only event-accurate for realtime
///   capture. A true event-clock would need a separate `event_timestamp` field on
///   `RawDocument` (follow-up).
/// - **Safe subset only (v1)**: only `yesterday`, `today`, and `tomorrow` are
///   grounded. Vague anaphors like "recently" or "last week" are NOT grounded —
///   they carry too much ambiguity to produce a correct absolute date without
///   additional context.
/// - **Possessive-safe**: a match immediately followed by an apostrophe
///   (`today's meeting`) is skipped — `\b` fires before the `'`, so grounding it
///   would corrupt the prose into `today (2026-05-01)'s meeting`. The Rust `regex`
///   crate has no lookahead, so this is handled in code by inspecting the char
///   after the match.
/// - **UTF-8 safe**: uses regex match offsets on validated UTF-8 text; no raw
///   byte-slicing outside of regex-returned boundaries.
///
/// ## Deferred (T11 follow-ups)
///
/// - Grounding `last week`, `last month`, N-ago, and explicit quarters.
/// - LLM-prompt `{observation_date}` injection (T11 steps 2-3).
/// - Server dead-module swap (T11 step 6).
/// - Eval seed-date plumbing (T11 step 7).
pub(crate) fn ground_relative_dates(content: &str, observation_date: DateTime<Utc>) -> String {
    let today = observation_date.date_naive();

    // RE_ALREADY_GROUNDED: detect whether the text immediately after a match
    // is already an absolute date annotation `(YYYY-MM-DD)` — the idempotency
    // guard. Defined as a module-level LazyLock so it compiles once.
    static RE_ALREADY_GROUNDED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\s*\(\d{4}-\d{2}-\d{2}\)").unwrap());

    // Collect (start, end, replacement) patches, applied in reverse byte order
    // so earlier offsets remain valid after each splice.
    let mut patches: Vec<(usize, usize, String)> = Vec::new();

    // Safe anchorable set (v1): yesterday / today / tomorrow only.
    // Vague phrases ("recently", "last week") deferred to v2.
    let candidates: &[(&LazyLock<Regex>, chrono::NaiveDate)] = &[
        (&RE_YESTERDAY, today - Duration::days(1)),
        (&RE_TODAY, today),
        (&RE_TOMORROW, today + Duration::days(1)),
    ];

    for (re, date) in candidates {
        for m in re.find_iter(content) {
            let after = &content[m.end()..];
            // Possessive-safe: \b fires before an apostrophe because ' is not
            // an ASCII word char, so "today's" matches "today". Grounding it would
            // corrupt prose into "today (2026-05-01)'s meeting". The Rust regex crate
            // has no lookahead, so skip the occurrence in code when the next char is
            // an apostrophe (straight or typographic).
            if matches!(after.chars().next(), Some('\'') | Some('\u{2019}')) {
                continue;
            }
            // Idempotency: skip if the match is already followed by (YYYY-MM-DD).
            if RE_ALREADY_GROUNDED.is_match(after) {
                continue;
            }
            // Append the absolute date in parentheses, preserving original text.
            let replacement = format!("{} ({})", m.as_str(), date.format("%Y-%m-%d"));
            patches.push((m.start(), m.end(), replacement));
        }
    }

    if patches.is_empty() {
        return content.to_owned();
    }

    // Sort descending by start offset so back-to-front splicing keeps earlier
    // offsets valid.
    patches.sort_by_key(|p| std::cmp::Reverse(p.0));

    let mut result = content.to_owned();
    for (start, end, replacement) in patches {
        result.replace_range(start..end, &replacement);
    }
    result
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

    // ── T11: ground_relative_dates unit tests ──────────────────────────────────

    #[test]
    fn ground_yesterday_appends_absolute() {
        let obs = "2026-05-01T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let result = ground_relative_dates("met her yesterday", obs);
        assert!(
            result.contains("yesterday (2026-04-30)"),
            "expected yesterday (2026-04-30) in {:?}",
            result
        );
    }

    #[test]
    fn ground_today_appends_absolute() {
        let obs = "2026-05-01T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let result = ground_relative_dates("shipped today", obs);
        assert!(
            result.contains("today (2026-05-01)"),
            "expected today (2026-05-01) in {:?}",
            result
        );
    }

    #[test]
    fn ground_tomorrow_appends_absolute() {
        let obs = "2026-05-01T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let result = ground_relative_dates("meeting tomorrow", obs);
        assert!(
            result.contains("tomorrow (2026-05-02)"),
            "expected tomorrow (2026-05-02) in {:?}",
            result
        );
    }

    /// Grounder uses observation_date not Utc::now() as the reference clock.
    #[test]
    fn ground_anchors_to_observation_not_now() {
        let obs = "2026-01-15T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let result = ground_relative_dates("I met him yesterday", obs);
        // Must anchor to 2026-01-14 (obs - 1 day), not the system date.
        assert!(
            result.contains("yesterday (2026-01-14)"),
            "expected yesterday (2026-01-14) anchored to obs date, got {:?}",
            result
        );
        let todays_date = Utc::now().date_naive().format("%Y-%m-%d").to_string();
        if todays_date != "2026-01-14" {
            // If system date differs from 2026-01-14, it must NOT appear.
            let wrong = format!("yesterday ({})", todays_date);
            assert!(
                !result.contains(&wrong),
                "grounder must NOT use Utc::now(); found today's date in {:?}",
                result
            );
        }
    }

    #[test]
    fn ground_idempotent() {
        let obs = "2026-05-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let inputs = [
            "met her yesterday",
            "delivered today",
            "meeting tomorrow",
            "no temporal phrase here",
        ];
        for input in &inputs {
            let once = ground_relative_dates(input, obs);
            let twice = ground_relative_dates(&once, obs);
            assert_eq!(
                once, twice,
                "ground_relative_dates must be idempotent for input {:?}",
                input
            );
        }
    }

    #[test]
    fn ground_no_relative_phrase_unchanged() {
        let obs = "2026-05-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let content = "the database password is hunter2";
        let result = ground_relative_dates(content, obs);
        assert_eq!(
            result, content,
            "non-temporal content must be byte-identical"
        );
    }

    #[test]
    fn ground_vague_phrase_not_grounded_v1() {
        let obs = "2026-05-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        for phrase in &["recently", "last week", "a while ago", "sometime ago"] {
            let result = ground_relative_dates(phrase, obs);
            assert_eq!(
                &result, phrase,
                "vague phrase {:?} must not be grounded in v1",
                phrase
            );
        }
    }

    /// Non-ASCII content (emoji, CJK) must not panic; the temporal phrase is still grounded.
    #[test]
    fn ground_utf8_no_panic() {
        let obs = "2026-05-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        // Multibyte chars around "yesterday" exercise UTF-8 boundary safety.
        let content = "\u{65E5}\u{672C}\u{8A9E}\u{30C6}\u{30B9}\u{30C8} shipped yesterday in \u{6771}\u{4EAC}";
        let result = ground_relative_dates(content, obs);
        assert!(
            result.contains("yesterday (2026-04-30)"),
            "expected yesterday grounded in multibyte content, got {:?}",
            result
        );
        assert!(result.contains('\u{6771}'), "CJK text must be preserved");
    }

    #[test]
    fn ground_empty_and_whitespace_unchanged() {
        let obs = "2026-05-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        assert_eq!(ground_relative_dates("", obs), "");
        assert_eq!(ground_relative_dates("   ", obs), "   ");
    }

    #[test]
    fn ground_multiple_phrases_in_one_string() {
        let obs = "2026-05-01T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let content = "I met her yesterday and the demo is tomorrow";
        let result = ground_relative_dates(content, obs);
        assert!(
            result.contains("yesterday (2026-04-30)"),
            "yesterday not grounded in {:?}",
            result
        );
        assert!(
            result.contains("tomorrow (2026-05-02)"),
            "tomorrow not grounded in {:?}",
            result
        );
    }

    /// Possessive forms ("today's", "yesterday's", "tomorrow's") must NOT be
    /// grounded — `\b` fires before the apostrophe, so grounding would corrupt
    /// the prose into "today (2026-05-01)'s meeting".
    #[test]
    fn ground_possessive_not_grounded() {
        let obs = "2026-05-01T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        for phrase in &[
            "today's meeting",
            "yesterday's standup",
            "tomorrow's deadline",
        ] {
            let result = ground_relative_dates(phrase, obs);
            assert_eq!(
                &result, phrase,
                "possessive {:?} must not be grounded",
                phrase
            );
        }
    }

    /// Typographic apostrophe (U+2019) possessive must also be skipped.
    #[test]
    fn ground_typographic_possessive_not_grounded() {
        let obs = "2026-05-01T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let phrase = "today\u{2019}s meeting";
        let result = ground_relative_dates(phrase, obs);
        assert_eq!(
            result, phrase,
            "typographic possessive must not be grounded"
        );
    }

    /// Non-possessive trailing chars (period, comma, space, end-of-string) MUST
    /// still ground — the possessive skip must not over-match.
    #[test]
    fn ground_non_possessive_trailing_still_grounds() {
        let obs = "2026-05-01T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        // trailing period
        assert!(
            ground_relative_dates("shipped today.", obs).contains("today (2026-05-01)."),
            "trailing period must still ground"
        );
        // trailing comma
        assert!(
            ground_relative_dates("today, we shipped", obs).contains("today (2026-05-01),"),
            "trailing comma must still ground"
        );
        // trailing space
        assert!(
            ground_relative_dates("today we shipped", obs).contains("today (2026-05-01) we"),
            "trailing space must still ground"
        );
        // end of string
        assert!(
            ground_relative_dates("we shipped today", obs).contains("today (2026-05-01)"),
            "end-of-string must still ground"
        );
    }
}
