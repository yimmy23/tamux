---
name: lifelines
description: "Survival analysis in Python: Kaplan-Meier, Cox proportional hazard, Aalen additive, parametric models, and competing risks. Censored data handling for churn, clinical, and actuarial applications."
tags: [survival-analysis, kaplan-meier, cox-model, actuarial, churn, statistics, zorai]
---
## Overview

Lifelines is a survival analysis library for Python. It implements Kaplan-Meier, Cox Proportional Hazard, parametric models (Weibull, Log-Normal), and Aalen's additive model. Use it for time-to-event data in clinical trials, churn analysis, reliability engineering, and customer retention studies.

## Installation

```bash
uv pip install lifelines
```

## Kaplan-Meier Estimate

```python
from lifelines import KaplanMeierFitter
import pandas as pd

T = pd.Series([5, 10, 15, 20, 25, 30])  # durations
E = pd.Series([1, 1, 0, 1, 0, 0])       # event observed?

kmf = KaplanMeierFitter()
kmf.fit(T, E)
kmf.plot_survival_function()
print(kmf.median_survival_time_)
```

## Cox Proportional Hazard

```python
from lifelines import CoxPHFitter

df = pd.DataFrame({
    "duration": [5, 10, 15, 20, 25, 30],
    "event": [1, 1, 0, 1, 0, 0],
    "age": [45, 60, 55, 70, 50, 65],
    "treatment": [1, 0, 1, 0, 1, 0],
})

cph = CoxPHFitter()
cph.fit(df, duration_col="duration", event_col="event")
cph.print_summary()
cph.plot_partial_effects_on_outcome("treatment", [0, 1])
```

## Weibull Parametric Model

```python
from lifelines import WeibullAFTFitter

wbf = WeibullAFTFitter()
wbf.fit(df, duration_col="duration", event_col="event")
wbf.print_summary()
```

## References
- [Lifelines docs](https://lifelines.readthedocs.io/)
- [Survival Analysis intro](https://arxiv.org/abs/2108.05918)