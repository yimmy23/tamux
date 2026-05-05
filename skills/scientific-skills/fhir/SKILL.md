---
name: fhir
description: "FHIR (Fast Healthcare Interoperability Resources) standard. Tools for reading, writing, and querying healthcare data via FHIR APIs: patients, observations, conditions, medications, procedures. Interop with EHR systems."
tags: [fhir, healthcare, ehr, interoperability, hl7, api, zorai]
---
## Overview

FHIR (Fast Healthcare Interoperability Resources) tools for reading, writing, and querying healthcare data via FHIR REST APIs. Work with Patient, Observation, Condition, MedicationRequest, and Encounter resources from EHR systems.

## Installation

```bash
uv pip install fhir.resources requests
```

## Query Patients

```python
import requests
from fhir.resources.patient import Patient

base_url = "https://hapi.fhir.org/baseR4"
resp = requests.get(f"{base_url}/Patient", params={"family": "Smith", "birthdate": "gt1970"})
data = resp.json()

for entry in data.get("entry", []):
    patient = Patient.parse_obj(entry["resource"])
    name = patient.name[0]
    print(f"{name.family}, {name.given[0]}")
```

## Create a Resource

```python
patient = Patient(
    name=[{"family": "Doe", "given": ["John"], "use": "official"}],
    birthDate="1980-05-15",
    gender="male",
)
resp = requests.post(
    f"{base_url}/Patient",
    json=patient.dict(),
    headers={"Content-Type": "application/fhir+json"},
)
print(f"Created: {resp.json()['id']}")
```

## Workflow

1. Identify FHIR server endpoint (HAPI, Epic, Cerner)
2. Query resources with search parameters
3. Parse responses into typed resource objects
4. Create/update with POST/PUT
5. Use `$everything` for comprehensive patient data
