# Reviewer setup (reviewer-owned library + public relay)

Status: not ready to send. The concrete distribution prerequisite is a final
reviewer-installable App package configured for the standalone relay origin,
followed by clean-machine signature/notarization and installation evidence.
The current R38 v0.18.10 candidate is locally ad-hoc signed. Earlier startup
and restart receipts for a Developer ID-signed candidate are historical and do
not establish R38's distribution status. R38 is not a public installer or final
clean-install evidence. This guide makes no claim of a
notarized/public installer or OpenAI eligibility.

Scope: your own computer, a synthetic local library, the public relay.
Install Wenlan locally, prepare the synthetic `atlas-review` dataset in the
App-owned local daemon, and use the public standalone relay only to forward
authorized requests to that device. The knowledge library is not hosted by a
developer or a paid backend, and the device must be online while it is queried.
There is no MSI or hosted-knowledge path in this procedure. All positive test
cases use `atlas-review` specifically; do not substitute another Space name
unless the review cases are updated too.

## 1. Install and open the App

Install the pinned signed release once it exists. Open the Wenlan App on
your own computer. The App starts and manages its local MCP; do not launch
a separate `wenlan-mcp serve` listener for ChatGPT.

### Internal candidate bootstrap (not the final reviewer installer)

`launch-candidate.mjs` is the bounded macOS candidate entry. It requires
Node.js 22+, the full matching App bundle, its verified executable SHA256, and
a dedicated existing model-cache directory, which may be empty. Verify the selected bundle's signing and
notarization separately; the launcher checks its executable hash, not those
distribution properties. The current R38 release-mode candidate is ad-hoc
signed, not Developer ID-signed or notarized. Pin the exact candidate manifest;
build mode and a previous version's signature are not signing evidence.
A first launch with an empty cache downloads the embedding model. An isolated
empty-cache launch completed fixture preparation with approximately 209 MB of
cache on the tested Mac. No developer-provided model cache is required for that
path. Initial model download requires internet access; slow or interrupted
downloads have not been validated. The launcher's 40-second readiness deadline
may be insufficient on a slow connection. Preserve failed profile receipts
instead of automatically retrying against a partially prepared library.
Do not substitute
the current public release or present this command as final clean-install proof.

Set the variables below to explicit absolute paths. The profile's parent must
exist, but the profile itself must not exist. From this directory:

```sh
: "${WENLAN_REVIEW_APP:?Full candidate .app/Contents/MacOS/wenlan-app path}"
: "${WENLAN_REVIEW_APP_SHA256:?Verified candidate executable SHA256}"
: "${WENLAN_REVIEW_PROFILE:?New absolute profile directory}"
: "${WENLAN_REVIEW_MODEL_CACHE:?Dedicated existing absolute cache directory, optionally empty}"
node launch-candidate.mjs --app "$WENLAN_REVIEW_APP" \
  --sha256 "$WENLAN_REVIEW_APP_SHA256" --profile "$WENLAN_REVIEW_PROFILE" \
  --cache "$WENLAN_REVIEW_MODEL_CACHE" --duration-seconds 900 \
  --confirm-synthetic-library
```

This creates isolated home/data/pages paths, starts only the selected App,
disables optional usage-statistics uploads for synthetic runs,
checks its live pages path, and runs the complete preparation in step 2. Do not
run `prepare.mjs` again on that profile. The `READY` line identifies the generated
test-case receipt. The App stops when the window expires or the command is
interrupted; inspect the final lifecycle receipt before treating cleanup as done.
The profile and logs are retained, never automatically deleted or overwritten.

To reopen a successfully stopped profile, repeat the same command with `--resume`.
The same candidate path/hash, cache and stored ports are required. The existing
fixture is checked, not imported again. r3 profiles without a `reviewer-profile.json`
record cannot be resumed; create a fresh r4 profile instead of manufacturing that
record. A remaining `.reviewer-launch.lock`, changed paths, changed preparation
receipt or failed prior shutdown stops the command before App launch. Do not
delete the lock or reset the library automatically; retain it for investigation.

The launcher neither enables Web access nor creates or revokes grants. Keep Web
access disabled for bootstrap validation. Stopping the App is not revocation of
any separately approved grant. Live pairing and relay recovery still need the
separate acceptance flow below; local restart evidence does not prove them.

## 2. Prepare the complete synthetic library

This preparation tool requires Node.js 22 or newer and the matching source
package, in addition to the candidate App. It does not install or start Wenlan,
enable Web access, or approve a connection. It is setup tooling, not an
installer, MCP backend, or hosted knowledge service. The final reviewer package
and release-ready isolated launcher remain separate delivery gates.

Start from an empty isolated library; do not run the manual CLI steps below
first. Set `WENLAN_HOST` to that daemon's full nondefault loopback URL,
`WENLAN_REVIEW_KNOWLEDGE_PATH` to its exact absolute pages directory, and
`WENLAN_REVIEW_RECEIPT` to a new absolute JSON output path. From this directory:

