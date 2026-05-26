---
name: runpod-parameter-golf-task
description: Use when preparing, launching, monitoring, or collecting results for Parameter Golf or similar competition training runs on RunPod GPU Pods.
recommended_skills:
  - runpod-parameter-golf
  - nanogpt
  - optimize-for-gpu
recommended_guidelines:
  - environment-setup-task
  - terminal-operations-task
  - deployment-release-task
---

# RunPod Parameter Golf Task Guideline

Use this guideline when the operator asks to run a Parameter Golf submission, provision RunPod GPUs, prepare FineWeb/SP1024 data, or collect logs/artifacts for a competition folder.

## Scope Rules

1. Competition runs must be fully Parameter Golf-shaped: one record folder containing `train_gpt.py`, `README.md`, `submission.json`, and later `train.log`.
2. Do not use unrelated training datasets, HF Trainer wrappers, local package imports, external checkpoints, or local workspace-only dependencies for a leaderboard-style run.
3. The script must run from inside the record folder in a clean Parameter Golf checkout.
4. Use FineWeb cached challenge shards and the stock SP1024 tokenizer unless the submission explicitly pays for and justifies a tokenizer change.
5. Record exact commands, pod specs, artifact size, final `val_bpb`, final `val_loss`, and whether the result is a smoke/non-record/final run.

## Required Operator Inputs Before You Provision

Ask for these as a compact checklist, not one by one unless a value is ambiguous:

- RunPod access path: API key with Pod read/write permission, or an already-created Pod SSH command.
- Budget ceiling: maximum dollars, hours, or explicit stop condition.
- Run mode: `smoke`, `final`, or `smoke-then-final`.
- GPU target: for final, prefer `NVIDIA H100 80GB HBM3` with `gpuCount=8`; for smoke, use `gpuCount=1`.
- Cloud/storage: Secure Cloud preferred; `volumeInGb` at least 200GB for full cached FineWeb, or a pre-existing `networkVolumeId`.
- SSH/file transfer method: public-IP SSH with `22/tcp`, `rsync/scp`, `runpodctl`, or Git clone.
- Source location: repository URL/branch or exact local folder to copy.
- Cleanup policy: stop or terminate Pod after logs/artifacts are retrieved.

## RunPod Provisioning Defaults

Use these defaults unless the operator says otherwise:

```json
{
  "cloudType": "SECURE",
  "computeType": "GPU",
  "gpuTypeIds": ["NVIDIA H100 80GB HBM3"],
  "gpuCount": 8,
  "imageName": "runpod/pytorch:2.1.0-py3.10-cuda11.8.0-devel-ubuntu22.04",
  "containerDiskInGb": 50,
  "volumeInGb": 200,
  "volumeMountPath": "/workspace",
  "ports": ["22/tcp", "8888/http"],
  "supportPublicIp": true,
  "interruptible": false
}
```

For smoke tests set `gpuCount=1` and `volumeInGb=100` if FineWeb is already cached elsewhere; otherwise keep 200GB.

## Execution Workflow

1. Confirm access, budget, mode, GPU count, storage, and cleanup policy.
2. Create or connect to the Pod. Prefer public-IP SSH when file transfer is needed; proxied basic SSH does not support SCP/SFTP.
3. Verify environment:
   - `nvidia-smi`
   - `python3 -c "import torch, numpy, sentencepiece; print(torch.__version__)"`
   - `df -h /workspace`
4. Clone Parameter Golf into `/workspace/parameter-golf` if absent.
5. Install missing deps only if needed: `pip install numpy sentencepiece huggingface-hub datasets tqdm`.
6. Cache data:
   - smoke: `python3 data/cached_challenge_fineweb.py --variant sp1024 --train-shards 1`
   - final: `python3 data/cached_challenge_fineweb.py --variant sp1024`
7. Place the submission folder under the correct `records/<track>/...` directory.
8. Run from inside the record folder with explicit env vars and `torchrun`.
9. Monitor logs until completion or failure. Do not declare success before `final_int8_zlib_roundtrip_exact` appears.
10. Copy `logs/<RUN_ID>.txt` to `train.log`, update `submission.json`, and retrieve `train.log`, `submission.json`, `final_model.int8.ptz` if needed.
11. Stop or terminate according to policy. If using a non-network Pod volume, retrieve artifacts before terminating.

## Quality Gate

A run is usable only when the record folder contains:

- self-contained `train_gpt.py`
- `README.md` with exact command and architecture summary
- `submission.json` with actual metrics, not placeholders
- `train.log` copied from the run
- log lines showing:
  - `Total submission size int8+zlib: < 16000000`
  - `final_int8_zlib_roundtrip_exact val_loss:... val_bpb:...`
  - wallclock evidence for 10-minute compliance when claiming record eligibility

If any requirement is missing, classify the result as `prepared`, `smoke`, or `non-record`, not a final competition run.
