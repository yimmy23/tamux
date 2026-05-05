---
name: clinical-trials
description: "ClinicalTrials.gov API client and analysis toolkit. Search, filter, and download trial records. Analyze trial designs, endpoints, enrollment, sponsors, and results. Automate systematic trial discovery."
tags: [clinical-trials, research, api, systematic-review, healthcare, zorai]
---
## Overview

Search, filter, and download clinical trial records from ClinicalTrials.gov. Analyze trial designs, endpoints, enrollment, sponsors, and results. Automate systematic trial discovery.

## Installation

```bash
uv pip install requests
```

## Search Trials

```python
import requests

params = {
    "query.term": "diabetes AND metformin AND phase 3",
    "pageSize": 25,
    "format": "json",
    "sort": "LastUpdateDate",
}

resp = requests.get("https://clinicaltrials.gov/api/v2/studies", params=params)
data = resp.json()

for study in data.get("studies", []):
    p = study["protocolSection"]
    nct = p["identificationModule"]["nctId"]
    title = p["identificationModule"]["briefTitle"]
    status = p["statusModule"].get("overallStatus", "Unknown")
    print(f"{nct}: {title[:60]} [{status}]")
```

## Study Details

```python
resp = requests.get("https://clinicaltrials.gov/api/v2/studies/NCT04251195")
study = resp.json()
design = study["protocolSection"]["designModule"]
print(f"Purpose: {design.get('primaryPurpose')}")
```

## Workflow

1. Search trials via ClinicalTrials.gov API v2
2. Filter by condition, intervention, phase, status
3. Download structured trial data (JSON)
4. Extract PICO: Population, Intervention, Comparison, Outcome
5. Analyze trial designs, enrollment, and results
