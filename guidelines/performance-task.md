---
name: performance-task
description: Use for speed, latency, memory, CPU, startup, throughput, or scalability work.
recommended_skills:
  - systematic-debugging
  - verification-before-completion
---

# Performance Task Guideline

Performance work needs measurement before and after.

## Workflow

1. Define the metric, workload, environment, and acceptable threshold.
2. Measure baseline behavior before optimizing.
3. Identify the bottleneck with evidence such as profiles, timings, traces, or counters.
4. Change one meaningful variable at a time when possible.
5. Verify correctness did not regress while improving performance.
6. Report both improvement and remaining limits.

## Quality Gate

Do not claim a performance improvement without comparable before-and-after evidence.
