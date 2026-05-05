---
name: spreadsheet-csv-task
description: Use for CSV, spreadsheet-like tables, imports, exports, data cleanup, or report generation.
recommended_skills:
recommended_guidelines:
  - data-analysis-task
  - automation-scripting-task
---

## Overview

Spreadsheet work requires data integrity checks and reproducible steps.

## Workflow

1. Inspect the data: headers, row count, data types, null values before manipulating.
2. Keep raw data separate from analysis — never modify the source file directly.
3. Document all formulas, assumptions, and filters used.
4. Validate key totals and cross-check against known values.
5. For CSV transformations, prefer scripted processing over manual formula operations.
6. Export with explicit encoding (UTF-8) and separator settings.
7. Save incremental versions rather than overwriting.

## Quality Gate

Do not distribute a spreadsheet without validating totals, formulas, and expected row counts.