mod imports;
mod maintenance;
mod queue;
mod refinement;
mod reviews;
mod source;

use super::config::OperationsRunConfig;
use super::result::Assessment;
use crate::lint::operations::read_context::OperationsReadContext;
use wenlan_types::lint::{LintMetric, LintMetricCode, LintMetricValue};

pub(super) async fn load(
    context: &OperationsReadContext<'_, '_>,
    config: OperationsRunConfig,
) -> Result<Vec<Assessment>, ()> {
    Ok(vec![
        queue::load(context).await?,
        imports::load(context).await?,
        maintenance::load(context).await?,
        reviews::load_refinements(context).await?,
        reviews::load_rejections(context).await?,
        source::load(context, config).await?,
    ])
}

#[derive(Default)]
pub(super) struct AgeBuckets {
    under_hour: u64,
    one_to_24_hours: u64,
    one_to_seven_days: u64,
    seven_days_or_more: u64,
}

impl AgeBuckets {
    pub(super) fn observe(&mut self, timestamp: i64, observed_at: i64) -> bool {
        if timestamp > observed_at {
            return false;
        }
        match observed_at - timestamp {
            0..=3_599 => self.under_hour = self.under_hour.saturating_add(1),
            3_600..=86_399 => self.one_to_24_hours = self.one_to_24_hours.saturating_add(1),
            86_400..=604_799 => self.one_to_seven_days = self.one_to_seven_days.saturating_add(1),
            _ => self.seven_days_or_more = self.seven_days_or_more.saturating_add(1),
        }
        true
    }

    pub(super) fn metrics(self) -> Vec<LintMetric> {
        vec![
            metric(LintMetricCode::OperationAgeUnderHour, self.under_hour),
            metric(
                LintMetricCode::OperationAgeOneTo24Hours,
                self.one_to_24_hours,
            ),
            metric(
                LintMetricCode::OperationAgeOneTo7Days,
                self.one_to_seven_days,
            ),
            metric(
                LintMetricCode::OperationAgeSevenDaysOrMore,
                self.seven_days_or_more,
            ),
        ]
    }
}

pub(super) fn metric(code: LintMetricCode, value: u64) -> LintMetric {
    LintMetric::new(code, LintMetricValue::Count { value })
}
