---
name: data-analysis-task
description: Use for analyzing datasets, logs, metrics, survey output, JSON, CSV, or tabular information.
recommended_skills:
  - product-analytics
  - analytics-tracking
  - support-analytics
  - spreadsheet-modeling
  - systematic-debugging
  - verification-before-completion
---

# Data Analysis Task Guideline

Data analysis should be reproducible and honest about uncertainty.

## Workflow

1. Identify the question, dataset, fields, time range, and expected output.
2. Inspect schema, size, missing values, units, and obvious quality issues.
3. Use structured parsers for structured data instead of ad hoc text slicing.
4. Keep transformations explicit and repeatable.
5. Separate descriptive results from causal claims or recommendations.
6. Include sanity checks, totals, or sample rows that validate the result.

## Quality Gate

Do not summarize data without checking whether parsing, filtering, or missing values could change the conclusion.
