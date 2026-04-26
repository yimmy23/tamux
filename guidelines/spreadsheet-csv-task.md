---
name: spreadsheet-csv-task
description: Use for CSV, spreadsheet-like tables, imports, exports, data cleanup, or report generation.
recommended_skills:
  - spreadsheet-modeling
  - financial-modeling
  - budgeting-planning
  - verification-before-completion
---

# Spreadsheet And CSV Task Guideline

Spreadsheet work should preserve structure and avoid silent data corruption.

## Workflow

1. Identify delimiter, encoding, headers, quoting, dates, numeric formats, and output format.
2. Preserve row counts and key columns unless the user asks to filter.
3. Validate parsing with sample rows and totals.
4. Treat IDs, ZIP codes, phone numbers, and account numbers as text unless explicitly numeric.
5. Document any normalization, deduplication, or dropped rows.
6. Verify the produced file can be read back.

## Quality Gate

Do not alter tabular data without checking row counts and representative records before and after.
