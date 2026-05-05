---
name: mimic
description: "MIMIC (Medical Information Mart for Intensive Care) database toolkit. Curated ICU data: vitals, labs, medications, notes, diagnoses. Tools for querying MIMIC-III/IV, building ML features, and reproducing benchmarks."
tags: [mimic, icu, ehr, clinical-database, research, healthcare, zorai]
---
## Overview

MIMIC (Medical Information Mart for Intensive Care) provides ICU data: vitals, labs, medications, notes, diagnoses. Tools for querying MIMIC-III/IV, building ML features, and reproducing clinical benchmarks.

## Access

Apply for access at https://physionet.org/content/mimiciv/ -- requires CITI data use training.

## Installation

```bash
uv pip install psycopg2 pandas
```

## Python Analysis

```python
import pandas as pd
from sqlalchemy import create_engine

engine = create_engine("postgresql://user:pass@localhost:5432/mimiciv")

# First 24h vitals
query = """
SELECT subject_id, charttime, valuenum
FROM mimiciv_icu.chartevents
WHERE itemid = 220045
AND valuenum IS NOT NULL
LIMIT 100
"""
hr = pd.read_sql(query, engine)
```

## Workflow

1. Apply for MIMIC access (physionet.org)
2. Load MIMIC-IV into PostgreSQL
3. Query ICU stays, diagnoses, labs, medications, notes
4. Extract features (vitals over time, labs at admission, comorbidity scores)
5. Build ML benchmarks (in-hospital mortality, LOS prediction, sepsis detection)
