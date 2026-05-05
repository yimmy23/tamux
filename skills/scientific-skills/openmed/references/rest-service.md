---
name: openmed-rest-service
description: "Deploy OpenMed behind a FastAPI REST API with Docker. Endpoints for health, text analysis, and PII operations."
tags: [openmed, rest-api, fastapi, docker, service, deployment]
---

# REST API Service

OpenMed includes a production-ready FastAPI service with Docker support.

## Run Locally

```bash
uv pip install -e ".[hf,service]"
uvicorn openmed.service.app:app --host 0.0.0.0 --port 8080
```

With model warm-up:

```bash
OPENMED_SERVICE_PRELOAD_MODELS=disease_detection_superclinical \
uvicorn openmed.service.app:app --host 0.0.0.0 --port 8080
```

## Docker

```bash
docker build -t openmed:1.2.0 .
docker run --rm -p 8080:8080 -e OPENMED_PROFILE=prod openmed:1.2.0
```

## Endpoints

| Method | Path | Description |
|---|---|---|
| GET | `/health` | Service health check |
| POST | `/analyze` | Analyze clinical text |
| POST | `/pii/extract` | Extract PII entities |
| POST | `/pii/deidentify` | De-identify text |

## Example Request

```bash
curl -X POST http://127.0.0.1:8080/pii/extract \
  -H "Content-Type: application/json" \
  -d '{"text":"Paciente: Maria Garcia, DNI: 12345678Z","lang":"es"}'
```

## Error Handling

Non-2xx responses use a unified error envelope:

```json
{
  "error": {
    "code": "validation_error",
    "message": "Text must not be blank",
    "details": [{"field": "body.text", "message": "Text must not be blank"}]
  }
}
```

## References
- docs/rest-service.md
- https://openmed.life/docs
