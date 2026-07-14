# Evidence-Driven Cleanup Proposal

## Decision

Do not mutate the live store from this campaign. The accepted canonical branch
reports identify review work, but the external aggregate probe is incomplete
because its `pages` count oracle disagreed. The lint report exposes run-local
opaque ordinals rather than durable owner IDs and expected versions, so none of
the current rows qualifies as an executable CAS repair.

The raw artifacts remain outside git at the manifest referenced by the evidence
ledger. This proposal contains counts and reason codes only.

## Deterministic Safe

None approved.

A deterministic repair entry must contain a durable owner identity, expected
version or content receipt, exact canonical writer, rollback artifact, and a
post-repair lint assertion. Current reports do not satisfy that target contract.
In particular, 73 orphan labels are safe to leave unresolved but are not safe
to bind automatically, and four Pages without provenance cannot have evidence
fabricated from title similarity.

## Needs Semantic Review

| Finding family | Observed candidates | Required decision |
|---|---:|---|
| Memory state and supersession integrity | 11 in each overlapping check | Decide the intended current state and temporal relation per durable memory owner; do not infer suppression from shape. |
| Duplicate active Page titles | 3 | Select merge, rename, archive, or intentional coexistence within scope; preserve Page versions and evidence. |
| Page orphan labels | 73 | Resolve only when one same-scope target is unambiguous; otherwise leave orphaned. |
| Page provenance adequacy | 4 | Recover a real `memory`, `external_file`, `external_url`, or `authored` source from durable history; never synthesize evidence. |
| Duplicate memory inventory | 27 | Distinguish exact duplicate, intentional copy, revision, and temporal evolution. |
| Structured-conflict inventory | 761 | Candidate generation only; adjudicate bounded packets before any supersession. |
| Duplicate observation inventory | 149 | Decide whether values are duplicated facts, repeated observations, or history. |
| Relation vocabulary drift | 116 | Normalize only through an approved vocabulary mapping with relation-owner CAS. |
| Retrieval substrate inventory | 781 | Determine whether each record is intentionally excluded or missing a required embedding/FTS/graph/Page substrate. |

The calling-agent dogfood packet rejected sampled contradiction, Page
faithfulness, Page-evidence removal, and memory-entity removal candidates when
the excerpts showed duplicates or direct support. Those sampled rows are not
cleanup candidates. Truncated populations remain unresolved rather than clean.

## Historical Telemetry

The incomplete external aggregate listed historical enrichment-step owners,
legacy Page-source owners, and terminal queue receipts. Those counters are not
accepted evidence and must not drive deletion. Before any future retention
cleanup, define the telemetry retention contract and prove the row is not
needed for audit, retry, or migration diagnosis.

## Environment Or Config

- `count_oracle_mismatch`: external SQLite selected the libSQL vector index for
  `COUNT(*)` on `pages` and returned zero while enumeration and `COUNT(id)`
  returned 191. Keep the probe incomplete; do not relabel this as data loss.
- `operations.source_configuration`: the isolated daemon intentionally used an
  empty source config so it could not read live paths or secrets. Its five
  checkpoint findings are scratch-environment findings.
- `serving.route_scope_contracts`: 36 expected route/scope observations were
  absent. This is runtime telemetry/configuration, not a stored-content repair.
- Deep semantic provider unavailable: plain Deep correctly returned exit 2.
  Agent-assisted Deep is the supported native harness route; configured
  provider selection remains future provider-slot CLI work.
- Deep progress: three calls took 83.83-84.63 seconds with no intermediate CLI
  progress. This needs a canonical progress/cancellation design, not data
  cleanup.
- Truncated judged findings: current reports hide known packet findings inside
  incomplete semantic rows. Fix the versioned report/rendering contract before
  using agent-assisted Deep as a cleanup queue.

## Do Not Touch

- Multi-chunk memories: chunking is a supported storage representation, not
  residue by itself.
- Additional legal Page evidence beyond the minimum source locator.
- Unconfirmed creation-kind states, until a real executable review-debt
  contract exists.
- Directly supported cross-space Page evidence sampled by agent-assisted Deep.
- Duplicate or similar text that may represent temporal evolution.
- Any owner represented only by a run-local opaque ordinal.
- Any aggregate from the invalidated real-store probe.

## Apply Gate

Application is a separate phase and requires explicit approval. Each proposed
repair must be produced by a versioned lint-repair skill or general agent using
canonical writers, then pass all of these gates:

1. Resolve opaque evidence to a durable owner without exposing content in logs.
2. Capture expected version/content receipt and a rollback artifact.
3. Explain the proposed mutation and its semantic basis.
4. Obtain approval; use CAS and one canonical transaction/write boundary.
5. Rerun General and applicable agent-assisted Deep.
6. Prove the target finding disappeared, unrelated DB/Page fingerprints did not
   change, and no new finding or incomplete row was introduced.

This file is a review queue definition, not an executable repair manifest.