```sh
: "${WENLAN_HOST:?Set the confirmed isolated daemon URL first}"
: "${WENLAN_REVIEW_KNOWLEDGE_PATH:?Set the isolated absolute pages directory}"
: "${WENLAN_REVIEW_RECEIPT:?Set a new absolute receipt file path}"
node prepare.mjs --daemon-url "$WENLAN_HOST" \
  --knowledge-path "$WENLAN_REVIEW_KNOWLEDGE_PATH" \
  --output "$WENLAN_REVIEW_RECEIPT" --confirm-synthetic-library
```

The command checks the daemon's pages path, refuses existing fixture Spaces and
never overwrites an output file. It creates two Spaces, a Brief, three memories
and three authored source-linked pages through the ordinary local APIs. One
source is moved outside the shared Space to exercise unavailable evidence.
No direct database writes, automatic page confirmation or hard-coded tool results.

Use the generated receipt's five positive and three negative test cases, which
contain this library's real IDs. Do not reuse the old `page_atlas-auth` or
`mem_atlas-auth` IDs. Share only `atlas-review`; leave `private-sentinel` private.
A `prepared` receipt is setup evidence, not a successful ChatGPT test or approval.
On failure, preserve the receipt and partial library for inspection; do not
automatically retry, overwrite or delete either. No rollback is attempted.

### Manual CLI alternative (partial setup only)

Precondition: an already isolated, running daemon holding only synthetic
reviewer data. A release-ready isolated-launch package is still missing; do not run these
commands against a personal profile. Set `WENLAN_HOST` to its confirmed full
loopback URL, not the default daemon. Run from this directory only after checking
that `atlas-review` does not already exist. These steps are not a complete import.
The launcher must also isolate `HOME`/`USERPROFILE` and `WENLAN_DATA_DIR`; a
different HTTP port alone does not isolate queued writes or local page reads.

```sh
: "${WENLAN_HOST:?Set the confirmed isolated daemon URL first}"
export WENLAN_NO_AUTOSTART=1
wenlan --format json spaces list
wenlan --format json spaces add atlas-review
wenlan --format json capture --space atlas-review --type decision --file atlas-authentication.txt
wenlan --format json brief update --space atlas-review --file atlas-brief.json
wenlan --format json brief --space atlas-review
```

Notes (source: `crates/wenlan-cli/src/main.rs`,
`commands/brief.rs`, `commands/space.rs`, `commands/pages.rs`):

- Record the generated memory and page IDs for the test cases, never use old
  seed IDs. The `pages --resolve-id` command reads the configured local page
  directory, not the daemon HTTP endpoint; do not assume `--space` makes that
  local-file lookup Space-filtered. Use the isolated App to check the page's Space.
- `brief update --file` reads a JSON `BriefUpdateRequest` from the file:
  `space`, `caller_id`, `operation_id`, plus `summary` and `mutations`.
  The file's `space` must match `--space`.
- Require actual created/applied receipts and a matching Brief readback.
  A `queued` result means the daemon was unreachable and the write was stored
  locally; it does not prove success. Stop and inspect the outbox before retrying
  so delayed replay cannot duplicate setup. On conflicts or partial application,
  stop and inspect rather than overwriting data or automatically reseeding.
- These manual CLI commands do not create source-linked pages. Use the complete
  preparation command on a fresh library for the full review fixture instead.

## 3. Enable Web access and pair ChatGPT

In the App, open "Web access" ("Experimental"). Manual consent, in order:

