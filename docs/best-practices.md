# tamux Best Practices

This is the practical version of the docs: how to configure tamux so it is useful every day, how to assign models to the built-in fires, and how to keep long-running agent work reliable instead of expensive or vague.

For installation and first-run setup, see [getting-started.md](getting-started.md). For the full configuration surface, see [reference.md](reference.md). For thread participants and internal delegation, see [operating/thread-participants.md](operating/thread-participants.md).

## Start With The Right Mental Model

tamux works best when you treat it as a daemon-backed operating environment, not as a single chat box.

- Keep the daemon running and reconnect from whichever client is convenient.
- Use the desktop UI when you want broad visibility and settings control.
- Use the TUI when you want keyboard-first control, SSH-friendly work, or fast inspection.
- Use the CLI and MCP server when you want automation, scripting, or external-agent integration.
- Let the daemon own state. Closing the UI should not mean losing the work.

The main failure mode for new users is treating tamux like a disposable prompt window. The whole point is durable state, durable threads, durable tasks, and durable goals.

## Choose Strong Model Roles

The built-in fires are more useful when each one is configured for the kind of work it is supposed to do instead of pointing everything at the same model.

### Svarog

Svarog is the main working fire. It should use the strongest model you have access to.

Use Svarog for:

- primary execution
- long reasoning chains
- tool orchestration
- multi-step coding and debugging
- synthesis across large threads
- goal runs that need planning and recovery

If you have only one premium model slot, spend it on Svarog first.

### Weles

Weles is for governance, guarded review, policy pressure, and second-pass skepticism. It does not need to be your single strongest model, but it should still be good enough to make strong review judgments.

Recommended baseline:

- a model in the `GLM-5.*` class
- `MiniMax M2.7`
- `Qwen 3.6 Plus`
- anything clearly above that tier

Use Weles for:

- governance and risk review
- checking risky plans before execution
- validating assumptions when the main thread is moving fast
- compaction when you want an LLM to compress context instead of relying on heuristic trimming

Do not starve Weles with a weak bargain model. If governance is too weak, tamux becomes noisier and less trustworthy exactly where it should be more disciplined.

### Rarog

Rarog should be light, cheap, and responsive, with solid tool-calling behavior.

Good Rarog characteristics:

- low cost
- fast latency
- reliable tool invocation
- acceptable instruction following without heavy reasoning

Rarog has reasoning turned off by default, so it can be a non-reasoning model. That is fine. If your provider has a better result with low or medium reasoning, you can change it, but the default role for Rarog is lightweight operator assistance rather than expensive deep analysis.

Use Rarog for:

- onboarding and setup help
- operator-facing summaries
- lightweight steering
- quick checks that do not deserve your strongest model
- conversational glue around the heavier fires

## Do Not Configure Every Fire The Same Way

A common bad setup is:

- strong model everywhere
- high reasoning everywhere
- expensive compaction
- no distinction between worker, reviewer, and concierge roles

That setup burns money and muddies responsibilities.

A better default is:

- Svarog: strongest available model
- Weles: strong review/governance model
- Rarog: cheap and quick model with strong tool calling
- other fires: match them to their job instead of copying Svarog blindly

If a model assignment has no role logic behind it, it is usually the wrong assignment.

## Use LLM-Based Compaction When Threads Matter

Heuristic compaction is the default. It is fine as a conservative baseline, and it is cheaper. But if you are using tamux for long-lived threads, serious coding work, layered investigations, or governance-heavy runs, you should usually move to LLM-based compaction.

Recommended practice:

- use `WELES` compaction when you want better continuity across long threads
- use a custom LLM compaction model only if you have a specific reason to separate compaction from Weles
- leave heuristic compaction as the fallback, not the aspirational setup

Where to change it:

- in the desktop app: `Settings` -> `Agent` -> `Context Compaction`
- in the TUI: `/settings` -> `Advanced`

Set the strategy to `WELES` when you want LLM-backed summarization. The default is `Heuristic`.

LLM-based compaction is especially useful when:

- requirements evolve over many turns
- the thread includes important tradeoffs, constraints, or prior failures
- the agent needs to preserve intent, not just recent text
- you expect the thread to survive over time rather than finish in one burst

Heuristic compaction is still reasonable when:

- the task is short
- the context is mostly recent and disposable
- cost matters more than continuity
- you are using tamux for quick utility work

## Prefer Durable Threads Over Fresh Chats

tamux gets stronger when a thread represents a real line of work.

Good thread habits:

- keep one thread per real objective
- continue the thread when the work is still the same mission
- start a new thread when the objective changes materially
- let context accumulate where continuity is useful
- avoid throwing away history just because a model had one bad turn

Resetting too often destroys one of tamux's biggest advantages.

## Be Explicit With Objectives

The agent does better when you state:

- the goal
- the constraints
- the success condition
- the preferred verification path

Weak prompt:

```text
check this repo
```

Better prompt:

```text
Find why the release build is failing on Linux, identify the first concrete root cause, make the smallest safe fix, and tell me what you verified locally.
```

tamux rewards operationally specific instructions.

## Use The Right Interaction Mode

tamux gives you different ways to involve other fires. Use them intentionally.

