---
name: prime-intellect-cli-task
description: Use when provisioning Prime Intellect GPU compute, managing pods/disks/sandboxes, running hosted RL training, installing RL environments, or exposing tunnels via the prime CLI.
recommended_skills:
  - prime-intellect-cli
recommended_guidelines:
  - environment-setup-task
  - api-integration-task
  - terminal-operations-task
---

# Prime Intellect CLI Task Guideline

Use this guideline when the operator asks to provision GPU instances on Prime Intellect, run hosted RL training, manage code sandboxes, install or publish RL environments, or expose local services via Prime Tunnel — all through the `prime` CLI.

## Scope Rules

1. The `prime` CLI (PyPI package `prime`, installed via `uv tool install prime` or `pip install prime`) is the only interface covered here. Do not confuse with the Prime Intellect REST API, which is a separate integration surface.
2. All commands require authentication: `prime login` (interactive) or `prime config set-api-key` (headless). Verify auth before any resource command.
3. Compute resources are billed continuously from creation until termination. Always confirm a cleanup policy (stop vs terminate) before provisioning.
4. Do not run training or sandbox commands against production workspaces without confirming the team context (`prime config set-team-id` or `--team-id`).
5. Sandbox secrets (`--secret KEY=VALUE`) are encrypted at rest; env vars (`--env KEY=VALUE`) are plaintext and visible on inspect. Never put credentials in `--env`.

## Required Operator Inputs Before Provisioning

Ask for these as a compact checklist, not one by one unless a value is ambiguous:

- **Auth path**: `prime login` (interactive OAuth) or API key with the required permission scopes (`Instances -> Read and write`, `Disks -> Read and write`, etc.).
- **Budget ceiling**: maximum dollars, hours, or explicit stop condition.
- **GPU target**: model (e.g. `H100_80GB`, `A100`), count, socket type (PCIe, SXM4), region preference.
- **Run mode**: `smoke`, `validation`, or `production` — determines GPU count, step count, and disk size.
- **Team context**: personal account or a specific team ID for billing and sharing.
- **SSH key**: path to private key (`prime config set-ssh-key-path`) for pod access.
- **Cleanup policy**: stop or terminate pods/disks/sandboxes after work is done.

## Provisioning Defaults

Use these defaults unless the operator says otherwise:

| Parameter | Smoke / Validation | Production |
|---|---|---|
| GPU type | `H100_80GB` | `H100_80GB` |
| GPU count | 1 | 8 |
| Disk size | 100 GB | 500 GB |
| Image | `runpod/pytorch:2.1.0-py3.10-cuda11.8.0-devel-ubuntu22.04` or template | operator-specified |
| Region | `united_states` | `united_states` |
| `--share-with-team` | false | true (if team set) |
| Timeout | 240 min | operator-specified |

## Workflow by Task Type

### 1. First-Time Setup

```bash
curl -LsSf https://astral.sh/uv/install.sh | sh
uv tool install prime
prime login
prime config set-ssh-key-path
prime config view
```

Verify the configuration shows API key (masked), team ID, base URL, and SSH key path.

### 2. GPU Provisioning

Check availability, then create a pod:

```bash
# Check what's available
prime availability list --gpu-type H100_80GB --regions united_states

# Create interactively
prime pods create

# Or non-interactively
prime pods create \
  --gpu-type H100_80GB \
  --gpu-count 1 \
  --disk-size 100 \
  --name my-training-pod
```

Attach persistent disks if the operator has existing storage:

```bash
prime pods create --id 346663 --disks disk-id-1 --disks disk-id-2
```

SSH into the pod:

```bash
prime pods ssh <pod-id>
```

List and clean up:

```bash
prime pods list
prime pods delete <pod-id>   # when done
```

### 3. Disk Management

```bash
# Check disk availability
prime availability disks --regions united_states

# Create a persistent disk
prime disks create --id c008ad --size 500 --name ml-training-data

# List disks
prime disks list --output json

# Delete when no longer needed (billed until terminated)
prime disks delete <disk-id>
```

### 4. Hosted RL Training (Lab)

```bash
# Set up workspace
mkdir ~/dev/my-lab && cd ~/dev/my-lab
prime lab setup

# Install an environment
prime env install primeintellect/alphabet-sort

# Run baseline evaluation
prime eval run primeintellect/alphabet-sort \
  -m Qwen/Qwen3-4B-Instruct-2507 -n 20 -r 1
prime eval tui

# Create a training config (TOML in configs/rl/)
prime train run configs/rl/alphabet-sort.toml

# Monitor
prime train logs <run-id> -f
```

