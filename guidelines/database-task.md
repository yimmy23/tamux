---
name: database-task
description: Use for schema changes, migrations, queries, persistence bugs, data repair, or database performance.
recommended_skills:
recommended_guidelines:
  - general-programming
  - coding-task
---
## Overview

Database work requires understanding the schema, query patterns, and data volume before making changes.

## Workflow

1. Understand the schema, relationships, and constraints before writing queries or migrations.
2. For migrations: plan forward and rollback paths. Test on a copy before production.
3. Write queries with explicit column lists instead of SELECT * — it's faster and more maintainable.
4. Check query plans for performance — use EXPLAIN ANALYZE on any query touching significant data.
5. Consider indexing: query WHERE clauses and JOIN conditions are the primary candidates.
6. For bulk operations, batch in transactions of manageable size. Consider row locks.

## Quality Gate

Do not run destructive queries or migrations without a verified rollback plan.