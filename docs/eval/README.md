# Eval Snapshot Workflow

This project keeps publishable benchmark numbers in a local gitignored file so README metrics can be updated without committing private baseline files.

## Files

- Local metrics (gitignored): `${EVAL_BASELINES_DIR:-~/.cache/origin-eval}/readme_metrics.json`
- Tracked template: `docs/eval/readme_metrics.example.json`
- README updater: `scripts/update-readme-eval.py`

## Update flow

1. Run benchmark(s) locally and record headline metrics.
2. Update `${EVAL_BASELINES_DIR:-~/.cache/origin-eval}/readme_metrics.json`.
3. Regenerate README snapshot:

```bash
python3 scripts/update-readme-eval.py
```

4. Commit the README and script/docs changes (the local metrics JSON stays untracked).

## Notes

- LongMemEval and LoCoMo use `Recall@5`, `MRR`, and `NDCG@10` as headline fields.
- Current README numbers are retrieval-only, single-run local snapshots unless a reproducibility pass is explicitly documented.
- Name the retrieval mode once in surrounding prose when all rows use the same mode.
- Keep `notes` in the metrics JSON for maintainer-facing caveats and run metadata; the root README does not render them.

## Links

- [useorigin.app](https://useorigin.app) — project home
- [useorigin.app#benchmarks](https://useorigin.app/#benchmarks) — the public benchmark table sourced from this workflow
