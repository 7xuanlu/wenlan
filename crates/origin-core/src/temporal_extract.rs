// SPDX-License-Identifier: Apache-2.0
//! Regex-based date extraction for the temporal channel.
//!
//! Extracts an `event_date` from raw memory text and returns it as a UTC
//! unix-epoch seconds value. Returns `None` when no pattern matches or when
//! the candidate is ambiguous (e.g. a bare `M/D` with no year). Conservative
//! by design: prefer a missed extraction over a wrong one.
//!
//! When multiple dates appear in the same text, the first successfully parsed
//! match wins. Patterns are tried in this order:
//!
//! 1. ISO 8601 calendar dates (`YYYY-MM-DD`)
//! 2. Written-month forms (`January 5, 2024`, `Jan 5 2024`, `5 January 2024`)
//! 3. US numeric dates (`M/D/YYYY`)
//! 4. Relative phrases (`today`, `yesterday`, `tomorrow`, `N days/weeks/months
//!    ago`, `last week`, `last Monday`)
//!
//! LLM augmentation for ambiguous or natural-language cases is a planned
//! follow-up. This module ships only the regex first-pass.
//!
//! All resolved timestamps are normalized to UTC midnight (00:00:00) for the
//! matched calendar day so the value is a stable day anchor regardless of the
//! caller's timezone.

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc, Weekday};
use regex::Regex;
use std::sync::LazyLock;

static ISO_DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(\d{4})-(\d{2})-(\d{2})\b").unwrap());

static US_DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(\d{1,2})/(\d{1,2})/(\d{4})\b").unwrap());

// "January 5, 2024" / "Jan 5 2024" / "January 5 2024"
static WRITTEN_MONTH_FIRST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|jun(?:e)?|jul(?:y)?|aug(?:ust)?|sep(?:tember)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)\s+(\d{1,2})(?:,)?\s+(\d{4})\b",
    )
    .unwrap()
});

// "5 January 2024" / "5 Jan 2024"
static WRITTEN_DAY_FIRST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(\d{1,2})\s+(jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|jun(?:e)?|jul(?:y)?|aug(?:ust)?|sep(?:tember)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)\s+(\d{4})\b",
    )
    .unwrap()
});

static RELATIVE_TODAY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\btoday\b").unwrap());
static RELATIVE_YESTERDAY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\byesterday\b").unwrap());
static RELATIVE_TOMORROW: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\btomorrow\b").unwrap());

// "3 days ago", "2 weeks ago", "1 month ago"
static RELATIVE_N_AGO: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(\d{1,3})\s+(day|days|week|weeks|month|months)\s+ago\b").unwrap()
});

static RELATIVE_LAST_WEEK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\blast\s+week\b").unwrap());

// "last Monday", "last Friday", etc.
static RELATIVE_LAST_WEEKDAY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\blast\s+(mon|tue|wed|thu|fri|sat|sun)(?:day|sday|nesday|rsday|urday)?\b")
        .unwrap()
});

