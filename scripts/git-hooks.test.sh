#!/usr/bin/env bash
set -euo pipefail

bash -n .githooks/pre-commit
bash -n .githooks/pre-push

grep -Fq 'cargo clippy $TOUCHED_CRATES -- -D warnings' .githooks/pre-commit
grep -Fq 'cargo metadata --format-version 1 --locked --no-deps' .githooks/pre-commit
if grep -Fq 'cargo check --workspace' .githooks/pre-commit; then
  echo 'pre-commit must not compile the complete workspace for ownerless inputs' >&2
  exit 1
fi

grep -Fq 'scripts/m5-reader-sweep.py --check' .githooks/pre-commit
if grep -Fq '"$PYTHON_BIN" scripts/m5-reader-sweep.py --update-inventory' .githooks/pre-commit; then
  echo 'pre-commit must not update the M5 reader inventory' >&2
  exit 1
fi
if grep -Fq 'git add "$INVENTORY"' .githooks/pre-commit; then
  echo 'pre-commit must not stage the M5 reader inventory' >&2
  exit 1
fi

# An unstaged source change left by another actor must not expand the hook's
# scope when the caller only staged an unrelated document. A staged M5 source
# change still runs --check, without mutating the caller's staged set.
PRECOMMIT_TMP=$(mktemp -d "${TMPDIR:-/tmp}/wenlan-precommit.XXXXXX")
trap 'rm -rf -- "$PRECOMMIT_TMP"' EXIT
mkdir -p "$PRECOMMIT_TMP/.githooks" "$PRECOMMIT_TMP/scripts" "$PRECOMMIT_TMP/crates/wenlan-core/contracts" "$PRECOMMIT_TMP/bin"
cp .githooks/pre-commit "$PRECOMMIT_TMP/.githooks/pre-commit"
PRECOMMIT_LOG="$PRECOMMIT_TMP/invocations.log"
printf '#!/usr/bin/env bash\nprintf "%%s\\n" "$*" >> "$PRECOMMIT_LOG"\nif [ "${2:-}" = --update-inventory ]; then printf generated > crates/wenlan-core/contracts/m5-reader-manifest-inventory.md; fi\n' > "$PRECOMMIT_TMP/bin/python3"
chmod +x "$PRECOMMIT_TMP/bin/python3"
git -C "$PRECOMMIT_TMP" init -q
git -C "$PRECOMMIT_TMP" config user.email hooks@example.test
git -C "$PRECOMMIT_TMP" config user.name hooks-test
printf 'before\n' > "$PRECOMMIT_TMP/crates/wenlan-core/changed.txt"
printf 'baseline inventory\n' > "$PRECOMMIT_TMP/crates/wenlan-core/contracts/m5-reader-manifest-inventory.md"
git -C "$PRECOMMIT_TMP" add -- crates/wenlan-core/changed.txt crates/wenlan-core/contracts/m5-reader-manifest-inventory.md
git -C "$PRECOMMIT_TMP" commit -q -m baseline
printf 'after\n' > "$PRECOMMIT_TMP/crates/wenlan-core/changed.txt"
printf 'staged\n' > "$PRECOMMIT_TMP/staged.txt"
git -C "$PRECOMMIT_TMP" add -- staged.txt
precommit_staged_before=$(git -C "$PRECOMMIT_TMP" diff --cached --name-only)
if ! (cd "$PRECOMMIT_TMP" && PATH="$PRECOMMIT_TMP/bin:$PATH" PRECOMMIT_LOG="$PRECOMMIT_LOG" bash .githooks/pre-commit >hook.out 2>&1); then
  echo 'pre-commit should ignore an unstaged M5 source change when only an unrelated document is staged' >&2
  sed -n '1,80p' "$PRECOMMIT_TMP/hook.out" >&2
  exit 1
fi
precommit_staged_after=$(git -C "$PRECOMMIT_TMP" diff --cached --name-only)
if [ "$precommit_staged_before" != "$precommit_staged_after" ]; then
  echo 'pre-commit changed the caller staging area while ignoring an unstaged M5 source change' >&2
  exit 1
fi
if [ -s "$PRECOMMIT_LOG" ]; then
  echo 'pre-commit ran the M5 inventory check for an unstaged-only source change' >&2
  exit 1
