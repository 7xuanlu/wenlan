/// Returns a trust score in [0, 1] combining confirmation status and stability tier.
///
/// stability tier: "new" (0.3) | "learned" (0.7) | "confirmed" (1.0) | other (0.5).
/// confirmed flag halves the tier when false.
// Plan B wires these signals into the composite scorer. Until then the module
// is pub(crate) only and the functions are unused outside their own tests.
#[allow(dead_code)]
pub fn trust(confirmed: bool, stability: &str) -> f64 {
    let s = match stability {
        "confirmed" => 1.0,
        "learned" => 0.7,
        "new" => 0.3,
        _ => 0.5,
    };
    if confirmed {
        s
    } else {
        s * 0.5
    }
}

/// Exponential decay over time. Returns value in (0, 1].
///
/// dt_days >= 0; the function clamps negatives to 0.
#[allow(dead_code)]
pub fn recency_decay(last_modified: i64, now: i64, tau_days: f64) -> f64 {
    let dt_days = (now - last_modified).max(0) as f64 / 86400.0;
    (-dt_days / tau_days).exp()
}

/// Log-normalized access frequency.
///
/// Returns ln(count + 1). Bounded growth: count=0 → 0, count=10 → ~2.4, count=100 → ~4.6.
#[allow(dead_code)]
pub fn access_frequency(access_count: u64) -> f64 {
    ((access_count as f64) + 1.0).ln()
}

/// Gaussian temporal proximity score in [0, 1].
///
/// Returns 0.0 when event_date is None. When Some, returns
/// exp(-(dt_days^2) / (2 * sigma_days^2)) where dt_days is the absolute difference
/// between query_date and event_date in days.
#[allow(dead_code)]
pub fn temporal_proximity(query_date: i64, event_date: Option<i64>, sigma_days: f64) -> f64 {
    match event_date {
        None => 0.0,
        Some(t) => {
            let dt_days = (query_date - t).unsigned_abs() as f64 / 86400.0;
            (-(dt_days * dt_days) / (2.0 * sigma_days * sigma_days)).exp()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_confirmed_outranks_unconfirmed() {
        assert!(trust(true, "confirmed") > trust(false, "confirmed"));
        assert!(trust(true, "confirmed") > trust(true, "learned"));
        assert!(trust(true, "learned") > trust(true, "new"));
    }

    #[test]
    fn trust_unknown_stability_returns_default() {
        let v = trust(true, "wat");
        assert!((v - 0.5).abs() < 1e-9);
    }

    #[test]
    fn recency_decay_monotone() {
        let now: i64 = 1_000_000;
        let r_recent = recency_decay(now - 86_400, now, 30.0);
        let r_old = recency_decay(now - 86_400 * 60, now, 30.0);
        assert!(r_recent > r_old);
    }

    #[test]
    fn recency_decay_now_returns_one() {
        let v = recency_decay(1_000_000, 1_000_000, 30.0);
        assert!((v - 1.0).abs() < 1e-9);
    }

    #[test]
    fn recency_decay_negative_dt_clamps() {
        // last_modified in the future should be treated as dt=0 (decay = 1.0).
        let v = recency_decay(2_000_000, 1_000_000, 30.0);
        assert!((v - 1.0).abs() < 1e-9);
    }

    #[test]
    fn access_frequency_log_normalized() {
        // count=0 → ln(1) = 0
        assert!((access_frequency(0) - 0.0).abs() < 1e-9);
        // monotone
        assert!(access_frequency(10) > access_frequency(1));
    }

    #[test]
    fn temporal_proximity_none_event_date_returns_zero() {
        assert_eq!(temporal_proximity(1_000_000, None, 30.0), 0.0);
    }

    #[test]
    fn temporal_proximity_same_day_near_one() {
        let v = temporal_proximity(1_000_000, Some(1_000_000), 30.0);
        assert!(v > 0.99);
    }

    #[test]
    fn temporal_proximity_far_decays() {
        let v_close = temporal_proximity(1_000_000, Some(1_000_000 + 86_400), 30.0);
        let v_far = temporal_proximity(1_000_000, Some(1_000_000 + 86_400 * 90), 30.0);
        assert!(v_close > v_far);
    }
}