/// Extract a single event date from `text`, returning UTC unix-epoch seconds.
///
/// `reference_time` is the "now" anchor for resolving relative phrases such as
/// `yesterday` or `3 days ago`. It is also unix-epoch seconds UTC. Returns
/// `None` when no supported pattern matches or the match is ambiguous.
pub fn extract_event_date(text: &str, reference_time: i64) -> Option<i64> {
    if let Some(caps) = ISO_DATE.captures(text) {
        let y: i32 = caps.get(1)?.as_str().parse().ok()?;
        let m: u32 = caps.get(2)?.as_str().parse().ok()?;
        let d: u32 = caps.get(3)?.as_str().parse().ok()?;
        if let Some(ts) = ymd_to_utc_midnight(y, m, d) {
            return Some(ts);
        }
    }

    if let Some(caps) = WRITTEN_MONTH_FIRST.captures(text) {
        let m = month_from_name(caps.get(1)?.as_str())?;
        let d: u32 = caps.get(2)?.as_str().parse().ok()?;
        let y: i32 = caps.get(3)?.as_str().parse().ok()?;
        if let Some(ts) = ymd_to_utc_midnight(y, m, d) {
            return Some(ts);
        }
    }

    if let Some(caps) = WRITTEN_DAY_FIRST.captures(text) {
        let d: u32 = caps.get(1)?.as_str().parse().ok()?;
        let m = month_from_name(caps.get(2)?.as_str())?;
        let y: i32 = caps.get(3)?.as_str().parse().ok()?;
        if let Some(ts) = ymd_to_utc_midnight(y, m, d) {
            return Some(ts);
        }
    }

    if let Some(caps) = US_DATE.captures(text) {
        let m: u32 = caps.get(1)?.as_str().parse().ok()?;
        let d: u32 = caps.get(2)?.as_str().parse().ok()?;
        let y: i32 = caps.get(3)?.as_str().parse().ok()?;
        if let Some(ts) = ymd_to_utc_midnight(y, m, d) {
            return Some(ts);
        }
    }

    let ref_dt = DateTime::<Utc>::from_timestamp(reference_time, 0)?;
    let ref_day = ref_dt.date_naive();

    if RELATIVE_YESTERDAY.is_match(text) {
        return naive_date_to_utc_midnight(ref_day - Duration::days(1));
    }
    if RELATIVE_TOMORROW.is_match(text) {
        return naive_date_to_utc_midnight(ref_day + Duration::days(1));
    }
    if RELATIVE_TODAY.is_match(text) {
        return naive_date_to_utc_midnight(ref_day);
    }

    if let Some(caps) = RELATIVE_N_AGO.captures(text) {
        let n: i64 = caps.get(1)?.as_str().parse().ok()?;
        let unit = caps.get(2)?.as_str().to_ascii_lowercase();
        let days = match unit.as_str() {
            "day" | "days" => n,
            "week" | "weeks" => n * 7,
            "month" | "months" => n * 30, // approximate; calendar months are uneven
            _ => return None,
        };
        return naive_date_to_utc_midnight(ref_day - Duration::days(days));
    }

    if RELATIVE_LAST_WEEK.is_match(text) {
        return naive_date_to_utc_midnight(ref_day - Duration::days(7));
    }

    if let Some(caps) = RELATIVE_LAST_WEEKDAY.captures(text) {
        let target = weekday_from_prefix(caps.get(1)?.as_str())?;
        let current = ref_day.weekday();
        // "last Monday" relative to today: walk back at least 1 day, at most 7.
        let cur_num = current.num_days_from_monday() as i64;
        let tgt_num = target.num_days_from_monday() as i64;
        let mut diff = cur_num - tgt_num;
        if diff <= 0 {
            diff += 7;
        }
        return naive_date_to_utc_midnight(ref_day - Duration::days(diff));
    }

    None
}

fn ymd_to_utc_midnight(year: i32, month: u32, day: u32) -> Option<i64> {
    let nd = NaiveDate::from_ymd_opt(year, month, day)?;
    naive_date_to_utc_midnight(nd)
}

fn naive_date_to_utc_midnight(nd: NaiveDate) -> Option<i64> {
    let ndt = nd.and_hms_opt(0, 0, 0)?;
    Some(Utc.from_utc_datetime(&ndt).timestamp())
}

fn month_from_name(name: &str) -> Option<u32> {
    let lower = name.to_ascii_lowercase();
    let m = match lower.as_str() {
        "jan" | "january" => 1,
        "feb" | "february" => 2,
        "mar" | "march" => 3,
        "apr" | "april" => 4,
        "may" => 5,
        "jun" | "june" => 6,
        "jul" | "july" => 7,
        "aug" | "august" => 8,
        "sep" | "september" => 9,
        "oct" | "october" => 10,
        "nov" | "november" => 11,
        "dec" | "december" => 12,
        _ => return None,
    };
    Some(m)
}