Training config fields:

| Field | Description |
|---|---|
| `model` | Hugging Face model ID (must be supported) |
| `max_steps` | Total training steps |
| `batch_size` | Rollouts per training batch |
| `rollouts_per_example` | Rollouts generated per dataset example |
| `[sampling].max_tokens` | Max tokens per model response |
| `[[env]].id` | Environment ID from the Environments Hub |
| `[wandb]` | Optional W&B integration |
| `[eval].interval` | Optional periodic eval during training |

### 5. Sandboxes

```bash
# Create a sandbox
prime sandbox create python:3.11-slim \
  --name analytics-lab \
  --cpu-cores 2 \
  --memory-gb 4 \
  --disk-size-gb 20 \
  --timeout-minutes 240 \
  --idle-timeout-minutes 15 \
  --env PROFILE=production \
  --secret DB_PASSWORD=<value>

# Run commands inside
prime sandbox run sbx_123 --working-dir /workspace "python -c 'print(42)'"

# Upload / download files (200MB per-file limit)
prime sandbox upload sbx_123 notebooks/analysis.ipynb /workspace/
prime sandbox download sbx_123 /workspace/report.csv reports/latest.csv

# Expose ports (range 22-9000; 8080, 2222, 8081 excluded)
prime sandbox expose <sandbox-id> 8000 --name web-server
prime sandbox expose <sandbox-id> 9000 --name tcp-server --protocol TCP

# SSH into a sandbox
prime sandbox ssh <sandbox-id>

# List, inspect, clean up
prime sandbox list --status RUNNING --output table
prime sandbox get sbx_123 --output json
prime sandbox logs sbx_123 > logs.txt
prime sandbox delete --label experiment --yes
```

Idle timeout rules:
- Disabled by default; opt in with `--idle-timeout-minutes`.
- Must satisfy `1 <= idle <= timeout` and `idle <= 1440`.
- Not supported for VM-backed sandboxes (`--vm`).
- SSH sessions do not count as activity; a sandbox with an active SSH connection can still be reaped.

### 6. Prime Tunnel

```bash
# Expose a local service to the internet
prime tunnel start --port 8000

# With basic auth
prime tunnel start --port 8000 --auth alice
```

The CLI prints a public HTTPS URL (e.g. `https://t-0-abc123def456.tunnel.pinfra.io`). With auth enabled, a password is auto-generated server-side and shown exactly once.

### 7. RL Environments Hub

```bash
# List available environments
prime env list

# Get info on an environment
prime env info owner/environment-name

# Install an environment
prime env install owner/environment-name

# Create a new environment
prime env init my-new-environment
```

### 8. Configuration Management

| Command | Description | Default |
|---|---|---|
| `prime config view` | Display current configuration | - |
| `prime config set-api-key` | Set API key | - |
| `prime config set-team-id` | Set team ID for team access | - |
| `prime config remove-team-id` | Switch to personal account | - |
| `prime config set-base-url` | Set API base URL | `https://api.primeintellect.ai` |
| `prime config set-ssh-key-path` | Set SSH private key path | `~/.ssh/id_rsa` |
| `prime config set-share-resources-with-team` | Auto-share new instances | `false` |
| `prime config reset` | Reset to defaults (removes API key) | - |

## Common Failure Modes

- **Auth errors**: run `prime config view` to verify API key is set. Use `prime login` for interactive re-auth.
- **No GPU stock**: filter with `prime availability list` and try alternative regions or GPU types.
- **SSH connection refused**: verify SSH key path with `prime config view`, ensure the key matches the one uploaded to the platform.
- **Sandbox idle timeout**: check `prime sandbox get` for termination reason `Idle Timeout`. Use `--idle-timeout-minutes` only when you have a cleanup mechanism.
- **Disk billed after pod deletion**: disks persist independently. Always run `prime disks delete <disk-id>` when storage is no longer needed.
- **File transfer auth errors**: run `prime sandbox reset-cache` and retry.

## Quality Gate

A Prime Intellect CLI task is complete when:
1. The requested compute resources are provisioned, verified, and accessible (SSH or sandbox exec).
2. Training runs or sandbox commands produce the expected output.
3. All provisioned resources (pods, disks, sandboxes) have a documented cleanup policy and are stopped or terminated when the work is done.
4. Costs are tracked: the operator knows the hourly rate and estimated total for the run.
