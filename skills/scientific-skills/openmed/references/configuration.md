---
name: openmed-configuration
description: "OpenMed configuration profiles, model registry navigation, and performance profiling for reproducible deployments."
tags: [openmed, configuration, profiles, model-registry, profiling]
---

# Configuration & Profiling

OpenMed supports environment-specific configuration profiles and built-in performance profiling.

## Configuration Profiles

```python
from openmed import OpenMedConfig, analyze_text

# Load a named profile
config = OpenMedConfig.from_profile("prod")

# Apply to analysis
result = analyze_text(text, model_name="disease_detection_superclinical", config=config)
```

Available profiles: `dev`, `prod`, `test`, `fast`

## Model Registry

```python
from openmed.core.model_registry import (
    get_all_models,
    list_model_categories,
    get_models_by_category,
    get_model_info,
    get_model_suggestions,
)

# List all categories
print(list_model_categories())

# Get models in a category
for info in get_models_by_category("Oncology"):
    print(info.display_name, info.model_id)

# Get detailed model info
info = get_model_info("disease_detection_superclinical")
print(info.entity_types, info.recommended_confidence)
```

## Performance Profiling

```python
from openmed import analyze_text, profile_inference

with profile_inference() as profiler:
    result = analyze_text(text, model_name="disease_detection_superclinical")
print(profiler.summary())
```

## References
- https://openmed.life/docs/configuration
- https://openmed.life/docs/profiles
- https://openmed.life/docs/model-registry
