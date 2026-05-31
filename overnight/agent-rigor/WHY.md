# Why this exists

The short version: I spent about six weeks building an AI memory tool. The most valuable thing I made was not the tool. It was the system that kept the coding agent honest while I built it. This repo is that system.

## The honest origin story

The product (a local-first memory daemon, Rust, a real eval harness) is good. Its traction is near zero. The category it competes in is crowded and has a funded, distributed incumbent. Polishing the product more would have produced a more beautiful repo and the same zero traction.

But buried in those six weeks and ~314 commits, roughly a third of the work went somewhere unusual: into the *discipline*. An `AGENTS.md` hierarchy that every coding agent reads. An eight-layer model for deciding where each test runs. A citation standard that refuses to quote a single-run benchmark number externally. Git hooks that auto-format and gate before code can rot. Boundary checks that keep the core logic framework-free. A worktree-cleanup playbook for the squash-merge SHA trap.

That is not memory-product overhead. That is a complete, written, battle-tested method for keeping an AI agent productive and honest, at exactly the moment the industry started calling this "eval-driven development" and naming quality as the number-one blocker to shipping agents.

## The reputation play this ties to

There is a companion move to productizing this method: publishing the rigor itself. Not as a sales funnel for a product, but as the work of someone who is demonstrably careful with numbers in a field full of careless ones.

The whole agent-memory category quotes cherry-picked single figures and compares across incompatible versions. An independent, no-product-to-sell teardown of what those benchmarks actually measure and where they mislead would travel, precisely because the author has a documented refusal to do the dishonest thing. The citation discipline in `docs/eval-citation-discipline.md` is not just a rule set. It is a credential. It says: this person will tell you when the number is scaffold, will not compare across schema versions, will show you the per-case breakdown that hides the regression everyone else buried.

Distribution is the thing a solo builder lacks. Audience is distribution. And the fastest way to an audience, for someone whose actual strength is rigor, is to make the rigor visible and useful to other people. agent-rigor is the useful artifact. The essay is the visible part. The reputation is the compounding asset that outlasts any single product.

## What I am claiming, and what I am not

I am not claiming this is novel. The individual ideas (surgical changes, verify before done, honest metrics) are old engineering virtues. I am claiming three things:

1. These specific rules were the working operating system of a real codebase, not a wishlist. The hooks ran. The boundary checks were enforced. The single-run ban was honored.
2. They are transferable. Strip the project-specific names and the structure survives intact, which is what this template proves.
3. Most people drowning in confidently-wrong agent output have not written their version down. Here is one that worked, ready to copy.

The product was the byproduct. The scaffolding was the asset. This repo is me admitting that out loud and handing you the scaffolding.
