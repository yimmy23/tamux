---
name: analytics-measurement-task
description: Use for tracking plans, metrics definitions, dashboards, attribution, product analytics, support analytics, SaaS metrics, or measurement audits.
recommended_skills:
  - analytics-tracking
  - product-analytics
  - support-analytics
  - saas-metrics
  - spreadsheet-modeling
  - financial-reporting
---

# Analytics And Measurement Task Guideline

Analytics work should answer decisions, not create unused data.

## Workflow

1. Identify the decision, audience, metric owner, time period, and action that will follow from the data.
2. Define metric formulas, event names, properties, dimensions, exclusions, and source of truth.
3. Separate acquisition, activation, retention, revenue, support, and financial metrics.
4. Use `analytics-tracking` for event plans, `product-analytics` for product funnels, `support-analytics` for support operations, and `saas-metrics` for SaaS KPI definitions.
5. Check data quality: missing events, duplicate events, timezone, identity stitching, sampling, and backfills.
6. Present findings with definitions, caveats, and next instrumentation or analysis steps.

## Quality Gate

Do not present a dashboard or metric without defining how it is calculated and what decision it supports.
