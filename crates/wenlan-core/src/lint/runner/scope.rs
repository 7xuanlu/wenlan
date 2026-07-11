use super::LintRunError;
use crate::lint::context::AppliedScope;
use crate::lint::observation::{LintRunEvent, LintRunObserver};
use crate::lint::snapshot::{LintReadSnapshot, SnapshotError};
use wenlan_types::lint::LintQuery;

pub(super) async fn validate(
    snapshot: &LintReadSnapshot<'_>,
    query: &LintQuery,
    observer: &dyn LintRunObserver,
) -> Result<AppliedScope, LintRunError> {
    observer.observe(LintRunEvent::ScopeValidation);
    let Some(requested) = query.space.as_deref() else {
        return Ok(AppliedScope::global());
    };
    if requested == "uncategorized" {
        return Ok(AppliedScope::uncategorized());
    }
    let mut rows = snapshot
        .query(
            "SELECT (SELECT COUNT(*) FROM spaces prior WHERE prior.name < current.name) FROM spaces current WHERE current.name = ?1 LIMIT 1",
            libsql::params::Params::Positional(vec![libsql::Value::Text(requested.to_string())]),
        )
        .await?;
    let Some(row) = rows.next().await? else {
        return Err(LintRunError::InvalidScope);
    };
    let ordinal = row.get::<i64>(0).map_err(SnapshotError::from)?;
    let position = usize::try_from(ordinal).map_err(|_| LintRunError::InvalidScope)?;
    let opaque = wenlan_types::lint::LintOpaqueId::from_sorted_position(position)
        .ok_or(LintRunError::InvalidScope)?;
    Ok(AppliedScope::registered(opaque, requested.to_string()))
}
