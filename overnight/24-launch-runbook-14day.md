# The 14-Day Launch Runbook (the operational capstone)

This turns the FINAL recommendation (00-START-HERE: launch once, first, with a kill/continue gate) into a
dated checklist. It exists because your failure mode is not knowing what to do, it is not doing the scary
outward thing. So here it is as a list you execute, not a decision you reopen.

One rule for the whole 14 days: **no retrieval tuning, no CI edits, no README rewrites.** If you feel the
pull, that is the avoidance talking. Run `bash overnight/tools/self-dashboard.sh 1` and look at the inward %.

## Set the gate NOW (before anything else)

Write these two numbers down today, publicly in the launch tracking issue, so you cannot move them later:
- CONTINUE if, 14 days after the Show HN, you have at least **___ installs** AND at least **___ real
  conversations** with users who tried it (issues, DMs, comments with specifics).
- Suggested defaults to fill in if you are unsure: 50 installs and 5 real conversations. Adjust up if you
  believe the wedge, down if you want a lower bar. The point is that the number is fixed before the data.
- If you miss the gate: stop building Origin as a product, pivot to the research-engineer path (22) with the
  field guide + citable-number as your portfolio. The launch was not wasted; a launched-and-measured product
  plus a published guide is a stronger job artifact than either alone.
- If you hit the gate: you have real users. Now retrieval tuning is justified, because real people feel it.

This gate is the whole point. It makes the launch a cheap reversible experiment instead of an identity bet.

## Days 1-2: make the product safe to show

- [ ] Fix the two remaining /review bugs from 13-review-fix-plan.md (bucket docs-sync + one atomic supersede
      endpoint). Half a day. The backend is already done; this is finishing, not building. [13]
- [ ] Run the real product end to end yourself, as a new user would: fresh install, /init, capture, /review,
      recall. Note every rough edge. Do NOT fix rough edges beyond /review; just write them down.
- [ ] Reply substantively to issue #194 (kiluazen). A real human asked a real design question and it has sat
      for days. Answering it well is the single cheapest user-retention act available, and it warms up one
      genuine relationship before launch. [09]
- [ ] Close issue #92 (or update it) now that /review works. Public hygiene before strangers arrive.

## Days 3-5: prep the two launch assets

- [ ] Polish the README above-the-fold per 18-launch-playbook: the one-line install, a 30-second demo
      gif/video, the honest one-line ask. [18]
- [ ] Finalize the field guide (10-field-guide.md). It is already drafted and fact-checked (the Zep three-way
      correction is in). This is your distribution vehicle. [10]
- [ ] Decide the launch order. Recommended: publish the FIELD GUIDE first as a Show HN ("Show HN: An honest
      field guide to AI memory benchmarks"), because it has no direct competitor and sells the product
      sideways without a "this already exists" pile-on. Then launch the product a few days later into warmed
      air. The product Show HN leads with composition + enforced provenance, NOT "git-versioned local
      memory" (that headline loses to sverklo and a dozen others). [16, 09, 18]
- [ ] Pre-write your replies to the 5 most likely critical comments (it already exists / why a Rust daemon /
      vs sverklo / vs mem0 / does provenance matter). Honest answers, drafted calm, not defensive. [17, 18]

## Day 6 (Tue-Thu, ~8-10am ET): launch the field guide

- [ ] Post the field guide as Show HN. Be at your desk. Reply to EVERY comment in the first 3 hours. Star
      impact half-life is ~24h and ~92% spent in 48h, so presence in the first window is the whole game. [18]
- [ ] Cross-post to r/LocalLLaMA and r/MachineLearning per the channel etiquette (native post, not a link
      drop). [18]
- [ ] Do not argue. Ask questions. Every critical commenter is free user research.

## Days 7-9: launch the product into warmed air

- [ ] Show HN the product. Lead with composition + provenance. Honest ask: "is on-device + provenance
      something you actually want, or just something I wanted to build." That line is both HN-resonant and
      the user-validation question you have never asked. [09, 18]
- [ ] Post to r/LocalLLaMA (the ICP's home turf) and r/ClaudeAI. [09]
- [ ] Post in the Claude Code plugin community / marketplace channels. [18]
- [ ] Reply to everything. Log every install signal and every conversation in the tracking issue.

## Days 10-14: watch, talk, measure (do NOT build)

- [ ] Run `bash overnight/automations/weekly-review/weekly-review.sh` and read it. [weekly-review]
- [ ] Get on a call (or a real DM thread) with at least 3 people who tried it. Ask what they expected, where
      they quit, what they would pay for. Do not pitch. Listen. This is the data you have never had.
- [ ] Tally against the gate: installs and real conversations. Write the number in the tracking issue.

## Day 14: decide from data, not mood

- [ ] Compare to the gate you set on Day 0. CONTINUE or PIVOT. Either is a win, because either way you now
      have the one thing six weeks of rigor never produced: evidence about whether a stranger wants this.

## VERIFICATION
- Every step links to a kit file that contains the actual asset or plan it references (13 for the /review
  fix, 10 for the guide, 09/18 for the launch assets, 16/17 for the positioning, 22 for the fallback, the
  weekly-review tool for the measurement). No step asks for work not already drafted or specified.
- The mechanics (Tue-Thu 8-10am ET, reply in first 3h, 24h half-life) are from 18-launch-playbook, which
  cited them to sources. [VERIFIED via 18]
- This runbook adds no new claims; it sequences existing, verified deliverables into a dated path. Its only
  opinion is the launch ORDER (guide first), labeled as a recommendation. [OPINION on ordering]
- Honest limit: I cannot make you do this. The kit can remove every excuse except the real one, which is that
  standing in front of strangers is uncomfortable. That discomfort is the signal you are finally doing the
  right work.
