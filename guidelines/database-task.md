---
name: database-task
description: Use for schema changes, migrations, queries, persistence bugs, data repair, or database performance.
recommended_skills:
  - systematic-debugging
  - test-driven-development
  - security-best-practices
---

# Database Task Guideline

Database work must protect data integrity first.

## Workflow

1. Identify tables, keys, constraints, ownership, and migration order.
2. Inspect existing data shape and edge cases before changing schema or queries.
3. Plan backward compatibility, rollback, and concurrent application versions where relevant.
4. Use transactions for multi-step writes.
5. Test empty, typical, duplicate, malformed, and large-data cases.
6. Verify indexes, query plans, or performance when access patterns change.

## Quality Gate

Do not modify persistent data or schema without a migration and validation path.
