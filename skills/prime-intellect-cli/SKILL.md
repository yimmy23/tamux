---
name: prime-intellect-cli
description: Use when provisioning Prime Intellect GPU compute, managing pods/disks/sandboxes, running hosted RL training via prime lab, installing or publishing RL environments, or exposing local services via Prime Tunnel. Covers the `prime` CLI (PyPI: prime) for all Prime Intellect platform operations.
tags: [prime-intellect, cli, gpu, compute, rl-training, sandboxes, tunnel, environments]
---

# Prime Intellect CLI

The `prime` CLI is the command-line interface for managing Prime Intellect compute resources, RL environments, code sandboxes, and tunnels. This skill provides the command reference and decision patterns for all platform operations.

## Prerequisites

- Python 3.10+
- Install: `uv tool install prime` (preferred) or `pip install prime`
- Auth: `prime login` (interactive) or `prime config set-api-key` (headless)
- SSH key: `prime config set-ssh-key-path` (for pod access)
- Verify: `prime config view`

## Command Reference

### Configuration (`prime config`)

| Command | Description | Default |
|---|---|---|
| `view` | Display current configuration | - |
| `set-api-key` | Set API key | - |
| `set-team-id` | Set team ID for team billing | - |
| `remove-team-id` | Switch back to personal account | - |
| `set-base-url` | Set API base URL | `https://api.primeintellect.ai` |
| `set-ssh-key-path` | Set SSH private key path | `~/.ssh/id_rsa` |
| `set-share-resources-with-team <bool>` | Auto-share new instances with team | `false` |
| `reset` | Reset all settings (removes API key) | - |

### GPU Availability (`prime availability`)

```bash
# List all GPU configurations with pricing
prime availability list

# Filter by GPU type, region, count, socket
prime availability list --gpu-type H100_80GB --regions united_states --gpu-count 2 --socket PCIe

# Find GPUs compatible with existing disks
prime availability list --disks disk-id-1 --disks disk-id-2

# List disk availability
prime availability disks --regions united_states
```

Filters: `--gpu-type`, `--gpu-count`, `--regions`, `--socket` (PCIe, SXM2, SXM3, SXM4, SXM5), `--disks`, `--group-similar` (default true).

### Pod Management (`prime pods`)

```bash
# Create interactively
prime pods create

# Create non-interactively
prime pods create \
  --gpu-type H100_80GB \
  --gpu-count 1 \
  --disk-size 100 \
  --name my-pod

# Create with persistent disks attached
prime pods create --id 346663 --disks disk-id-1 --disks disk-id-2

# Create with custom template
prime pods create --image custom_template --custom-template-id "template_id"

# List pods
prime pods list

# SSH into a pod
prime pods ssh <pod-id>

# Delete a pod
prime pods delete <pod-id>
```

Pod creation options: `--id`, `--cloud-id`, `--gpu-type`, `--gpu-count`, `--name`, `--disk-size`, `--vcpus`, `--memory`, `--image`, `--team-id`, `--env KEY=value`, `--disks`, `--share-with-team`, `--add-members`.

### Disk Management (`prime disks`)

```bash
# Check availability
prime availability disks

# Create a persistent disk
prime disks create --id c008ad --size 500 --name ml-training-data

# List disks (with pagination)
prime disks list --limit 50 --offset 0 --output json

# Delete a disk
prime disks delete <disk-id>
```

Disks persist independently from pods and are billed continuously until terminated. Use `--yes` to skip confirmation in automation.

### Sandboxes (`prime sandbox`)

