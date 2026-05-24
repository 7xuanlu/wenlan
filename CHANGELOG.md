# Changelog

## [0.7.0](https://github.com/7xuanlu/origin/compare/v0.6.1...v0.7.0) (2026-05-24)


### Features

* **cli:** implement Windows install via schtasks ([#162](https://github.com/7xuanlu/origin/issues/162)) ([ed9b96f](https://github.com/7xuanlu/origin/commit/ed9b96f6a76eaec4a7b2a32dbc6b7debfa9dd48b))
* **cli:** origin space subcommands + doctor resolver state (Plan C) ([#159](https://github.com/7xuanlu/origin/issues/159)) ([fd28fb2](https://github.com/7xuanlu/origin/commit/fd28fb2915364a631531bc8d3bb00fbd06881055))
* cross-platform Linux and Windows support ([#150](https://github.com/7xuanlu/origin/issues/150)) ([e732909](https://github.com/7xuanlu/origin/commit/e7329092884d063172d02a8898cf2b11ae81da29))
* **eval:** KG-faithfulness benchmark (Plan C-B) ([#149](https://github.com/7xuanlu/origin/issues/149)) ([93b9982](https://github.com/7xuanlu/origin/commit/93b998288db6ca76adb371d4adf99886a55374ce))
* **eval:** LLM judge for KG-faithfulness (Plan C-C) ([#152](https://github.com/7xuanlu/origin/issues/152)) ([f09fcf2](https://github.com/7xuanlu/origin/commit/f09fcf2878dea156e30d414082267efb6a5bab1e))
* **eval:** page-distillation faithfulness benchmark (Plan C-D) ([#151](https://github.com/7xuanlu/origin/issues/151)) ([eda861c](https://github.com/7xuanlu/origin/commit/eda861c11d3611d8e4872e3287d7e775bd64bbc2))
* **eval:** reproducibility foundations (Plan A) ([#145](https://github.com/7xuanlu/origin/issues/145)) ([a8424ef](https://github.com/7xuanlu/origin/commit/a8424ef9453dca62c3b4c0ed8bcd801a9e35cefe))
* **eval:** structured binary judge via tool_use ([#164](https://github.com/7xuanlu/origin/issues/164)) ([23dba48](https://github.com/7xuanlu/origin/commit/23dba48dec7bcaedd50f3ba65d00367b4c07d319))
* **plugin:** space resolver + 6-layer chain (Plan A) ([#153](https://github.com/7xuanlu/origin/issues/153)) ([0916e8c](https://github.com/7xuanlu/origin/commit/0916e8c5c83edd2c96b6d2e02d8124de2db12e95))
* **server, mcp:** X-Origin-Space header + tool schema gating (Plan B) ([#156](https://github.com/7xuanlu/origin/issues/156)) ([285d11a](https://github.com/7xuanlu/origin/commit/285d11a78406585e08169c92bc669ac6ee7bac4c))


### Bug Fixes

* **ci:** release.yml — publish-crates correctness + add origin CLI to Homebrew tap ([#163](https://github.com/7xuanlu/origin/issues/163)) ([6780986](https://github.com/7xuanlu/origin/commit/67809868c982cccd39ab20b4ed38bc51569e3ab4))
* **core:** serialize EVAL_MAX_USD env touches in eval_harness tests ([#160](https://github.com/7xuanlu/origin/issues/160)) ([ae9253c](https://github.com/7xuanlu/origin/commit/ae9253c87e72084cc2879362ccfc08ac7a60d93b))
* **docker:** switch daemon image base to Debian trixie ([#158](https://github.com/7xuanlu/origin/issues/158)) ([97642c2](https://github.com/7xuanlu/origin/commit/97642c20857f159d6c34a748de18cafda063cecd))
* **eval:** restore app/eval/fixtures to monorepo for L6 CI canary ([#148](https://github.com/7xuanlu/origin/issues/148)) ([379e2bc](https://github.com/7xuanlu/origin/commit/379e2bc9b961ef4f1a74dae32b2bb8f5831469e4))
* harden MCP setup distribution path ([d6fb5da](https://github.com/7xuanlu/origin/commit/d6fb5daada78a677d2d61dfe2906f19be3597fad))
* tighten version sync validation ([#139](https://github.com/7xuanlu/origin/issues/139)) ([80db565](https://github.com/7xuanlu/origin/commit/80db5652190a07b6fb79579c6fb01d62a116461b))

## [0.6.1](https://github.com/7xuanlu/origin/compare/v0.6.0...v0.6.1) (2026-05-16)


### Bug Fixes

* sync README to npm packages + republish v0.6.0+ with README content ([#137](https://github.com/7xuanlu/origin/issues/137)) ([43e4ce9](https://github.com/7xuanlu/origin/commit/43e4ce966be985ce5b6888a9ff23360e1cc685d9))

## [0.6.0](https://github.com/7xuanlu/origin/compare/v0.5.3...v0.6.0) (2026-05-16)


### Features

* BM-mode consumer-side accept dispatch ([#96](https://github.com/7xuanlu/origin/issues/96)) ([033ce55](https://github.com/7xuanlu/origin/commit/033ce5570f2e56f384c669e053dcfbcf661db822))
* BM-mode curation mutate MCPs (Spec C-2) ([#105](https://github.com/7xuanlu/origin/issues/105)) ([73aec7a](https://github.com/7xuanlu/origin/commit/73aec7a127f31d31a41b518a6ebd6a37c304ccfa))
* rename domain → space + complete e2e scoping (BREAKING CHANGE) ([#123](https://github.com/7xuanlu/origin/issues/123)) ([7281202](https://github.com/7xuanlu/origin/commit/72812025b0dbf73652bd8654f7016633fc2c76ad))


### Bug Fixes

* auto-supersede conflicting relations (last-write-wins) ([#111](https://github.com/7xuanlu/origin/issues/111)) ([eda6718](https://github.com/7xuanlu/origin/commit/eda67180ee8c390e6d5c4ed666c0e1dc24295936))
* bundle quick wins for CI noise + correctness ([#102](https://github.com/7xuanlu/origin/issues/102)) ([edde4c9](https://github.com/7xuanlu/origin/commit/edde4c9e45ebde81c6a607a7a795e54d1e684752))
* capture inline contradiction signal + surface bug fix ([#110](https://github.com/7xuanlu/origin/issues/110)) ([b35e843](https://github.com/7xuanlu/origin/commit/b35e843d0b15595d0ba17e867906e8e2762011dd))
* **ci:** main-canary filter eval::token_efficiency → eval::retrieval ([#124](https://github.com/7xuanlu/origin/issues/124)) ([ecfd386](https://github.com/7xuanlu/origin/commit/ecfd3867164f7cfebe0e384900532f1bd020249b))
* **ci:** split fmt/lint/test on ubuntu, pin toolchain + SHAs ([#117](https://github.com/7xuanlu/origin/issues/117)) ([f5cd75d](https://github.com/7xuanlu/origin/commit/f5cd75d51994725aa9d312711d13439724a45dbe))
* **core:** apply supersedes_exclusion to MemoryDB::search ([#130](https://github.com/7xuanlu/origin/issues/130)) ([0add226](https://github.com/7xuanlu/origin/commit/0add2268201f794bdf0e3f510f6d0d6a27b8b750))
* **core:** cross-process file lock around FastEmbed init ([#125](https://github.com/7xuanlu/origin/issues/125)) ([d7aaaab](https://github.com/7xuanlu/origin/commit/d7aaaab418ea1f0e5dcee084be3a1e89b5873e24))
* **core:** honor ORIGIN_DATA_DIR in spaces.legacy_db_path ([#135](https://github.com/7xuanlu/origin/issues/135)) ([a5c23ee](https://github.com/7xuanlu/origin/commit/a5c23eebb0b7e6cc1995b7fac7586faea1f41b5a))
* **distill:** respect user_edited + thread knowledge_path + add /distill rebuild ([#106](https://github.com/7xuanlu/origin/issues/106)) ([26a7345](https://github.com/7xuanlu/origin/commit/26a734549e3aed141543b1749deb4f025c89fe52))
* gate MCP curation wrappers to stdio transport ([#122](https://github.com/7xuanlu/origin/issues/122)) ([d874907](https://github.com/7xuanlu/origin/commit/d87490775e2d213148d42d1f8dabbcef2731dbe0))
* handoff pending-captures preview + list_pending plumbing (Spec C-3b) ([#114](https://github.com/7xuanlu/origin/issues/114)) ([4fe5fba](https://github.com/7xuanlu/origin/commit/4fe5fbae8a25f49a9f5f288a075cc43dfc41581a))
* handoff status file uses Active/Backlog two-tier split + date stamps ([#116](https://github.com/7xuanlu/origin/issues/116)) ([636e49a](https://github.com/7xuanlu/origin/commit/636e49aa4e62b4df10e0fe22f58940261b79cfa8))
* **kg:** coerce non-vocabulary relation types to related_to + prompt update ([#100](https://github.com/7xuanlu/origin/issues/100)) ([d6cd5d8](https://github.com/7xuanlu/origin/commit/d6cd5d8e9ce64617aa23e466cbea7640773d6979))
* make origin CLI own runtime setup ([#128](https://github.com/7xuanlu/origin/issues/128)) ([4f6d946](https://github.com/7xuanlu/origin/commit/4f6d946153691821c6c7ae13b3529f7f9e47d174))
* **mcp:** list_spaces tool + activate doc-path space filter ([#126](https://github.com/7xuanlu/origin/issues/126)) ([0ed205f](https://github.com/7xuanlu/origin/commit/0ed205fa21ff351ff38d7be6e1bb7a3d48c069ff))
* **mcp:** observation CRUD wrappers (PR-A of bm-mode extraction) ([#95](https://github.com/7xuanlu/origin/issues/95)) ([fda9b63](https://github.com/7xuanlu/origin/commit/fda9b631fc05a990b4d38b469a944654ce6d9fad))
* **refinery:** thread knowledge_path through re_distill_stale_pages ([#108](https://github.com/7xuanlu/origin/issues/108)) ([21a25a0](https://github.com/7xuanlu/origin/commit/21a25a09002373f120184798595227d11065b3be))
* remove /refinery skill (power-user MCPs stay) ([#109](https://github.com/7xuanlu/origin/issues/109)) ([083f458](https://github.com/7xuanlu/origin/commit/083f4580fe9576a35c49f4938463b17c6536f538))
* remove entity-suggestion mutate MCPs (dead scaffolding) ([#113](https://github.com/7xuanlu/origin/issues/113)) ([fe6fe18](https://github.com/7xuanlu/origin/commit/fe6fe182a77c12608f7375e7d4832ecfe375e972))
* **server:** clone Arc&lt;MemoryDB&gt; before await in 3 space-mutate handlers ([#129](https://github.com/7xuanlu/origin/issues/129)) ([226ae8d](https://github.com/7xuanlu/origin/commit/226ae8d8ed0669064320af43745d69b56c56b8ee))
* **server:** clone Arc&lt;MemoryDB&gt; before await in handle_list_memories ([#136](https://github.com/7xuanlu/origin/issues/136)) ([39a600d](https://github.com/7xuanlu/origin/commit/39a600d4bee3babb1a6b978d806e4b0d13ba93be))
* **server:** clone Arc&lt;MemoryDB&gt; before await in remaining handlers ([#131](https://github.com/7xuanlu/origin/issues/131)) ([7236eeb](https://github.com/7xuanlu/origin/commit/7236eeb4f5a2a728ddbfd4d0abff6e77c1d43e35))
* **skills:** /brief reads status file first + /review drops stale C-3b note ([#121](https://github.com/7xuanlu/origin/issues/121)) ([aa9899e](https://github.com/7xuanlu/origin/commit/aa9899e4bd3bd7a12df208ef14a61d02e0281cf0))
* soft-archive supersede_relation via activity payload ([#120](https://github.com/7xuanlu/origin/issues/120)) ([daf9bc2](https://github.com/7xuanlu/origin/commit/daf9bc20a3e5bd081ea0560ec6a569bb04519b94))
* stop emitting dedup_merge + detect_contradiction proposals ([#112](https://github.com/7xuanlu/origin/issues/112)) ([521498d](https://github.com/7xuanlu/origin/commit/521498d97c5df4984b1704abf97b592de0868e13))
* surface pending revisions in /brief + scoped /review walks (Spec C-3 Phase 1) ([#107](https://github.com/7xuanlu/origin/issues/107)) ([54b4e3b](https://github.com/7xuanlu/origin/commit/54b4e3b522416494a5c647e8991810a4b1f93a91))
* trust-tier auto-supersede for high-confidence contradictions ([#115](https://github.com/7xuanlu/origin/issues/115)) ([0c74271](https://github.com/7xuanlu/origin/commit/0c74271e0ba1ec42b1c1b1878db252a86cdfdb4a))

## [0.5.3](https://github.com/7xuanlu/origin/compare/v0.5.2...v0.5.3) (2026-05-13)


### Bug Fixes

* get_page_sources MCP tool + auto-commit retry (close skill ↔ MCP boundary) ([#85](https://github.com/7xuanlu/origin/issues/85)) ([101b595](https://github.com/7xuanlu/origin/commit/101b59535e8a14836801f8a9b5054af387510377))
* memory + page revision surfacing (Phase 1 of Task [#57](https://github.com/7xuanlu/origin/issues/57)) ([#91](https://github.com/7xuanlu/origin/issues/91)) ([02ddd43](https://github.com/7xuanlu/origin/commit/02ddd43d97f7b4a8d83af5ed24c4f15a437455df))
* **memory_routes:** drop silent topic-match upsert from write path ([#84](https://github.com/7xuanlu/origin/issues/84)) ([46175a0](https://github.com/7xuanlu/origin/commit/46175a0dfc433272994def751828daf8f77e72f7))
* **topic_match:** entity match must also satisfy similarity threshold ([#83](https://github.com/7xuanlu/origin/issues/83)) ([0670772](https://github.com/7xuanlu/origin/commit/067077225ac619d8e0c69deb54a9a0d3d4ec2a01))

## [0.5.2](https://github.com/7xuanlu/origin/compare/v0.5.1...v0.5.2) (2026-05-12)


### Bug Fixes

* handoff skill — categorized confirm output + git retry for index.lock ([01f87da](https://github.com/7xuanlu/origin/commit/01f87da9a37cf289c4cc1659c39504dc68f620f4))
* handoff skill — user-friendly labels mapped to daemon memory types ([9c14c2e](https://github.com/7xuanlu/origin/commit/9c14c2e957720293789246acfed4a4e594221ca2))
* handoff skill uses daemon's 6 canonical memory types ([9c74d34](https://github.com/7xuanlu/origin/commit/9c74d34baa291b68c2e2fe63d3b9de3acbeb7ee3))
* MCP wrappers for /api/pages/search + /api/pages/recent ([#77](https://github.com/7xuanlu/origin/issues/77)) ([6fab560](https://github.com/7xuanlu/origin/commit/6fab56012421a6f3b26b8acf601d009dfb53cdf6))
* **pages:** llm-wiki foundations — user_edited, cluster cap, refresh route, wikilink graph, fs watcher ([#78](https://github.com/7xuanlu/origin/issues/78)) ([a611ae1](https://github.com/7xuanlu/origin/commit/a611ae1c21dad56caacdbd93f5ed7b87fae52b72))
* plugin UX — ~/.origin consolidation, version pins, skill upgrades ([#73](https://github.com/7xuanlu/origin/issues/73)) ([4483dd6](https://github.com/7xuanlu/origin/commit/4483dd607ef1c8e3c9cdfd22a72e5ecc92ae606a))
* PR [#73](https://github.com/7xuanlu/origin/issues/73) follow-ups — daemon version hook + Basic Memory skill phases ([#75](https://github.com/7xuanlu/origin/issues/75)) ([b27c0ef](https://github.com/7xuanlu/origin/commit/b27c0efee67d40c5b70403aa90bad92671c799b8))
* reconcile README with PR [#72](https://github.com/7xuanlu/origin/issues/72) structure ([587b26c](https://github.com/7xuanlu/origin/commit/587b26cf2c532fd67898fec7b829148876634714))
* remove duplicate Repo Map section from README ([f5b946e](https://github.com/7xuanlu/origin/commit/f5b946e41f33d225dede004213ef5f78cd96791e))
* update release-please git-add paths for plugin/ subdir migration ([c711034](https://github.com/7xuanlu/origin/commit/c7110347ad46d6236703035c24575f5258c91799))

## [0.5.1](https://github.com/7xuanlu/origin/compare/v0.5.0...v0.5.1) (2026-05-10)


### Bug Fixes

* align README with monorepo runtime ([#72](https://github.com/7xuanlu/origin/issues/72)) ([ce44ceb](https://github.com/7xuanlu/origin/commit/ce44ceb1e6ac62027b0f5b4366b6d69fdab053da))
* **release:** drop origin-mcp from pre-flight dry-run ([a1b804b](https://github.com/7xuanlu/origin/commit/a1b804b4f4e9c1340cae06c3c26a049b85c35e6b))
* **release:** make origin-types publish idempotent ([afde654](https://github.com/7xuanlu/origin/commit/afde6548d0875ea88b620128831f5259a324159c))
* **release:** use RELEASE_TAG in npm + homebrew jobs ([f087f87](https://github.com/7xuanlu/origin/commit/f087f8772b585b187bed1cd9470b00471613ecf1))

## [0.5.0](https://github.com/7xuanlu/origin/compare/v0.3.1...v0.5.0) (2026-05-10)


### Features

* **mcp:** switch to workspace inheritance, Apache-2.0, path dep on origin-types ([52721f9](https://github.com/7xuanlu/origin/commit/52721f9490b921cb74a9088322fa61c0a6203dd5))
* merge origin-mcp + origin-plugin into monorepo (v0.5.0) ([bc95c84](https://github.com/7xuanlu/origin/commit/bc95c846d0a9b8f8381993a996ec26638e79895c))
* merge origin-mcp into monorepo as crates/origin-mcp/ ([c982ec7](https://github.com/7xuanlu/origin/commit/c982ec738930671f9c7e3eb1f24227fa86fab756))
* merge origin-plugin into monorepo (staging) ([0fdfe0e](https://github.com/7xuanlu/origin/commit/0fdfe0eeb7b683942bec75be6166dffef30b8c34))
* **plugin:** update manifest for monorepo (v0.5.0, repository=origin) ([647bab1](https://github.com/7xuanlu/origin/commit/647bab165e21c56437bc613cbf28839282943e89))
* **scripts:** add validate-versions.sh pre-flight check ([94587d2](https://github.com/7xuanlu/origin/commit/94587d2fc01befafcfe22c5e9dac79227b0abf2c))
* **scripts:** extend bump-version.sh to sync npm + plugin manifests ([cb26bc4](https://github.com/7xuanlu/origin/commit/cb26bc484c51d8a3aabb95647997c08b238f9803))


### Bug Fixes

* bump npm/package.json + Cargo to 0.4.1 (sync after v0.4.0 npm publish skip) ([a8f6a59](https://github.com/7xuanlu/origin/commit/a8f6a5920469d25498c2ca4ee39f63a4363e05b3))
* **ci,npm:** align npm syntax-check paths and metadata ([4c4e240](https://github.com/7xuanlu/origin/commit/4c4e2408515ff158c99572ff6b6ed7295052e9a4))
* **ci:** quote rust job if-expression to fix YAML parse ([553eed7](https://github.com/7xuanlu/origin/commit/553eed770a78d5733f57155bab8434e12f100308))
* **mcp:** suppress deprecated field warnings for include_goals + goals ([4dad838](https://github.com/7xuanlu/origin/commit/4dad83865732b069d853ca698a18504dd93933ef))
* replace placeholder skills with locked verb set (init/brief/capture/recall/distill/review/forget/handoff) ([196dc75](https://github.com/7xuanlu/origin/commit/196dc7594b19f9d1e3205df698ccbd3bd9d8929a))

## [0.3.1](https://github.com/7xuanlu/origin/compare/v0.3.0...v0.3.1) (2026-05-07)


### Bug Fixes

* /api/llm/test endpoint + app proxy (Phase 5 PR3 — recreated) ([#60](https://github.com/7xuanlu/origin/issues/60)) ([64805ee](https://github.com/7xuanlu/origin/commit/64805eedb1025177e0e43f095d67c854a0039b83))
* origin-cli crate with subcommands (Phase 3 PR2) ([#54](https://github.com/7xuanlu/origin/issues/54)) ([3c9a60f](https://github.com/7xuanlu/origin/commit/3c9a60f9efba6ab5f8b0355f6625a290948ecf09))
* Phase 5-D PR1 foundation — wire types + config fields + system_info inline ([#61](https://github.com/7xuanlu/origin/issues/61)) ([734920f](https://github.com/7xuanlu/origin/commit/734920f4a72a9548ca0db778d59abca770c2ad32))
* Phase 5-D PR2 — drop origin-core dep from app crate ([#62](https://github.com/7xuanlu/origin/issues/62)) ([7ffda88](https://github.com/7xuanlu/origin/commit/7ffda88fdb3ac03717590632f3961614c801ef5d))

## [0.3.0](https://github.com/7xuanlu/origin/compare/v0.2.1...v0.3.0) (2026-05-05)


### Features

* rename Concept → Page + expand MemoryType taxonomy ([4b91089](https://github.com/7xuanlu/origin/commit/4b91089f305d7e3f43ef49a4a0b8ddd44c4e8ab1))


### Bug Fixes

* add branded Origin setup flow ([dd4208a](https://github.com/7xuanlu/origin/commit/dd4208a85996d6487045463161aa9466e1c39e45))
* **updater:** avoid temp LaunchAgent paths ([#48](https://github.com/7xuanlu/origin/issues/48)) ([14af3e4](https://github.com/7xuanlu/origin/commit/14af3e4321948952376da767830c2bf4afca3041))

## [0.2.1](https://github.com/7xuanlu/origin/compare/v0.2.0...v0.2.1) (2026-05-03)


### Bug Fixes

* **updater:** emit release updater metadata ([#43](https://github.com/7xuanlu/origin/issues/43)) ([ceedde4](https://github.com/7xuanlu/origin/commit/ceedde4a82979838b0f56e586e77722e0c0b16f0))

## [0.2.0](https://github.com/7xuanlu/origin/compare/v0.1.4...v0.2.0) (2026-05-03)


### Features

* Tauri auto-updater + pnpm update-all + UX polish ([#30](https://github.com/7xuanlu/origin/issues/30)) ([d898d6b](https://github.com/7xuanlu/origin/commit/d898d6b901ee85c2adbb7d31dbc4624dd2f016a2))
* tray + lifecycle decoupling (LSUIElement + launchd) ([#39](https://github.com/7xuanlu/origin/issues/39)) ([b185967](https://github.com/7xuanlu/origin/commit/b185967cf17e1b1f49824aa93bc358f528616c03))


### Bug Fixes

* **ci:** use RELEASE_TOKEN for version-sync push in release-please ([#24](https://github.com/7xuanlu/origin/issues/24)) ([585b7b0](https://github.com/7xuanlu/origin/commit/585b7b02549ce4b3aeaf9f30bb62d0e3d4f72dd0))
* **db:** insert_concept dual-writes concept_sources at creation ([#37](https://github.com/7xuanlu/origin/issues/37)) ([239fe35](https://github.com/7xuanlu/origin/commit/239fe35bbc96782cbff3eb79d1cf28f529030111))
* **db:** migration 44 backfills concept_sources from source_memory_ids JSON ([#36](https://github.com/7xuanlu/origin/issues/36)) ([7791811](https://github.com/7xuanlu/origin/commit/77918111791f124569e0d11b974e209dd001ed20))
* eval module restructure + Batch API integration ([#27](https://github.com/7xuanlu/origin/issues/27)) ([2ca12c1](https://github.com/7xuanlu/origin/commit/2ca12c1d10a2880bcf8056a7afd3d9b8fcefdc6e))
* **eval:** default enrichment to on-device, drop dead token_efficiency.rs ([b23f0bd](https://github.com/7xuanlu/origin/commit/b23f0bda7fbebeffde5b4a49af92e0a30ba4cd17))
* **eval:** EVAL_BASELINES_DIR env var for worktree-agnostic cache sharing ([#33](https://github.com/7xuanlu/origin/issues/33)) ([f855932](https://github.com/7xuanlu/origin/commit/f855932a761858372d7242fa4007833173efadbc))
* **eval:** per-scenario DBs in full-pipeline eval (LoCoMo + LME) ([#32](https://github.com/7xuanlu/origin/issues/32)) ([2566c61](https://github.com/7xuanlu/origin/commit/2566c612e5eae5c3dbcd491c3ac9b44dd543b57f))
* full-pipeline eval + source overlap concept gate + 3 production bugs ([#29](https://github.com/7xuanlu/origin/issues/29)) ([e8923b7](https://github.com/7xuanlu/origin/commit/e8923b720750de434e257378f829ddcc0d16bf79))
* **hooks:** pre-push skips clippy + tests + coverage on docs-only changes ([#26](https://github.com/7xuanlu/origin/issues/26)) ([e3c0124](https://github.com/7xuanlu/origin/commit/e3c0124831ffd446121c942bb37dc40a275cc480))
* **updater:** in-app toast overlay + Settings version footer ([#40](https://github.com/7xuanlu/origin/issues/40)) ([b51d244](https://github.com/7xuanlu/origin/commit/b51d24477b5668f615c849149c4a663d5e4ffb20))

## [0.1.4](https://github.com/7xuanlu/origin/compare/v0.1.3...v0.1.4) (2026-04-26)


### Bug Fixes

* **ci:** skip CI on release-please merge commits ([7c74be7](https://github.com/7xuanlu/origin/commit/7c74be78ddbe56676a71eb0d4052f0234a8a1c84))
* **distill:** prevent generic-title and runaway-cluster concepts ([#23](https://github.com/7xuanlu/origin/issues/23)) ([c3ff292](https://github.com/7xuanlu/origin/commit/c3ff292859d25a7d877afade9be322128cf2d04d))
* enrichment status honesty -- per-step tracking + self-healing ([#9](https://github.com/7xuanlu/origin/issues/9)) ([1f18813](https://github.com/7xuanlu/origin/commit/1f1881392c08018e7c99579b1b7bbd8d4411894d))
* **hooks:** run targeted clippy in pre-commit, not just cargo check ([a99681c](https://github.com/7xuanlu/origin/commit/a99681c753a4f68ba3cb5785d50d2923f1b2c694))
* **quality-gate:** fail closed when embedding fails, not open ([8661a80](https://github.com/7xuanlu/origin/commit/8661a803cf0c4f269f1fe2366b961411ac088f42))
* remove useless format\! in refinery.rs ([4ae9195](https://github.com/7xuanlu/origin/commit/4ae9195a1f0b24d31317617230813a944f55c6a0))
* self-healing title re-enrichment for truncated titles ([#22](https://github.com/7xuanlu/origin/issues/22)) ([28b731c](https://github.com/7xuanlu/origin/commit/28b731cbbba0702cf9c55dc2caf562ea8deb6823))

## [0.1.3](https://github.com/7xuanlu/origin/compare/v0.1.2...v0.1.3) (2026-04-25)


### Bug Fixes

* **ci:** add workflow_dispatch to release.yml for manual re-trigger ([85c2842](https://github.com/7xuanlu/origin/commit/85c28420587076da33786fdbf2061abe51b0251c))
* **ci:** drop Origin prefix from release name, remove dead config ([aa25245](https://github.com/7xuanlu/origin/commit/aa25245dbe561df2d799f46664e277b4c4c3b953))
* **ci:** single release per version, consistent titles, changelog in body ([d30930b](https://github.com/7xuanlu/origin/commit/d30930b1ff98024737ca837725a38a481bcea028))
* **ci:** use env context for secrets check in workflow_dispatch ([cb84b95](https://github.com/7xuanlu/origin/commit/cb84b954a14af74b0f9184ebcf80bde3fb45c024))
* **ci:** use PAT in release-please so tag push triggers release build ([928ce65](https://github.com/7xuanlu/origin/commit/928ce6508b57e8257e05d41ba5c413280b7872b1))
* **eval:** token efficiency evaluation framework ([#3](https://github.com/7xuanlu/origin/issues/3)) ([311ceea](https://github.com/7xuanlu/origin/commit/311ceea4543f5c02864e03d9fe7d57fa3197ca61))

## [0.1.2](https://github.com/7xuanlu/origin/compare/v0.1.1...v0.1.2) (2026-04-24)


### Bug Fixes

* **app:** actually add fixture-gen feature gate ([aff3ffb](https://github.com/7xuanlu/origin/commit/aff3ffb75639ce70f8a788937a9c9c3d3900264a))
* **app:** gate fixture_gen dev binary behind opt-in feature ([ffa992e](https://github.com/7xuanlu/origin/commit/ffa992ee19b61115cf08628a01d6fe3bde9f16a8))
* **app:** spawn origin-server sidecar by bare name ([6e7f15d](https://github.com/7xuanlu/origin/commit/6e7f15de5c852cf4593fc920ab303d44970b91cc))
* **app:** tee logs to ~/Library/Logs so sidecar errors are visible ([045ebb8](https://github.com/7xuanlu/origin/commit/045ebb82963bff4d331f5df2f4e7ec177421486d))
* auto-format on commit and auto-activate git hooks ([57f6170](https://github.com/7xuanlu/origin/commit/57f617034c753792abe8105ce1559bb78b3a8daf))
* bump version to 0.1.2 ([33df942](https://github.com/7xuanlu/origin/commit/33df9420e72348b2c0a232257f9d449af3ca5950))
* cache FastEmbed ONNX model in CI to prevent flaky test failures ([003299d](https://github.com/7xuanlu/origin/commit/003299d5e04a68aac7f64249d9b60f840478ea16))
* cargo fmt on db.rs test formatting ([b6a6f32](https://github.com/7xuanlu/origin/commit/b6a6f32349aee720be5ede7f1719cf46c441a7bb))
* filter superseded source memories in concept re-distill ([30c90e5](https://github.com/7xuanlu/origin/commit/30c90e58fbd3850f01ce9acf0580e1abeabf4624))
* force next release-please version to 0.1.2 via release-as ([da8b62a](https://github.com/7xuanlu/origin/commit/da8b62a88a62c18d5da6668d67780cea573c8c74))
* force v0.1.2 release-as, document feat: bumps minor pre-1.0 ([7ca2c63](https://github.com/7xuanlu/origin/commit/7ca2c636beaadadad356221b3c841978ad0b4588))
* make feat: bump patch (not minor) while pre-1.0 ([52b147e](https://github.com/7xuanlu/origin/commit/52b147ec34d6b9cd7bf6d8cb284ffa2c5bc7e664))
* **quality-gate:** require 20+ token chars for bearer credential match ([a606636](https://github.com/7xuanlu/origin/commit/a6066360c384430565340a4a1c76411b45a8fd76))
* **quality-gate:** require non-alpha char in bearer token match ([0c3e9a6](https://github.com/7xuanlu/origin/commit/0c3e9a654b61bb0bb41adbb5ab7c8788eb126d0c))
* remove empty APPLE_ID/PASSWORD/TEAM_ID from tauri-action env ([3bc4a9a](https://github.com/7xuanlu/origin/commit/3bc4a9a76fcd7126138db617ece98925ec859d0d))
* skip crates.io publish when CARGO_REGISTRY_TOKEN not set ([1bb6ccc](https://github.com/7xuanlu/origin/commit/1bb6cccb7c4d170d590d49d92096dd39b757bacd))
* vector search for concepts (hybrid vector + FTS + RRF) ([#8](https://github.com/7xuanlu/origin/issues/8)) ([74c8287](https://github.com/7xuanlu/origin/commit/74c828776ba3d547195436328d07b41e1e25abcf))
* **workspace:** move fixture_gen to origin-core so Tauri doesn't bundle it ([8f076a7](https://github.com/7xuanlu/origin/commit/8f076a71846777869b5b10f45b7842d23f3fe397))

## [0.2.0](https://github.com/7xuanlu/origin/compare/v0.1.0...v0.2.0) (2026-04-23)


### Features

* automated release pipeline with release-please ([c9395ac](https://github.com/7xuanlu/origin/commit/c9395ac91601de680766eb13c2c9a89603fb5f45))
* code signing and notarization infrastructure ([f5614e8](https://github.com/7xuanlu/origin/commit/f5614e830f6338b8e2d76a41b5072030a72f24f9))
* **kg:** alias resolution and relation vocabulary query methods ([e5e5913](https://github.com/7xuanlu/origin/commit/e5e59138fcee0310ad806784748f3f20fe3fa727))
* **kg:** alias-based 4-step entity resolution ([69e08de](https://github.com/7xuanlu/origin/commit/69e08def7f55baeb51645cadbe89385b5a2a96ab))
* **kg:** migration 40 - alias table, relation vocabulary, dedup ([cd327ae](https://github.com/7xuanlu/origin/commit/cd327aeda28760bd0dcac216b8a77d174f1f7715))
* **kg:** migration 40 - alias table, relation vocabulary, dedup ([5ef6db4](https://github.com/7xuanlu/origin/commit/5ef6db4ac7e6a24b997d53e8e3bb869443dd5c38))
* **kg:** periodic rethink pass + integration test ([b73a498](https://github.com/7xuanlu/origin/commit/b73a4985bc1e6131ee56cf5b41713eda8dd86d94))
* **kg:** post-store verification checks for entities, concepts, relations ([d76a533](https://github.com/7xuanlu/origin/commit/d76a5337a136d369cfabe7eebd3479a5c859101a))
* **kg:** relation type normalization at ingest, source_memory_id tracking ([34c76cd](https://github.com/7xuanlu/origin/commit/34c76cd7d61f31ce8b2d09c47dc3207554d7941e))
* **kg:** self-healing entity backfill phase in refinery ([298a8d9](https://github.com/7xuanlu/origin/commit/298a8d968f07a21536106e3ae4a5a5a480e58a66))
* **kg:** structured extraction prompt with vocabulary and confidence ([2342841](https://github.com/7xuanlu/origin/commit/23428412324275ecb8f64141ab4984ad8ed271b3))
* knowledge graph quality - extraction, aliases, verification, rethink ([1beb6a3](https://github.com/7xuanlu/origin/commit/1beb6a3d5e3078be7d043a91768bdee7c01ef848))
* knowledge graph quality + chat template fix ([#5](https://github.com/7xuanlu/origin/issues/5)) ([1beb6a3](https://github.com/7xuanlu/origin/commit/1beb6a3d5e3078be7d043a91768bdee7c01ef848))
* topic-key upsert + concept source linking ([#4](https://github.com/7xuanlu/origin/issues/4)) ([84874c1](https://github.com/7xuanlu/origin/commit/84874c1b96644eec7366d934188c52771ac0b5f9))


### Bug Fixes

* apply Qwen chat template in OnDeviceProvider (entities never extracted via API) ([c8a3f84](https://github.com/7xuanlu/origin/commit/c8a3f84d354410ed39aa96022330746d29dbfd2f))
* filter concepts by domain in list endpoint ([ae06a76](https://github.com/7xuanlu/origin/commit/ae06a7686eb50c2d5e4f640392055a6c63a4da11))
* **kg:** critical review fixes - upsert, case-insensitive resolution, idempotent migration ([c42395d](https://github.com/7xuanlu/origin/commit/c42395dd6aa3e656f07b323e2ca841b0502d9523))
* **kg:** rename migration 40 refs to 41 + prevent orphaned aliases ([3d8ecaf](https://github.com/7xuanlu/origin/commit/3d8ecaf817a5185dac0392f8227d562584393e10))
* remove Cargo.toml from release-please extra-files ([cb054a0](https://github.com/7xuanlu/origin/commit/cb054a0fac7f92996dbb549b1a69c706ac3299bd))
* switch release-please to simple type with version markers ([ba46e0b](https://github.com/7xuanlu/origin/commit/ba46e0b0f96f4401a03b1ed4201737913d252de9))
* use node release-type for cargo workspace compatibility ([480c545](https://github.com/7xuanlu/origin/commit/480c545d9a21d34c52d66cba91dfa276d1756c25))
