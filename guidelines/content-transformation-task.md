---
name: content-transformation-task
description: Use for rewriting, summarizing, translating, extracting, formatting, or restructuring provided content.
recommended_skills:
recommended_guidelines:
  - data-analysis-task
  - automation-scripting-task
---

## Overview

Content transformation must preserve data integrity.

## Workflow

1. Understand the source schema and the target schema.
2. Map all fields explicitly, not by position.
3. Test the transformation on a small sample first.
4. Validate output: check row counts, field types, nulls, and edge cases.
5. Handle encoding and special characters explicitly.
6. Log skipped or failed records during transformation.
7. Verify a representative sample of the output against the source.

## Quality Gate

Do not declare a transformation complete without verifying at least 5% of output records or running automated validation.