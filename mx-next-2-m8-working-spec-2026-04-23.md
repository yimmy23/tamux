# Working spec — Mx-next-2 / M8 trusted provenance signature validation

Date: 2026-04-23
Workspace: `/home/mkurman/gitlab/it/cmux-next`
Goal: `goal_796e374a-8bb9-4f47-9d54-596f30ce6f32`
Execution dir: `/home/mkurman/.tamux/goals/goal_796e374a-8bb9-4f47-9d54-596f30ce6f32/inventory/execution/m8-trusted-provenance-signature-validation`

## Selection basis
- Step 1 marker exists, satisfying the gating condition to proceed beyond Mx-next-1.
- Repo docs and moat audit place **M8 Trusted Provenance** immediately after M7 in the remaining moat sequence.
- The bounded candidate gap is inside the provenance-report integrity surface rather than docs or the already-shipped fallback adaptation.

## Hypothesis
The current provenance report may overstate signature validity because unsigned entries are treated as `signature_valid = true` even though they are not counted under `signed_entries`. If confirmed, the smallest safe closure is to distinguish "not signed" from "signed and valid" in the report surface and tests while preserving hash/chain validation.

## Planned contract
1. Persist moat execution artifacts early (`todo.md`, spec, normalized command/output logs).
2. Research with `alibaba-coding-plan/glm-5` via spawned subagent and combine with local source inspection.
3. Implement only the narrow daemon/runtime change needed for M8.
4. Run narrow verification for the touched provenance logic.
5. Run review with `alibaba-coding-plan/qwen3.6-plus` and code review with `github-copilot/gpt-5.3-codex`, save artifacts, iterate if needed.
6. Write moat completion marker plus goal step completion marker if the full step contract is satisfied.
