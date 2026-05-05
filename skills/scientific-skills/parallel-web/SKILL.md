---
name: parallel-web
description: Search the web, extract URL content, and run deep research using the Parallel Chat API and Extract API. Use for ALL web searches, research queries, and general information gathering. Provides synthesized summaries with citations.
allowed-tools: Read Write Edit Bash
license: MIT license
compatibility: PARALLEL_API_KEY required
tags: [scientific-skills, parallel-web, api, writing, search]
metadata:
    skill-author: K-Dense Inc.
---

## Model Selection Guide

The Chat API supports two research models. Use `base` for most searches and `core` for deep research.

| Model  | Latency    | Strengths                        | Use When                    |
|--------|------------|----------------------------------|-----------------------------|
| `base` | 15s-100s   | Standard research, factual queries | Web searches, quick lookups |
| `core` | 60s-5min   | Complex research, multi-source synthesis | Deep research, comprehensive reports |

**Recommendations:**
- `search` command defaults to `base` — fast, good for most queries
- `research` command defaults to `core` — thorough, good for comprehensive reports
- Override with `--model` when you need different depth/speed tradeoffs

---

## Python API Usage

### Search

```python
from parallel_web import ParallelSearch

searcher = ParallelSearch()
result = searcher.search(
    objective="Find latest information about transformer architectures in NLP",
    model="base",
)

if result["success"]:
    print(result["response"])  # Synthesized summary
    for src in result["sources"]:
        print(f"  {src['title']}: {src['url']}")
```

### Deep Research

```python
from parallel_web import ParallelDeepResearch

researcher = ParallelDeepResearch()
result = researcher.research(
    query="Comprehensive analysis of AI regulation in the EU and US",
    model="core",
)

if result["success"]:
    print(result["response"])  # Full research report
    print(f"Citations: {result['citation_count']}")
```

### Extract (Verification Only)

```python
from parallel_web import ParallelExtract

extractor = ParallelExtract()
result = extractor.extract(
    urls=["https://docs.example.com/api-reference"],
    objective="API authentication methods and rate limits",
)

if result["success"]:
    for r in result["results"]:
        print(r["excerpts"])
```

---

## MANDATORY: Save All Results to Sources Folder

**Every web search and deep research result MUST be saved to the project's `sources/` folder.**

This ensures all research is preserved for reproducibility, auditability, and context window recovery.

### Saving Rules

| Operation | `-o` Flag Target | Filename Pattern |
|-----------|-----------------|------------------|
| Web Search | `sources/search_<topic>.md` | `search_YYYYMMDD_HHMMSS_<brief_topic>.md` |
| Deep Research | `sources/research_<topic>.md` | `research_YYYYMMDD_HHMMSS_<brief_topic>.md` |
| URL Extract | `sources/extract_<source>.md` | `extract_YYYYMMDD_HHMMSS_<brief_source>.md` |

### How to Save (Always Use `-o` Flag)

**CRITICAL: Every call to `parallel_web.py` MUST include the `-o` flag pointing to the `sources/` folder.**

```bash
# Web search — ALWAYS save to sources/
python scripts/parallel_web.py search "latest advances in quantum computing 2025" \
  -o sources/search_20250217_143000_quantum_computing.md

# Deep research — ALWAYS save to sources/
python scripts/parallel_web.py research "comprehensive analysis of the global EV battery market" \
  -o sources/research_20250217_144000_ev_battery_market.md

# URL extraction (verification only) — save to sources/
python scripts/parallel_web.py extract "https://example.com/article" --objective "key findings" \
  -o sources/extract_20250217_143500_example_article.md
```

### Why Save Everything

1. **Reproducibility**: Every claim in the final document can be traced back to its raw source material
2. **Context Window Recovery**: If context is compacted mid-task, saved results can be re-read from `sources/`
3. **Audit Trail**: The `sources/` folder provides complete transparency into how information was gathered
4. **Reuse Across Sections**: Saved research can be referenced by multiple sections without duplicate API calls
5. **Cost Efficiency**: Avoid redundant API calls by checking `sources/` for existing results
6. **Peer Review Support**: Reviewers can verify the research backing every claim

### Logging

When saving research results, always log:

```
[HH:MM:SS] SAVED: Search results to sources/search_20250217_143000_quantum_computing.md
[HH:MM:SS] SAVED: Deep research report to sources/research_20250217_144000_ev_battery_market.md
```

### Before Making a New Query, Check Sources First

Before calling `parallel_web.py`, check if a relevant result already exists in `sources/`:

```bash
ls sources/  # Check existing saved results
```

---

## Integration with Scientific Writer

### Routing Table

| Task | Tool | Command |
|------|------|---------|
| Web search (any) | `parallel_web.py search` | `python scripts/parallel_web.py search "query" -o sources/search_<topic>.md` |
| Deep research | `parallel_web.py research` | `python scripts/parallel_web.py research "query" -o sources/research_<topic>.md` |
| Citation verification | `parallel_web.py extract` | `python scripts/parallel_web.py extract "url" -o sources/extract_<source>.md` |
| Academic paper search | `research_lookup.py` | Routes to Perplexity sonar-pro-search |
| DOI/metadata lookup | `parallel_web.py extract` | Extract from DOI URLs (verification) |

### When Writing Scientific Documents

1. **Before writing any section**, use `search` or `research` to gather background information — **save results to `sources/`**
2. **For academic citations**, use `research-lookup` (which routes academic queries to Perplexity) — **save results to `sources/`**
3. **For citation verification** (confirming a specific URL), use `parallel_web.py extract` — **save results to `sources/`**
4. **For current market/industry data**, use `parallel_web.py research --model core` — **save results to `sources/`**
5. **Before any new query**, check `sources/` for existing results to avoid duplicate API calls

---

## Environment Setup

```bash
# Required: Set your Parallel API key
export PARALLEL_API_KEY="your_api_key_here"

# Required Python packages
pip install openai        # For Chat API (search/research)
pip install parallel-web  # For Extract API (verification only)
```

Get your API key at https://platform.parallel.ai

---

## Error Handling

The script handles errors gracefully and returns structured error responses:

```json
{
  "success": false,
  "error": "Error description",
  "timestamp": "2025-02-14 12:00:00"
}
```

**Common issues:**
- `PARALLEL_API_KEY not set`: Set the environment variable
- `openai not installed`: Run `pip install openai`
- `parallel-web not installed`: Run `pip install parallel-web` (only needed for extract)
- `Rate limit exceeded`: Wait and retry (default: 300 req/min for Chat API)

---

## Complementary Skills

| Skill | Use For |
|-------|---------|
| `research-lookup` | Academic paper searches (routes to Perplexity for scholarly queries) |
| `citation-management` | Google Scholar, PubMed, CrossRef database searches |
| `literature-review` | Systematic literature reviews across academic databases |
| `scientific-schematics` | Generate diagrams from research findings |