1. "Shared Space": select `atlas-review`.
2. Read the disclosure ("Authorized requests and results pass through
   wenlan-relay to your AI client. Your library stays on this device;
   the device must be online.") and check the consent box ("Allow remote
   queries only in atlas-review. Other and future Spaces stay private.").
3. Enable Web access.
4. In ChatGPT, open the Wenlan OAuth flow and copy the "Pairing code".
5. In Wenlan, paste it under "Authorize a connection" and choose
   "Review request" to inspect "Client ID" and "Expires".
6. Choose "Approve connection". There is no automatic approval.
7. Back in the browser/AI client, Continue to finish the connection.

No universal page-confirmation requirement is assumed: whether a page is
queryable is established by exercising the real query path.

## 4. Revoke or stop

- "Revoke access" removes one grant under "Authorized connections".
- "Stop access" stops all remote access from this App.
These are distinct actions. "Access revoked. Stored token cleanup is pending."
means that grant has been denied and stored-token cleanup needs a retry.
"Local access is stopped. Remote revocation is pending" is a different state:
use "Retry disconnect" and do not claim remote revocation is confirmed yet.

## 5. Local Codex MCP (separate, optional)

Local Codex launches its configured `wenlan-mcp` stdio process on this machine;
that process talks to the local daemon. It is separate from the Web access
flow above and requires no public endpoint or relay pairing. Verify the target
daemon and Space in that client's configuration before querying review data.

## Verification and remaining gaps

### Acceptance run from the final package

This checklist is a procedure, not a completed receipt. Record the package hash,
App version, OS, client/version, relay origin, date, generated preparation receipt
and outcome of each step. Keep access tokens, management credentials and pairing
codes out of recordings, public logs and submission screenshots. Use synthetic
data only. Retain detailed raw evidence privately with credentials redacted.

1. **Distribution:** download the pinned final package on a clean supported
   reviewer machine. Verify its signature/notarization and complete installation
   without bypassing an OS security warning. Record prerequisites and actual
   first-run behavior. The internal Node/cache bootstrap above is not this gate.
2. **Setup and consent:** prepare one fresh isolated library, keep Web access off
   until the receipt succeeds, and follow step 3's normal consent flow. Record
   that only `atlas-review` is shared. Do not reseed a partially prepared profile
   or silently share `private-sentinel` to make a case pass.
3. **ChatGPT positives:** run all five `test_cases` from the generated receipt,
   using its `user_prompt`, `tools_triggered` and `expected_output`. Save both the
   actual tool output and model answer. Check Brief items, separately labeled
   related context, at most three recall hits, source-ID correspondence, and
   the unavailable page's empty sources without leaked IDs or invented reasons.
   The placeholders in the source manifest are not the generated fixture IDs.
4. **ChatGPT negatives:** run all three `negative_test_cases`: general knowledge,
   saving a memory and local software setup. Check that no Wenlan tool is invoked
   and no unsupported save or installation is claimed. An answer alone without
   tool-use evidence does not establish non-invocation.
5. **Offline and recovery:** stop the synthetic local runtime without revoking
   its grant. Ask a new library-specific question; require an explicit unavailable
   outcome, not a remembered answer or an indefinite spinner. Restart the same
   profile without reseeding and repeat a positive case. If an old MCP session
   receives 404, record whether the client actually reinitializes and succeeds;
   manual HTTP reinitialization is not proof of automatic client recovery.
   Separately exercise sleep/wake and a prolonged outage and record durations.
6. **Revocation:** revoke this test's grant, then verify a fresh tool request is
   denied. Stop all Web access and verify no request reaches the library. Record
   the two pending-cleanup states described in step 4 accurately; an App exit or
   a UI click alone is not confirmed remote revocation. Never revoke unrelated
   connections. New access after revocation requires a new explicit consent flow.
7. **Codex:** exercise the public relay separately if claiming relay support in
   that client. Also run the local stdio lane with the isolated daemon/Space,
   confirming that it works with Web access disabled. Do not imply that ChatGPT's
   desktop app automatically supports this local stdio configuration.
8. **Demo and portal:** record the real install/pair/query/source/offline/recovery
   flow from the accepted candidate. Exclude secrets and unrelated windows.
   Verify that the reviewer can open the recording URL without your login, then
   enter it in the portal's Demo Recording URL field. Per-case attachment/output
   URL fields are not substitutes for that field. Align portal cases with the
   generated receipt and verify public privacy/terms pages and tool scan against
   this same candidate. Stop before final attestations, submission or publication.

Mark a failed, skipped or unavailable step explicitly. A loopback tool pass,
mocked UI, complete portal form or this checklist does not prove review readiness.

The internal candidate launcher/state helpers passed 32 focused tests on
2026-09-13 UTC. A real full-App test created a fresh synthetic library, stopped
the App and daemon, then reopened the same profile without re-seeding. Generated
IDs and actual scoped source readbacks matched across both launches; both daemon
port closure checks passed. No Web access was enabled. This is local persistence
evidence, not OAuth, relay reconnect, model query, UI or clean-install acceptance.

The complete preparation command passed 42 focused unit tests and a fresh
native integration on 2026-09-13 UTC. Its real generated pages and source IDs
passed all five authenticated loopback MCP query cases, including unavailable
source omission and private-Space isolation. No page confirmation flags were
modified. This is not model-mediated ChatGPT or clean-install evidence.

Preparation also re-reads scoped page sources after moving the unavailable
memory. A successful move acknowledgement alone does not validate isolation;
missing or malformed source-memory fields fail preparation.

The five CLI commands above were executed successfully against a fresh isolated
real daemon with the included files on 2026-09-13 UTC. Capture returned `created`;
the Brief update applied the summary and both items with no conflicts, and its
readback was `ready`. `spaces add` prints text even with `--format json`; that
message alone is not the fixture validation. Owned daemon shutdown was verified.

Signed release pin/download; release-ready isolated launch; execution of these
steps by a reviewer from the final package; live installed-App consent and recovery; deployed relay and real
ChatGPT checks on this exact candidate; packaged sleep/wake and prolonged-outage
recovery. Earlier protocol/client receipts are not proof for this setup flow.
