---
name: epidemiology
description: "Disease modeling and epidemiological analysis: SIR/SEIR compartmental models, R0 estimation, outbreak simulation, incidence/prevalence forecasting, and intervention impact modeling."
tags: [epidemiology, disease-modeling, simulation, public-health, sir, zorai]
---
## Overview

Epidemiological disease modeling with compartmental models (SIR, SEIR), R0 estimation, outbreak simulation, incidence/prevalence forecasting, and intervention impact analysis. Covers the core ODE-based approach used in public health and infectious disease research.

## Installation

```bash
uv pip install scipy numpy matplotlib
```

## SIR Model

```python
import numpy as np
from scipy.integrate import solve_ivp

def sir(t, y, beta, gamma):
    S, I, R = y
    dS = -beta * S * I
    dI = beta * S * I - gamma * I
    dR = gamma * I
    return [dS, dI, dR]

beta, gamma = 0.3, 0.1
R0 = beta / gamma
print(f"R0 = {R0:.2f}")

sol = solve_ivp(sir, [0, 160], [0.99, 0.01, 0], args=(beta, gamma), dense_output=True)
```

## SEIR Model

```python
def seir(t, y, beta, sigma, gamma):
    S, E, I, R = y
    dS = -beta * S * I
    dE = beta * S * I - sigma * E
    dI = sigma * E - gamma * I
    dR = gamma * I
    return [dS, dE, dI, dR]

sol = solve_ivp(seir, [0, 200], [0.99, 0.005, 0.005, 0], args=(0.3, 0.2, 0.1))
```

## Key Parameters

- **R0 (basic reproduction number)**: average secondary cases from one infected in a naive population
- **Beta**: transmission rate (contacts * probability of infection per contact)
- **Gamma**: recovery rate (1 / infectious period)
- **Sigma**: incubation rate (1 / incubation period)

## Workflow

1. Estimate parameters from literature or case data
2. Define compartment equations (SIR, SEIR, extended with age/risk strata)
3. Solve ODE with `solve_ivp`
4. Plot S, I, R curves vs time
5. Run sensitivity: change beta/gamma and observe peak timing, total cases
6. Add interventions by reducing beta over time (lockdown, masking, vaccination)
7. Compare scenarios: no intervention vs vaccination vs NPIs

## References
- [SciPy ODE docs](https://docs.scipy.org/doc/scipy/reference/integrate.html)
- [Compartmental models in epidemiology](https://en.wikipedia.org/wiki/Compartmental_models_in_epidemiology)