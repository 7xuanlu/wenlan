# Reviewer setup (reviewer-owned library + public relay)

Status: not ready to send. The release pin and download are unresolved:
public v0.18.6 still carries the old relay. An internal Developer ID-signed
release-mode candidate has passed isolated startup and restart, but is not
notarized or published and is not final clean-install evidence.

Scope: your own computer, a synthetic local library, the public relay.
No developer-owned always-on device, hosted knowledge runtime, or paid
host. All positive test cases use `atlas-review` specifically; do not
substitute another Space name unless the review cases are updated too.

## 1. Install and open the App

Install the pinned signed release once it exists. Open the Wenlan App on
your own computer. The App starts and manages its local MCP; do not launch
a separate `wenlan-mcp serve` listener for ChatGPT.

### Internal candidate bootstrap (not the final reviewer installer)

`launch-candidate.mjs` is the bounded macOS candidate entry. It requires
Node.js 22+, the full matching App bundle, its verified executable SHA256, and
an existing model-cache directory. Verify the selected bundle's signing and
notarization separately; the launcher checks its executable hash, not those
distribution properties. Historical debug candidates are only ad-hoc signed.
The internal release-mode candidate is Developer ID-signed but not notarized.
Do not substitute
the current public release or present this command as final clean-install proof.

Set the variables below to explicit absolute paths. The profile's parent must
exist, but the profile itself must not exist. From this directory:

```sh
: "${WENLAN_REVIEW_APP:?Full candidate .app/Contents/MacOS/wenlan-app path}"
: "${WENLAN_REVIEW_APP_SHA256:?Verified candidate executable SHA256}"
: "${WENLAN_REVIEW_PROFILE:?New absolute profile directory}"
: "${WENLAN_REVIEW_MODEL_CACHE:?Existing absolute model-cache directory}"
node launch-candidate.mjs --app "$WENLAN_REVIEW_APP" \
  --sha256 "$WENLAN_REVIEW_APP_SHA256" --profile "$WENLAN_REVIEW_PROFILE" \
  --cache "$WENLAN_REVIEW_MODEL_CACHE" --duration-seconds 900 \
  --confirm-synthetic-library
```

This creates isolated home/data/pages paths, starts only the selected App,
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
enable Web access, or approve a connection. The final signed installer and
release-ready isolated launcher remain separate delivery gates.

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
