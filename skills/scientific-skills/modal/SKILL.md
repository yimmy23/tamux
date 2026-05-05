---
name: modal
description: Cloud computing platform for running Python on GPUs and serverless infrastructure. Use when deploying AI/ML models, running GPU-accelerated workloads, serving web endpoints, scheduling batch jobs, or scaling Python code to the cloud. Use this skill whenever the user mentions Modal, serverless GPU compute, deploying ML models to the cloud, serving inference endpoints, running batch processing in the cloud, or needs to scale Python workloads beyond their local machine. Also use when the user wants to run code on H100s, A100s, or other cloud GPUs, or needs to create a web API for a model.
license: Apache-2.0
tags: [gpu-serverless, cloud-inference, batch-gpu-jobs, model-deployment, modal]
metadata:
  skill-author: K-Dense Inc.
------|-------------|
| `modal setup` | Authenticate with Modal |
| `modal run script.py` | Run a script's local entrypoint |
| `modal serve script.py` | Dev server with hot reload |
| `modal deploy script.py` | Deploy to production |
| `modal volume ls <name>` | List files in a volume |
| `modal volume put <name> <file>` | Upload file to volume |
| `modal volume get <name> <file>` | Download file from volume |
| `modal secret create <name> K=V` | Create a secret |
| `modal secret list` | List secrets |
| `modal app list` | List deployed apps |
| `modal app stop <name>` | Stop a deployed app |

## Reference Files

Detailed documentation for each topic:

- `references/getting-started.md` — Installation, authentication, first app
- `references/functions.md` — Functions, classes, lifecycle hooks, remote execution
- `references/images.md` — Container images, package installation, caching
- `references/gpu.md` — GPU types, selection, multi-GPU, training
- `references/volumes.md` — Persistent storage, file management, v2 volumes
- `references/secrets.md` — Credentials, environment variables, dotenv
- `references/web-endpoints.md` — FastAPI, ASGI/WSGI, streaming, auth, WebSockets
- `references/scheduled-jobs.md` — Cron, periodic schedules, management
- `references/scaling.md` — Autoscaling, concurrency, .map(), limits
- `references/resources.md` — CPU, memory, disk, timeout configuration
- `references/examples.md` — Common use cases and patterns
- `references/api_reference.md` — Key API classes and methods

Read these files when detailed information is needed beyond this overview.
