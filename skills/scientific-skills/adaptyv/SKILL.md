---
name: adaptyv
author: "K-Dense, Inc."
description: "How to use the Adaptyv Bio Foundry API and Python SDK for protein experiment design, submission, and results retrieval. Use this skill whenever the user mentions Adaptyv, Foundry API, protein binding assays, protein screening experiments, BLI/SPR assays, thermostability assays, or wants to submit protein sequences for experimental characterization. Also trigger when code imports `adaptyv`, `adaptyv_sdk`, or `FoundryClient`, or references `foundry-api-public.adaptyvbio.com`."

tags: [scientific-skills, adaptyv, python, api, experimental-design, search, citation-management, experimentation]
---|---|---|
| `affinity` | `bli` or `spr` | KD, kon, koff kinetics | Yes |
| `screening` | `bli` or `spr` | Yes/no binding | Yes |
| `thermostability` | — | Melting temperature (Tm) | No |
| `expression` | — | Expression yield | No |
| `fluorescence` | — | Fluorescence intensity | No |

## Experiment Lifecycle

```
Draft → WaitingForConfirmation → QuoteSent → WaitingForMaterials → InQueue → InProduction → DataAnalysis → InReview → Done
```

| Status | Who Acts | Description |
|---|---|---|
| `Draft` | You | Editable, no cost commitment |
| `WaitingForConfirmation` | Adaptyv | Under review, quote being prepared |
| `QuoteSent` | You | Review and confirm the quote |
| `WaitingForMaterials` | Adaptyv | Gene fragments and target ordered |
| `InQueue` | Adaptyv | Materials arrived, queued for lab |
| `InProduction` | Adaptyv | Assay running |
| `DataAnalysis` | Adaptyv | Raw data processing and QC |
| `InReview` | Adaptyv | Final validation |
| `Done` | You | Results available |
| `Canceled` | Either | Experiment canceled |

The `results_status` field on an experiment tracks: `none`, `partial`, or `all`.

## Common Workflows

### 1. Submit a Binding Screen (Step by Step)

```python
# 1. Find a target
targets = client.targets.list(search="EGFR", selfservice_only=True)
target_id = targets.items[0].id

# 2. Preview cost
estimate = client.experiments.cost_estimate({
    "experiment_spec": {
        "experiment_type": "screening",
        "method": "bli",
        "target_id": target_id,
        "sequences": {"seq1": "EVQLVESGGGLVQ...", "seq2": "MKVLVAG..."},
        "n_replicates": 3
    }
})

# 3. Create experiment (starts as Draft)
exp = client.experiments.create({
    "name": "EGFR binder screen batch 1",
    "experiment_spec": {
        "experiment_type": "screening",
        "method": "bli",
        "target_id": target_id,
        "sequences": {"seq1": "EVQLVESGGGLVQ...", "seq2": "MKVLVAG..."},
        "n_replicates": 3
    }
})

# 4. Submit for review
client.experiments.submit(exp.experiment_id)

# 5. Poll or use webhooks until Done
# 6. Retrieve results
results = client.experiments.get_results(exp.experiment_id)
```

### 2. Automated Pipeline (Skip Draft + Auto-Accept Quote)

```python
exp = client.experiments.create({
    "name": "Auto pipeline run",
    "experiment_spec": {...},
    "skip_draft": True,
    "auto_accept_quote": True,
    "webhook_url": "https://my-server.com/webhook"
})
# Webhook fires on each status transition; poll or wait for Done
```

### 3. Using Webhooks

Pass `webhook_url` when creating an experiment. Adaptyv POSTs to that URL on every status transition with the experiment ID, previous status, and new status.

## Sequences

- Simple format: `{"seq1": "EVQLVESGGGLVQPGGSLRLSCAAS"}`
- Rich format: `{"seq1": {"aa_string": "EVQLVESGGGLVQ...", "control": false, "metadata": {"type": "scfv"}}}`
- Multi-chain: use colon separator — `"MVLS:EVQL"`
- Valid amino acids: A, C, D, E, F, G, H, I, K, L, M, N, P, Q, R, S, T, V, W, Y (case-insensitive, stored uppercase)
- Sequences can only be added to experiments in `Draft` status

## Filtering, Sorting, and Pagination

All list endpoints support pagination (`limit` 1-100, default 50; `offset`), search (free-text on name fields), and sorting.

**Filtering** uses s-expression syntax via the `filter` query parameter:
- Comparison: `eq(field,value)`, `neq`, `gt`, `gte`, `lt`, `lte`, `contains(field,substring)`
- Range/set: `between(field,lo,hi)`, `in(field,v1,v2,...)`
- Logic: `and(expr1,expr2,...)`, `or(...)`, `not(expr)`
- Null: `is_null(field)`, `is_not_null(field)`
- JSONB: `at(field,key)` — e.g., `eq(at(metadata,score),42)`
- Cast: `float()`, `int()`, `text()`, `timestamp()`, `date()`

**Sorting** uses `asc(field)` or `desc(field)`, comma-separated (max 8):
```
sort=desc(created_at),asc(name)
```

**Example:** `filter=and(gte(created_at,2026-01-01),eq(status,done))`

## Error Handling

All errors return:
```json
{
  "error": "Human-readable description",
  "request_id": "req_019462a4-b1c2-7def-8901-23456789abcd"
}
```
The `request_id` is also in the `x-request-id` response header — include it when contacting support.

## Token Management

Tokens use Biscuit-based cryptographic attenuation. You can create restricted tokens scoped by organization, resource type, actions (read/create/update), and expiry via `POST /tokens/attenuate`. Revoking a token (`POST /tokens/revoke`) revokes it and all its descendants.

## Detailed API Reference

For the full list of all 32 endpoints with request/response schemas, read `references/api-endpoints.md`.
