---
name: bentoml
description: "BentoML — model serving and deployment. Build prediction services from any ML framework with OpenAPI/Swagger. Containerize, deploy to Kubernetes, AWS, GCP, Azure. Adaptive batching and GPU support."
tags: [bentoml, model-serving, deployment, mlops, api, kubernetes, zorai]
---
## Overview

BentoML packages ML models with service definitions, dependencies, environment config, and deployment targets into a portable "Bento." Deploy to Kubernetes (Kserve, Seldon), AWS SageMaker, GCP Vertex AI, or as a standalone Docker container.

## Installation

```bash
uv pip install bentoml
```

## Service Definition

```python
import bentoml
from bentoml.io import JSON
import numpy as np

iris_clf = bentoml.sklearn.get("iris_model:latest")

@bentoml.service
class IrisClassifier:
    def __init__(self):
        self.model = iris_clf.to_runner()
        self.model.init_local()

    @bentoml.api(input=JSON(), output=JSON())
    def classify(self, input_data):
        result = self.model.run(np.array([input_data["features"]]))
        return {"class": int(result[0]), "probabilities": result[1].tolist()}
```

## Build & Deploy

```bash
bentoml build      # creates a Bento
bentoml containerize iris_classifier:latest  # Docker image
docker run -p 3000:3000 iris_classifier:latest
```

## References
- [BentoML docs](https://docs.bentoml.com/)
- [BentoML GitHub](https://github.com/bentoml/BentoML)