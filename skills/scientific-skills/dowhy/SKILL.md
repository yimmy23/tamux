---
name: dowhy
description: "DoWhy (Microsoft) — causal inference library. Causal graph modeling, identification (back-door, front-door, IV), estimation (matching, IPW, double-ML), and refutation/robustness checks for causal claims."
tags: [dowhy, causal-inference, causal-graph, identification, estimation, microsoft, zorai]
---
## Overview

DoWhy (Microsoft/py-why) provides end-to-end causal inference: causal graph modeling (DAG), identification strategies (back-door, front-door, instrumental variables), estimation (linear regression, matching, IV, double-ML), and refutation tests (placebo, bootstrap, random common cause, data subset).

## Installation

```bash
uv pip install dowhy
```

## Full Workflow

```python
from dowhy import CausalModel

model = CausalModel(
    data=df,
    treatment="treatment",
    outcome="outcome",
    common_causes=["age", "gender", "income"],
)

# 1. Identify
identified = model.identify_effect(proceed_when_unidentifiable=True)

# 2. Estimate
estimate = model.estimate_effect(identified, method_name="backdoor.linear_regression")
print(f"ATE: {estimate.value:.4f} (p={estimate.p_value:.4f})")

# 3. Refute
refute = model.refute_estimate(identified, estimate, method_name="placebo_treatment_refuter")
print(f"Refutation passed: {refute.refutation_result}")
```

## References
- [DoWhy docs](https://www.pywhy.org/dowhy/)
- [DoWhy GitHub](https://github.com/py-why/dowhy)