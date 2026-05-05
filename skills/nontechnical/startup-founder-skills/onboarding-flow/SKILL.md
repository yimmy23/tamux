---
name: onboarding-flow
description: When the user needs to design, improve, or audit a post-signup activation flow to get new users to their first value moment. Activate when activation is lagging, time-to-value feels excessive, or first sessions lack impact.
related: [support-docs, email-marketing, churn-analysis]
reads: [startup-context]

tags: [nontechnical, startup-founder-skills, onboarding-flow, experimental-design, compliance]
----------|------------------------|---------------------|
| B2B SaaS (simple) | Complete first core workflow | < 10 minutes |
| B2B SaaS (complex) | Import data + run first report | < 24 hours |
| PLG / Self-serve | Invite first team member + collaborate | < 48 hours |
| Developer tool | First successful API call or deploy | < 30 minutes |
| B2C app | Complete first session/transaction | < 3 minutes |

### Inside-the-Product Onboarding
Prioritize onboarding that happens within the product experience itself, not detached from it. Product tours that overlay the UI without context are less effective than:
- **Empty states that teach** — Every blank screen guides the next action
- **Inline prompts** — Contextual guidance that appears when the user reaches a decision point
- **Progressive disclosure** — Reveal complexity as the user demonstrates readiness
- **Sample data** — Let users experience the product's value before investing their own data

### Multi-Channel Coordination Rules
- **In-app:** Real-time guidance for active users. Checklists, tooltips, progress bars, empty state CTAs.
- **Email:** Async nudges for users who leave. Day 0 welcome, Day 1 reinforcement, Day 3 re-engagement, Day 7 value recap.
- **Push/SMS:** Reserve for high-intent signals only ("Your report is ready," "Your teammate just joined"). Never use for generic reminders.
- **Suppression rule:** If the user completed the action in-app, suppress the corresponding email. Nothing kills trust faster than "Complete your setup!" emails sent after setup is done.

### Checklist Design Principles
- **3-5 items maximum.** Completion rates drop sharply above 5.
- **First item should be pre-completed.** Starting with visible progress significantly increases completion rates (endowed progress effect).
- **Each item takes under 2 minutes.** If longer, break it into sub-steps.
- **Show the reward.** "Connect Slack -> Get instant notifications" not just "Connect Slack."
- **Allow skipping with consequences.** Let users skip but show what they lose.

### Re-Engagement for Stalled Users
Diagnose before prescribing. Different stall points indicate different problems:

| Stall Point | Likely Cause | Intervention |
|-------------|-------------|--------------|
| Signed up, never returned | Unclear value prop or bad timing | Email with specific use case matching signup context |
| Started setup, abandoned | Too many steps or hit a blocker | Email linking directly to where they stopped |
| Completed setup, never used | No compelling reason to return | Trigger-based email when something relevant happens |
| Used once, never returned | First experience was not valuable | Ask what they were trying to accomplish; offer guided call |

### Quality Guardrails
All onboarding designs should include:
- Risk documentation identifying assumptions that could be wrong
- Open questions that need user research to answer
- Explicit next steps with owners and timelines
- Leading indicators to measure before waiting for retention data

## Related Skills
- `support-docs` — Create help center articles and getting-started guides that support the onboarding flow
- `email-marketing` — Build the full lifecycle email program beyond onboarding (retention, expansion, win-back)
- `churn-analysis` — When onboarding completion data reveals early churn patterns needing deeper investigation

## Examples

### Example 1: Designing a new onboarding flow
**User:** "We're a B2B project management tool. Users sign up but only 20% create their first project. Help us fix onboarding."

**Good output excerpt:**
> **Activation Event:** Create first project + add at least one task (correlates with 60-day retention).
>
> **First 30 Seconds Design:**
> After signup, land on a pre-built sample project (not an empty dashboard). The user sees what the product looks like when populated. A single guided prompt says "Create your first real project" with 3 templates to choose from. After project creation, inline prompt to add a first task with an example. Celebration state: "Your project is live! Invite your team to collaborate."
>
> **Expected impact:** Reducing pre-value steps from 4 to 2 should increase project creation from 20% to 45-55%.

### Example 2: Activation is lagging
**User:** "We have 3,000 signups from last month and 60% never completed setup. How do we bring them back?"

**Good output approach:** Segment the 60% by where they stalled. Design different re-engagement emails for each stall point. Include subject lines, body copy, and CTAs linking directly to the abandoned step. Recommend a sunset policy (stop emailing after 30 days to protect deliverability). Build an experiment backlog testing different interventions at each stall point, prioritized by volume of stalled users and estimated recovery rate.
