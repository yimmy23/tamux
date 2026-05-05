---
name: esg-reporting
description: "ESG (Environmental, Social, Governance) reporting and analytics. SASB, TCFD, GRI, and CDP frameworks. Carbon accounting, supply chain sustainability, diversity metrics, and regulatory disclosure support."
tags: [esg, sustainability, carbon-accounting, sasb, tcfd, reporting, zorai]
---
## Overview

ESG reporting covers environmental, social, and governance disclosures for investors, regulators, customers, and internal planning. Use this skill when producing structured ESG reports, carbon accounting summaries, framework mappings, or evidence-backed disclosure drafts.

## When to Use

Use this skill when:
- preparing ESG, sustainability, or climate disclosures,
- mapping company metrics to frameworks like GRI, SASB, TCFD, CSRD/ESRS,
- summarizing carbon emissions or supplier sustainability data,
- drafting board/investor-facing ESG updates,
- or identifying missing evidence in an ESG reporting process.

## Core reporting frameworks

- **GRI** — broad sustainability disclosure standard
- **SASB/ISSB** — investor-material industry disclosures
- **TCFD** — climate-related governance, strategy, risk, metrics
- **CSRD / ESRS** — EU corporate sustainability reporting
- **CDP** — climate and environmental questionnaires

Do not mix frameworks casually. State clearly which framework a deliverable targets.

## Practical workflow

1. Define reporting scope: entity, time period, subsidiaries, boundary.
2. Gather source evidence: utility bills, fuel usage, travel logs, HR stats, governance records.
3. Normalize units and methodology assumptions.
4. Calculate metrics.
5. Map each metric or statement to target framework sections.
6. Separate measured facts from narrative claims.
7. Flag data gaps and estimation methods explicitly.

## Carbon accounting structure

Typical emissions buckets:
- **Scope 1**: direct owned/controlled emissions
- **Scope 2**: purchased electricity/steam/heat/cooling
- **Scope 3**: supply chain, travel, commuting, downstream usage, etc.

## Simple emissions calculation example

```python
def calculate_emissions(electricity_kwh, gas_therms, business_travel_miles):
    # Illustrative factors only — replace with jurisdiction/year-specific factors
    scope1 = gas_therms * 0.0053
    scope2 = electricity_kwh * 0.0004
    scope3 = business_travel_miles * 0.0004
    return {
        'scope1_tco2e': round(scope1, 3),
        'scope2_tco2e': round(scope2, 3),
        'scope3_tco2e': round(scope3, 3),
        'total_tco2e': round(scope1 + scope2 + scope3, 3),
    }

print(calculate_emissions(120000, 9000, 25000))
```

## Evidence table pattern

Use a table like this before writing the final report:

```text
Metric | Value | Unit | Source | Period | Method | Confidence
------ | ----- | ---- | ------ | ------ | ------ | ----------
Scope 2 electricity | 48.0 | tCO2e | utility invoices | FY2025 | location-based factor | high
Board independence | 4/5 | directors | board register | FY2025 | direct count | high
Employee turnover | 12.4 | % | HRIS export | FY2025 | voluntary+involuntary | medium
```

## Writing rules

- Never present estimated numbers as measured numbers.
- Say which framework and year/version you are aligning to.
- Keep methodology notes near the metric.
- Separate commitments/goals from achieved results.
- If data coverage is partial, state boundary limitations explicitly.

## Common failure modes

- mixing entities or time periods
- using inconsistent emissions factors
- vague claims like “sustainable” without evidence
- reporting percentages with no denominator
- copying framework language without mapping to actual evidence
