---
name: merger-model
description: "M&A deal modeling: accretion/dilution analysis, LBO models, DCF valuation, comparable company analysis, precedent transactions, and synergy estimation. Full three-statement merger models."
tags: [mergers, acquisitions, lbo, valuation, dcf, financial-modeling, zorai]
---
## Overview

A merger model estimates the financial impact of an acquisition or merger: purchase price, financing mix, pro forma statements, synergies, accretion/dilution, leverage, and returns. Use it for M&A analysis, not just valuation in isolation.

## When to Use

Use this skill when:
- evaluating whether a deal is accretive or dilutive,
- comparing stock vs cash vs mixed consideration,
- building a simple LBO/strategic deal screen,
- estimating synergy sensitivity,
- or drafting the structure for a full M&A model.

## Core model sections

A proper merger model usually includes:
1. transaction assumptions
2. purchase price / enterprise value
3. financing sources and uses
4. purchase accounting adjustments
5. pro forma income statement
6. share count bridge
7. accretion/dilution analysis
8. leverage and coverage metrics
9. sensitivity tables

## Basic accretion / dilution calculation

```python
def accretion_dilution(acquirer_net_income, target_net_income,
                       acquirer_shares, new_shares_issued,
                       after_tax_synergies=0.0):
    pro_forma_income = acquirer_net_income + target_net_income + after_tax_synergies
    pro_forma_shares = acquirer_shares + new_shares_issued
    base_eps = acquirer_net_income / acquirer_shares
    pro_forma_eps = pro_forma_income / pro_forma_shares
    accretion_pct = (pro_forma_eps / base_eps - 1) * 100
    return {
        'base_eps': round(base_eps, 4),
        'pro_forma_eps': round(pro_forma_eps, 4),
        'accretion_pct': round(accretion_pct, 2),
    }

print(accretion_dilution(
    acquirer_net_income=900,
    target_net_income=220,
    acquirer_shares=300,
    new_shares_issued=50,
    after_tax_synergies=40,
))
```

## Sources and uses pattern

```text
Uses:
- equity purchase price
- debt repayment / assumption
- fees and expenses

Sources:
- cash on balance sheet
- new debt
- stock issuance
```

Always reconcile sources == uses.

## LBO-style screen example

```python
def rough_lbo_irr(entry_ebitda, entry_multiple, debt_pct, ebitda_growth, exit_multiple, years):
    entry_ev = entry_ebitda * entry_multiple
    debt = entry_ev * debt_pct
    equity = entry_ev - debt
    exit_ebitda = entry_ebitda * ((1 + ebitda_growth) ** years)
    remaining_debt = debt * 0.65  # simple placeholder assumption
    exit_ev = exit_ebitda * exit_multiple
    exit_equity = exit_ev - remaining_debt
    irr = (exit_equity / equity) ** (1 / years) - 1
    return round(irr * 100, 2)
```

## Minimum real-world checks

- Is the target EV/EBITDA multiple plausible vs comps?
- Are synergies cost, revenue, or both?
- Are synergies pre-tax or after-tax?
- Is financing realistic at current rates and leverage levels?
- Does the model include fees, integration costs, and stock-based comp effects?
- Are share issuance and treasury method assumptions explicit?

## Common failure modes

- counting synergies with no timing ramp
- mixing EBITDA, EBIT, and net income carelessly
- forgetting transaction fees
- using enterprise value where equity value is needed
- assuming all synergies are immediate and fully realizable
- treating accretion as proof of strategic quality
