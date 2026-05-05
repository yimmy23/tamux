---
name: investor-research
description: When the user wants to identify, evaluate, or prioritize potential investors for a fundraising round. Also activates when the user asks "who should I pitch?", "find me investors", "build an investor list", or mentions VC/angel targeting.
related: [pitch-deck, fundraising-email]
reads: [startup-context]

tags: [nontechnical, startup-founder-skills, investor-research]
---|---------|-------------|------------|------------|-------------|-----------|-----------|-------|
```

Followed by a "Conflicts" section listing excluded firms and why.

Followed by a "Research Gaps" section listing anything that could not be verified and needs the founder's input.

## Frameworks & Best Practices

### Investor Qualification Criteria (The 7-Point Filter)

1. **Stage fit** — Does the firm invest at the founder's current stage? A Series B fund will not lead a seed round. This is the first filter and it is binary: pass or fail.
2. **Sector focus** — Does the firm have a stated thesis or track record in the founder's sector? Look at their last 10 investments, not just their website copy.
3. **Check size match** — Does the firm's typical check size align with what the founder needs? A $2B fund rarely writes $500K checks. A $50M fund rarely leads $20M rounds.
4. **Portfolio conflicts** — Does the firm already have a company in the same space? This is the most common reason pitches are dead-on-arrival. Check every portfolio company, including quiet ones.
5. **Fund vintage** — Is the firm actively deploying from a recent fund? A fund raised 4+ years ago is likely in harvest mode and not writing new checks. Prefer firms that closed a fund within the last 18 months.
6. **Geographic relevance** — Some firms only invest locally. Others require board seats that demand proximity. Remote-friendly firms have expanded, but geography still matters for many funds.
7. **Partner-level interest** — Is there a specific partner whose background, interests, or public writing aligns with the startup? Pitching the right partner at the right firm matters as much as pitching the right firm.

### Tiering Framework

- **Tier 1**: Matches on 6-7 of the criteria above. The firm has invested in adjacent companies, the partner has spoken publicly about the space, and a warm intro path exists. Pursue first.
- **Tier 2**: Matches on 4-5 criteria. Good fit on stage and sector but may lack a warm path or have a slightly mismatched check size. Pursue in the second wave.
- **Tier 3**: Matches on 3 criteria. Acceptable as backfill if the round needs more participants. Do not spend significant time here until Tier 1 and 2 are exhausted.

### Sourcing Investor Information

- **Crunchbase / PitchBook**: Fund size, recent investments, portfolio companies.
- **Firm website**: Stated thesis, partner bios, blog posts that reveal focus areas.
- **Twitter/X and Substack**: Many partners publish their current interests publicly. Recent posts are a better signal than old "About" pages.
- **SEC filings**: Fund size from Form D filings when not publicly disclosed.
- **Portfolio founder back-channels**: The single best diligence on an investor is talking to founders they have backed — both successes and companies that struggled.

### Common Mistakes to Avoid

- **Spraying 200 cold emails** — Fundraising is a funnel. 30 well-targeted, well-introduced conversations beat 200 cold ones.
- **Ignoring portfolio conflicts** — Founders waste weeks pitching firms that will never invest because of a conflict.
- **Pitching the wrong partner** — At multi-partner firms, the wrong partner will say "interesting, let me introduce you to my colleague" at best, or just pass.
- **Targeting only brand-name firms** — Tier 2 and emerging funds are often faster to decide, more founder-friendly, and more willing to lead at earlier stages.
- **Not tracking your pipeline** — Use a simple spreadsheet or CRM: investor name, status (researching / intro requested / meeting scheduled / pitched / passed / term sheet), and next action.

### Angel Investor Considerations

- Angels decide faster (days, not weeks) but write smaller checks ($25K-$250K typically).
- Look for angels with operational experience in your sector — they add value beyond capital.
- Angel syndicates (AngelList, etc.) can aggregate small checks into a meaningful allocation.
- Be cautious about taking angel money from potential acquirers or competitors without understanding the signaling implications.

## Related Skills

- `pitch-deck` — tailor the deck narrative based on what specific investors care about
- `fundraising-email` — write targeted outreach once the investor list is built

## Examples

**Example prompt**: "We're raising a $2.5M seed round for a developer tools company based in SF. Help me build an investor list."

**Good output snippet** (one Tier 1 entry):

> | Boldstart Ventures | Ed Sim | Pre-seed/Seed | Developer tools, infrastructure | $1-3M | $160M Fund IV (2023) | None | Ed is active on Twitter re: dev tools; check if any portfolio founders overlap with your network | Led seed in [similar company]; blog post on "Why developer experience is the next platform shift" |

**Example prompt**: "I have a list of 15 VCs I want to pitch. Can you help me prioritize?"

**Good output approach**: Run each firm through the 7-point filter against the founder's startup context. Re-tier the list. Flag any portfolio conflicts the founder may have missed. Identify the 5 to pitch first and suggest the outreach sequence.