Use `@agent ...` when:

- you want a visible participant watching a thread
- you want specialist input without changing the main owner
- you want that agent to jump in only when it has something useful

Use `!agent ...` when:

- you want hidden internal delegation
- you want a second opinion without cluttering the visible thread
- you want behind-the-scenes comparison or verification

Use handoff when:

- another agent should own future replies
- the nature of the work really changed
- you want a specialist to take the lead instead of just advising

How to ask for it:

- ask directly in the thread for another fire to take over
- make the ownership change explicit, not implied
- say who should take over and why

Good handoff requests:

- `Handoff this thread to Weles for governance review.`
- `Let Rarog take over this thread and keep the guidance lightweight.`
- `Switch this thread to Swarozyc and continue the implementation there.`

Handoff behavior:

- a handoff changes the active responder for the thread
- future operator messages route to that agent until a return handoff
- tamux records the switch as a visible system event while keeping linked handoff context hidden

Use handoff instead of `!agent` when you want to talk to that agent directly going forward. Use handoff instead of `@agent` when advisory participation is not enough and ownership should actually move.

Good examples:

- `@weles verify claims before answering`
- `!weles assess the risk of this migration plan`
- `@rarog keep the operator summary concise and practical`

## Use Weles For Friction, Not For Decoration

Weles should introduce useful friction at the points where the system could become reckless.

Ask Weles to look at:

- risky shell or file mutations
- security-sensitive changes
- migrations and deletions
- policy-heavy workflows
- claims that sound too confident
- plans with large blast radius

Bad Weles usage is adding it everywhere with no purpose. Good Weles usage is applying it exactly where bad decisions would be expensive.

## Run Goal Work As Goal Work

If the task is long-running, multi-step, or likely to survive UI restarts, use goal runners instead of trying to manually babysit a chat thread.

Good goal-run candidates:

- release preparation
- large bug investigations
- repository-wide audits
- multi-step refactors
- documentation sweeps
- repeated operational workflows

Goal runs are the right layer when you want planning, task queues, approvals, persistence, and reflection instead of a one-shot answer.

See [goal-runners.md](goal-runners.md) for the goal-runner model.

## Keep Governance And Cost In Balance

You do not need maximum quality everywhere. You do need quality where errors are expensive.

A useful budget strategy:

- spend most on Svarog
- spend enough on Weles to keep reviews meaningful
- save money with Rarog and lighter support roles
- use heuristic compaction for throwaway threads
- use WELES compaction for serious or durable threads

If cost is exploding, do not immediately weaken Svarog. First check:

- whether too many fires are configured with premium models
- whether compaction is overpowered for the type of thread
- whether you are creating too many fresh threads and losing continuity
- whether tool loops or retries are set too aggressively

## Use Approvals To Protect The Edges

tamux is strongest when autonomy and operator control are both real.

Practical approval habits:

- allow routine low-risk work to move
- stop and inspect risky mutations
- look carefully at destructive commands
- review surprising network access or environment changes
- treat policy escalations as useful signals, not as annoying interruptions

The point is not to slow the agent down. The point is to make expensive mistakes rarer.

## Keep Context Clean

Long threads stay usable when you keep the signal high.

Do this:

- restate key constraints when they change
- correct wrong assumptions explicitly
- keep goals concrete
- prefer one thread per mission
- use compaction strategically

Avoid this:

- mixing unrelated objectives in one thread
- giving vague directives and then expecting perfect execution
- silently changing requirements midstream
- using a lightweight model for work that obviously needs depth

## Match Client To Situation

Use the client that gives the right control surface for the task.

- desktop app: best for settings, visibility, chat, and broad operational control
- TUI: best for keyboard-first work, remote sessions, and daemon inspection in the terminal
- CLI: best for automation and scripted inspection
- MCP: best when another agent or toolchain needs to drive tamux as a system component

There is no single correct frontend. The daemon is the product; clients are views into it.

## Prefer Real Histories Over Re-Explaining Everything

When tamux already has the thread, task, or goal history, use that continuity instead of restating the whole situation from zero every time. This is one of the main reasons to use tamux in the first place.

That means:

- keep important work in durable threads
- revisit existing threads when the work continues
- use participants and delegation to extend a thread rather than fork chaos around it
- let compaction preserve continuity instead of discarding it

## Suggested Baseline Configuration

If you want a simple starting point:

- Svarog: strongest model you can access
- Weles: `GLM-5.*`, `MiniMax M2.7`, `Qwen 3.6 Plus`, or stronger
- Rarog: cheap fast tool-capable model, reasoning off by default
- Compaction: `WELES` for important threads, `Heuristic` for cheap short work
- Goal runners: use them for anything you would otherwise “come back to later”
- Participants: add `@weles` when correctness and risk matter; add `@rarog` when operator guidance matters

That baseline is usually enough to get tamux feeling substantially better than a generic single-model terminal chat workflow.

## Related Reading

- [Getting Started](getting-started.md)
- [Reference](reference.md)
- [How tamux Works](how-tamux-works.md)
- [Goal Runners](goal-runners.md)
- [Thread Participants](operating/thread-participants.md)
