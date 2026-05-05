---
name: performance-task
description: Use for speed, latency, memory, CPU, startup, throughput, or scalability work.
recommended_skills:
  - optimize-for-gpu
  - systematic-debugging
recommended_guidelines:
  - general-programming
  - testing-task
---
## Overview

Performance optimization must be measurement-driven. Guessing is not acceptable.

## Workflow

1. Establish a clear baseline with a reproducible benchmark before any optimization.
2. Profile before hypothesis: use perf, flamegraphs, memory profilers, or tracing to find real bottlenecks.
3. Change one variable at a time. Re-measure after each change.
4. Document the before/after metrics and the conditions under which they were measured.
5. Consider algorithmic improvements before micro-optimizations.
6. For GPU performance, use `optimize-for-gpu` for CUDA/GPU-specific guidance.

## Quality Gate

Do not claim a performance improvement without baseline numbers and a reproducible measurement methodology.