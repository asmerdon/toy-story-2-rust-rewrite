# Agent instructions

## Scope and precedence
Applies to every agent working in this repository, regardless of provider. Read PROJECT.md ground rules and the newest HANDOFF.md entries first; preserve the clean-room boundary and user-owned runtime assets. User instructions and platform constraints take precedence. Preserve unrelated changes. Do not commit or push unless requested.

## Cost-conscious working mode
- Start with a short scope, milestones, risks and observable completion criteria. Continue authorized work without routine approval stops; ask only for missing decisions or genuine blockers.
- Work directly for small or tightly coupled changes. Delegation is optional, not a requirement for each milestone: its briefing and review cost must be justified.
- Default to one helper at a time. Parallel helpers require explicit authorization and independent scopes without competing writes.
- Choose the least expensive AVAILABLE model capable of the task. Use the capability tiers below, not hard-coded vendor names or assumed prices. If selection is unavailable, do the work directly or disclose the limitation; never claim a cheaper model was selected.

| Tier | Appropriate tasks | Limits / escalation trigger |
| --- | --- | --- |
| Fast / economical | Targeted file or symbol lookup, bounded inventories, formatting, simple documented edits, running prescribed checks | Do not assign uncertain physics, rendering, binary interpretation or architecture; escalate ambiguous evidence |
| Standard / balanced | Normal Rust implementation, focused multi-file fixes, regression tests, reproducible routine debugging | Escalate when a focused attempt reveals unclear behavior, cross-system causes or conflicting evidence |
| Advanced reasoning | Difficult collision/rendering/boss debugging, interpretation of original routines, uncertain specifications, architectural decisions | Narrow the investigation first; return implementation to standard tier once the cause and acceptance criteria are known |

Examples of vendor labels are not binding: Anthropic's Haiku/Sonnet/Opus traditionally express these broad tiers; other providers may expose fast/balanced/advanced variants or reasoning settings. Determine actual available capabilities and cost from the host configuration; model names alone do not prove suitability. Do not spend tool calls researching prices for each helper.

## Helper contract
Give each helper the objective, exact input files or excerpts, allowed write scope, constraints, expected output, test command and completion criteria. Prefer fresh bounded context over a full conversation copy. Helpers must not spawn further helpers, commit, push, change unrelated files or weaken tests to obtain a pass.
Require a short return: findings and evidence, changed files, checks and results, assumptions, blockers and next step. Record the model/tier actually used and relevant files consulted once in the milestone log, not every read operation.
After a failed approach, diagnose and change strategy; do not repeat an unchanged attempt. Escalate capability when reasoning is the bottleneck, not when data, permissions or tools are missing.

## Evidence and project continuity
- Use targeted searches and excerpts; do not reread unchanged files or dump decompiler output into context. Original binaries and decompiled sources stay private and outside tracked code.
- For behavior fixes, establish the original behavior and a reproducible failure first. Treat prior agents' claims as evidence to verify, not authority.
- Run the smallest meaningful check, then the affected integration checks. The main agent reviews helper diffs and independently verifies consequential results; avoid rerunning expensive identical suites without a reason.
- Simulation tests do not constitute playtesting. Inspect rendered screenshots before claiming visual correctness. Record actual commands, results, failures and untested behavior; never count skipped game-data tests as coverage.
- Preserve Level 01's user-approved behavior; use its attic see-saw as a regression reference.
- HANDOFF.md is the resume record: objective, milestone status, decisions, relevant files, checks, failed approaches and next actions. MODLOG.md records changes. Keep blockers and test coverage in one linked record instead of duplicating multiple status files.
- Update records at milestones and before stopping. Use automatic context compaction with these records as the recovery point.
- Final reports state changes, evidence, limitations, outstanding work and where to resume. Do not claim all levels are complete merely because their scripts compile.