fn weekday_from_prefix(prefix: &str) -> Option<Weekday> {
    let lower = prefix.to_ascii_lowercase();
    let wd = match lower.as_str() {
        "mon" => Weekday::Mon,
        "tue" => Weekday::Tue,
        "wed" => Weekday::Wed,
        "thu" => Weekday::Thu,
        "fri" => Weekday::Fri,
        "sat" => Weekday::Sat,
        "sun" => Weekday::Sun,
        _ => return None,
    };
    Some(wd)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-05-24 00:00:00 UTC — a Sunday.
    const REF_2026_05_24: i64 = 1_779_580_800;

    fn ts(y: i32, m: u32, d: u32) -> i64 {
        ymd_to_utc_midnight(y, m, d).unwrap()
    }

    #[test]
    fn iso_8601_extracts() {
        assert_eq!(
            extract_event_date("we met on 2024-03-15 for coffee", REF_2026_05_24),
            Some(ts(2024, 3, 15)),
        );
        assert_eq!(
            extract_event_date("ship date 2024-12-31.", REF_2026_05_24),
            Some(ts(2024, 12, 31)),
        );
    }

    #[test]
    fn us_date_extracts() {
        assert_eq!(
            extract_event_date("scheduled 3/15/2024 with team", REF_2026_05_24),
            Some(ts(2024, 3, 15)),
        );
        assert_eq!(
            extract_event_date("deadline 12/31/2024.", REF_2026_05_24),
            Some(ts(2024, 12, 31)),
        );
    }

    #[test]
    fn written_month_first_extracts() {
        assert_eq!(
            extract_event_date("January 5, 2024 was the kickoff", REF_2026_05_24),
            Some(ts(2024, 1, 5)),
        );
        assert_eq!(
            extract_event_date("Jan 5 2024 onsite", REF_2026_05_24),
            Some(ts(2024, 1, 5)),
        );
    }

    #[test]
    fn written_day_first_extracts() {
        assert_eq!(
            extract_event_date("we met on 5 January 2024 in Berlin", REF_2026_05_24),
            Some(ts(2024, 1, 5)),
        );
    }

    #[test]
    fn yesterday_resolves_to_prior_day() {
        assert_eq!(
            extract_event_date("we agreed on this yesterday", REF_2026_05_24),
            Some(ts(2026, 5, 23)),
        );
    }

    #[test]
    fn today_resolves_to_reference_day() {
        assert_eq!(
            extract_event_date("today we ship", REF_2026_05_24),
            Some(ts(2026, 5, 24)),
        );
    }

    #[test]
    fn tomorrow_resolves_to_next_day() {
        assert_eq!(
            extract_event_date("tomorrow's standup", REF_2026_05_24),
            Some(ts(2026, 5, 25)),
        );
    }

    #[test]
    fn n_days_ago_resolves() {
        assert_eq!(
            extract_event_date("3 days ago we shipped", REF_2026_05_24),
            Some(ts(2026, 5, 21)),
        );
    }

    #[test]
    fn last_week_resolves_to_seven_days_back() {
        assert_eq!(
            extract_event_date("last week the build broke", REF_2026_05_24),
            Some(ts(2026, 5, 17)),
        );
    }

    #[test]
    fn last_weekday_walks_back_within_seven_days() {
        // 2026-05-24 is a Sunday; "last Monday" -> 2026-05-18.
        assert_eq!(
            extract_event_date("last Monday we agreed", REF_2026_05_24),
            Some(ts(2026, 5, 18)),
        );
    }

    #[test]
    fn no_match_returns_none() {
        assert_eq!(extract_event_date("hello world", REF_2026_05_24), None);
        assert_eq!(
            extract_event_date("the meeting was great", REF_2026_05_24),
            None,
        );
    }

    #[test]
    fn ambiguous_bare_md_returns_none() {
        // "on 5/6" has no year and could be M/D or D/M — refuse to guess.
        assert_eq!(extract_event_date("on 5/6", REF_2026_05_24), None);
    }

    #[test]
    fn first_match_wins_when_multiple_present() {
        // ISO 8601 has highest priority; both should be present but ISO returns.
        assert_eq!(
            extract_event_date(
                "originally 2024-03-15, rescheduled 4/20/2024",
                REF_2026_05_24,
            ),
            Some(ts(2024, 3, 15)),
        );
    }

    #[test]
    fn invalid_calendar_date_returns_none() {
        // Feb 30 does not exist.
        assert_eq!(extract_event_date("2024-02-30", REF_2026_05_24), None);
    }
}