fi
git -C "$PRECOMMIT_TMP" add -- crates/wenlan-core/changed.txt
precommit_staged_before=$(git -C "$PRECOMMIT_TMP" diff --cached --name-only)
if ! (cd "$PRECOMMIT_TMP" && PATH="$PRECOMMIT_TMP/bin:$PATH" PRECOMMIT_LOG="$PRECOMMIT_LOG" bash .githooks/pre-commit >hook.out 2>&1); then
  echo 'pre-commit should check a staged M5 source change' >&2
  sed -n '1,80p' "$PRECOMMIT_TMP/hook.out" >&2
  exit 1
fi
precommit_staged_after=$(git -C "$PRECOMMIT_TMP" diff --cached --name-only)
if [ "$precommit_staged_before" != "$precommit_staged_after" ]; then
  echo 'pre-commit changed the caller staging area while checking a staged M5 source change' >&2
  exit 1
fi
if ! grep -Fxq 'scripts/m5-reader-sweep.py --check' "$PRECOMMIT_LOG"; then
  echo 'pre-commit did not run --check for a staged M5 source change' >&2
  exit 1
fi
if grep -Fq -- '--update-inventory' "$PRECOMMIT_LOG"; then
  echo 'pre-commit attempted to update the M5 reader inventory' >&2
  exit 1
fi
echo 'pre-commit staged/unstaged inventory contract: PASS'

grep -Fq 'scripts/m5-reader-sweep.py --check' .githooks/pre-push
grep -Fq 'lint::serving::tests::review_tests::route_catalog_freezes_exact_global_and_scoped_keys' .githooks/pre-push
grep -Fq '1 passed' .githooks/pre-push

fast_gate_line=$(grep -n 'scripts/m5-reader-sweep.py --check' .githooks/pre-push | head -n 1 | cut -d: -f1)
planner_line=$(grep -n 'scripts/ci_test_plan.py local' .githooks/pre-push | head -n 1 | cut -d: -f1)
if [ "$fast_gate_line" -ge "$planner_line" ]; then
  echo 'pre-push must run the fast drift gates before the ci_test_plan.py planner' >&2
  exit 1
fi

grep -Fq 'scripts/ci_test_plan.py local' .githooks/pre-push
if grep -Eq 'cargo (check|clippy|test) --workspace' .githooks/pre-push; then
  echo 'pre-push must delegate changed-owner routing to the fail-closed planner' >&2
  exit 1
fi

# WENLAN_PUSH_FULL must guard both compiling steps: the route-catalog cargo
# test and the ci_test_plan.py planner. Everything before the guard line must
# stay non-compiling.
guard_line=$(grep -n '"\${WENLAN_PUSH_FULL:-}"' .githooks/pre-push | head -n 1 | cut -d: -f1)
[ -n "$guard_line" ] || { echo 'pre-push must gate the heavy section behind WENLAN_PUSH_FULL' >&2; exit 1; }
cargo_line=$(grep -n 'cargo ' .githooks/pre-push | head -n 1 | cut -d: -f1)
if [ -n "$cargo_line" ] && [ "$guard_line" -ge "$cargo_line" ]; then
  echo 'pre-push must gate every cargo invocation behind WENLAN_PUSH_FULL' >&2
  exit 1
fi
if [ "$guard_line" -ge "$planner_line" ]; then
  echo 'pre-push must gate the ci_test_plan.py planner behind WENLAN_PUSH_FULL' >&2
  exit 1
fi

# Rebase-safe base: every assignment to `base` must go through git merge-base,
# so a stale remote tip (the sha git passes on stdin) can never become the diff
# base again in any spelling (base="$remote_sha", base=$remote_sha, ...).
if grep -E '^[[:space:]]*base=' .githooks/pre-push | grep -qv 'merge-base'; then
  echo 'pre-push must derive the changed-files base from git merge-base' >&2
  exit 1
fi

# The reader-inventory check is the one gate that must still run by default,
# so it has to sit above the WENLAN_PUSH_FULL guard.
if [ "$fast_gate_line" -ge "$guard_line" ]; then
  echo 'pre-push must run the reader-inventory check before the WENLAN_PUSH_FULL guard' >&2
  exit 1
fi

echo 'git hook routing contracts: PASS'