```bash
# Create a sandbox
prime sandbox create python:3.11-slim \
  --name analytics-lab \
  --cpu-cores 2 --memory-gb 4 --disk-size-gb 20 \
  --timeout-minutes 240 --idle-timeout-minutes 15 \
  --env PROFILE=production \
  --secret DB_PASSWORD=<value> \
  --label experiment --label ml-pipeline

# Run a command inside
prime sandbox run sbx_123 --working-dir /workspace "python -c 'print(42)'"

# Upload / download (200MB per-file limit)
prime sandbox upload sbx_123 notebooks/analysis.ipynb /workspace/
prime sandbox download sbx_123 /workspace/report.csv reports/latest.csv

# Expose ports (range 22-9000; 8080, 2222, 8081 excluded)
prime sandbox expose <sandbox-id> 8000 --name web-server
prime sandbox expose <sandbox-id> 9000 --name tcp-server --protocol TCP
prime sandbox list-ports <sandbox-id>
prime sandbox unexpose <sandbox-id> <exposure-id> --yes

# SSH into a sandbox
prime sandbox ssh <sandbox-id> --shell zsh

# Inspect, logs, cleanup
prime sandbox list --status RUNNING --output table
prime sandbox get sbx_123 --output json
prime sandbox logs sbx_123 > logs.txt
prime sandbox delete --label experiment --yes
```

Idle timeout constraints:
- Disabled by default; opt in with `--idle-timeout-minutes`.
- Must satisfy `1 <= idle <= timeout` and `idle <= 1440`.
- Not supported for VM-backed sandboxes (`--vm`).
- SSH sessions do not count as activity.
- File transfer auth errors: `prime sandbox reset-cache` then retry.

### Hosted RL Training (`prime lab`, `prime train`, `prime eval`)

```bash
# Set up workspace
prime lab setup

# Install an environment
prime env install primeintellect/alphabet-sort

# Run baseline evaluation
prime eval run primeintellect/alphabet-sort \
  -m Qwen/Qwen3-4B-Instruct-2507 -n 20 -r 1
prime eval tui

# Launch training
prime train run configs/rl/alphabet-sort.toml

# Monitor
prime train logs <run-id> -f
prime train models    # list available models
```

Training config (TOML):

```toml
model = "Qwen/Qwen3-4B-Instruct-2507"
max_steps = 50
batch_size = 128
rollouts_per_example = 8

[sampling]
max_tokens = 512

[[env]]
id = "primeintellect/alphabet-sort"

# Optional W&B integration
[wandb]
project = "my-experiment"
name = "alphabet-sort-30b"

# Optional periodic eval
[eval]
interval = 50
```

Run size guidelines:

| Size | Model | max_steps | batch_size | rollouts_per_example |
|---|---|---|---|---|
| Validation | `Qwen/Qwen3-4B-Instruct-2507` | 50 | 128 | 8 |
| Experimentation | `Qwen/Qwen3-30B-A3B-Instruct-2507` | 200 | 256 | 16 |
| Production | `Qwen/Qwen3-235B-A22B-Instruct-2507` | 1000+ | 512+ | 16+ |

### RL Environments Hub (`prime env`)

```bash
prime env list
prime env info owner/environment-name
prime env install owner/environment-name
prime env init my-new-environment
```

### Prime Tunnel (`prime tunnel`)

```bash
# Expose a local service
prime tunnel start --port 8000

# With basic auth (password auto-generated, shown once)
prime tunnel start --port 8000 --auth alice
```

Returns a public HTTPS URL like `https://t-0-abc123def456.tunnel.pinfra.io`. Hosted evaluations can use tunnels with `--allow-tunnel-access` flag.

## Common Mistakes

- **Forgetting to clean up disks**: disks are billed continuously until explicitly deleted, even after pods are terminated. Always `prime disks delete` when done.
- **Using `--env` for secrets**: env vars are plaintext and visible on inspect. Use `--secret` for credentials.
- **Sandbox idle timeout with SSH**: SSH sessions do not count as activity. A sandbox can be reaped while you have an active SSH session.
- **Port restrictions**: ports 8080, 2222, and 8081 cannot be exposed. Valid range is 22-9000.
- **200MB file transfer limit**: individual file uploads/downloads are capped at 200MB. Use multiple calls or compress first.
- **Not setting team context**: if billing to a team, set `prime config set-team-id` before provisioning, or pass `--team-id` per command.
- **Custom template compatibility**: ensure templates are compatible with the selected GPU configuration before creating pods.
