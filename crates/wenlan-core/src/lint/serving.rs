use super::catalog::LintCheckGroup;
use super::context::LintContext;
use super::memories::MemoryFeatureConfig;
use wenlan_types::lint::LintCheckResult;

mod query;
mod result;

pub(crate) const ROUTE_SCOPE_ID: &str = "serving.route_scope_contracts";
pub(crate) const CHANNEL_EPISODE_ID: &str = "serving.channel.episode";
pub(crate) const CHANNEL_FACT_ID: &str = "serving.channel.fact";
pub(crate) const CHANNEL_GRAPH_ID: &str = "serving.channel.graph";
pub(crate) const CHANNEL_PAGE_ID: &str = "serving.channel.page";
pub(crate) const CHANNEL_SUMMARY_ID: &str = "serving.channel.summary";
pub(crate) const FACT_STARVATION_ID: &str = "serving.fact_scope_starvation";
pub(crate) const OBSERVABILITY_ID: &str = "serving.observability_inventory";
pub(crate) const RERANKER_ID: &str = "serving.reranker_fallback_inventory";
pub(crate) const KNOWN_SCOPE_BYPASSES: &[&str] = &[
    "entity_search",
    "entity_detail",
    "page_detail",
    "page_sources",
    "page_links",
    "page_revisions",
];

#[derive(Debug, Clone, Copy)]
pub(crate) struct ServingRunConfig {
    page: bool,
    episode: bool,
    fact: bool,
    summary: bool,
    graph: bool,
    pub(crate) reranker: bool,
}

impl ServingRunConfig {
    pub(crate) const fn new(memory: MemoryFeatureConfig, graph: bool, reranker: bool) -> Self {
        Self {
            page: memory.page,
            episode: memory.episode,
            fact: memory.fact,
            summary: memory.summary,
            graph,
            reranker,
        }
    }

    #[cfg(test)]
    pub(crate) const fn all_disabled() -> Self {
        Self {
            page: false,
            episode: false,
            fact: false,
            summary: false,
            graph: false,
            reranker: false,
        }
    }

    #[cfg(test)]
    pub(crate) const fn all_enabled() -> Self {
        Self {
            page: true,
            episode: true,
            fact: true,
            summary: true,
            graph: true,
            reranker: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChannelAssessment {
    ExpectedEmpty { eligible: u64 },
    Finding { eligible: u64 },
    Live { eligible: u64, observed: u64 },
}

pub(crate) const fn assess_channel(
    _id: &'static str,
    enabled: bool,
    eligible: u64,
    observed: u64,
) -> ChannelAssessment {
    if !enabled {
        ChannelAssessment::ExpectedEmpty { eligible }
    } else if eligible > 0 && observed == 0 {
        ChannelAssessment::Finding { eligible }
    } else {
        ChannelAssessment::Live { eligible, observed }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FactCandidate<'a> {
    scope: &'a str,
    fact: bool,
}

impl<'a> FactCandidate<'a> {
    pub(crate) const fn new(scope: &'a str, fact: bool) -> Self {
        Self { scope, fact }
    }
}

pub(crate) fn fact_scope_starved(
    candidates: &[FactCandidate<'_>],
    scope: &str,
    top_k: usize,
) -> bool {
    candidates
        .iter()
        .any(|candidate| candidate.fact && candidate.scope == scope)
        && !candidates
            .iter()
            .take(top_k)
            .any(|candidate| candidate.fact && candidate.scope == scope)
}

pub(crate) async fn run(
    context: &LintContext<'_, '_>,
    config: ServingRunConfig,
) -> Vec<LintCheckResult> {
    let counts = match query::load(context).await {
        Ok(counts) => counts,
        Err(()) => {
            return super::runner::failed_results_for_group(
                context.clock(),
                LintCheckGroup::Serving,
            )
        }
    };
    let bypasses = u64::try_from(KNOWN_SCOPE_BYPASSES.len()).unwrap_or(u64::MAX);
    let mut results = vec![result::fixed(
        context,
        ROUTE_SCOPE_ID,
        bypasses,
        true,
        false,
    )];
    for (id, enabled, observed) in [
        (CHANNEL_EPISODE_ID, config.episode, counts.episode),
        (CHANNEL_FACT_ID, config.fact, counts.fact),
        (CHANNEL_GRAPH_ID, config.graph, counts.graph),
        (CHANNEL_PAGE_ID, config.page, counts.page),
        (CHANNEL_SUMMARY_ID, config.summary, counts.summary),
    ] {
        results.push(result::channel(
            context,
            id,
            assess_channel(id, enabled, counts.eligible, observed),
        ));
    }
    let fact_probe = [
        FactCandidate::new("other", true),
        FactCandidate::new("selected", true),
    ];
    results.push(result::fixed(
        context,
        FACT_STARVATION_ID,
        counts.eligible,
        config.fact && counts.eligible > 0 && fact_scope_starved(&fact_probe, "selected", 1),
        false,
    ));
    results.push(result::fixed(context, OBSERVABILITY_ID, 2, false, true));
    results.push(result::fixed(
        context,
        RERANKER_ID,
        u64::from(config.reranker),
        false,
        true,
    ));
    results
}

#[cfg(test)]
pub(crate) fn channel_result(
    clock: &super::context::LintClock,
    id: &'static str,
    assessment: ChannelAssessment,
) -> LintCheckResult {
    result::channel_for_test(clock, id, assessment)
}

#[cfg(test)]
#[path = "serving_test.rs"]
mod tests;
