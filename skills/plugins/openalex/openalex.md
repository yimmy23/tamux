---
name: openalex
description: >
  Query the OpenAlex scholarly database for research papers (works), authors,
  institutions, topics, sources, publishers, funders, keywords, and more. Use
  when searching academic papers, resolving DOIs, downloading open-access PDFs,
  finding an author's publications, aggregating bibliometric data (citation
  counts, h-index, impact factor), exploring the research taxonomies, or
  performing DOI lookups. Backed by the public OpenAlex REST API
  (https://api.openalex.org/) — anonymous by default, optional API key raises
  rate limit.
---

# OpenAlex Plugin

Use the **openalex** plugin for scholarly literature queries. The plugin
wraps the deepmind `openalex_cli.py` script.

## Auth (optional)

`OPENALEX_API_KEY` setting is **optional**. Without a key, OpenAlex allows
~10 req/s and tags you as the "polite pool" if you supply a `mailto=` in
your User-Agent. With a key, you get a higher rate limit.

The script loads the key from `~/.env` via `dotenv` (it picks up
`OPENALEX_API_KEY` automatically). The plugin **never surfaces the key in
the agent context**.

## Entity types

The OpenAlex API has 15 entity types:

- `works` (research papers) — most common
- `authors`
- `sources` (journals / repositories)
- `institutions`
- `topics` (research topics)
- `domains`, `fields`, `subfields` (research taxonomy)
- `sdgs` (UN Sustainable Development Goals)
- `countries`, `continents`, `languages`
- `keywords`
- `publishers`, `funders`

The `entity_type` argument to `resolve`, `get`, and `filter` accepts any of
these.

## Commands

### `/openalex.rate-limit`

Check the current rate-limit status (remaining requests, reset time).

No env vars required.

### `/openalex.resolve`

Resolve an entity name to its OpenAlex ID.

Required env: `OA_ENTITY_TYPE`, `OA_QUERY`.
Optional: `OA_PER_PAGE` (default 10).

Example — find an author:

```bash
OA_ENTITY_TYPE=authors OA_QUERY="Yann LeCun" \
/openalex.resolve
```

### `/openalex.get`

Get a single entity by its OpenAlex ID.

Required env: `OA_ENTITY_TYPE`, `OA_ID`.
Optional: `OA_SELECT` (comma-separated list of fields).

Example — fetch a work's metadata:

```bash
OA_ENTITY_TYPE=works OA_ID=W2741809807 OA_SELECT="id,title,doi,cited_by_count" \
/openalex.get
```

### `/openalex.filter`

Filter and search entities. The workhorse of OpenAlex queries.

Required env: `OA_ENTITY_TYPE`.
Common optional: `OA_SEARCH` (full-text), `OA_FILTER` (boolean filter
string), `OA_SORT`, `OA_GROUP_BY`, `OA_PER_PAGE`, `OA_PAGE`, `OA_SELECT`.

Example — top-cited recent papers on AlphaFold:

```bash
OA_ENTITY_TYPE=works \
OA_SEARCH="AlphaFold" \
OA_FILTER="publication_year:2020-2026,is_oa:true" \
OA_SORT="cited_by_count:desc" \
OA_PER_PAGE=10 \
/openalex.filter
```

### `/openalex.download-pdf` ⚠ **$0.01 per request**

Download the open-access PDF for a work. **Always confirm with the user
before invoking** — this is a real per-request charge billed by OpenAlex.

Required env: `OA_ID` (Work ID), `OA_OUTPUT` (output file path).

### `/openalex.run`

Generic catch-all. Forwards an arbitrary `openalex_cli.py` invocation.
Currently all 5 subcommands are exposed as named commands, so this is
forward-compat and for full-arg-passing in complex queries.

Required env: `OA_SUBCOMMAND`. Other args pass through.

## OpenAlex filter syntax

The `OA_FILTER` env var accepts the OpenAlex native filter language:

- Equality: `is_oa:true`
- Range: `publication_year:2020-2026`
- Comparison: `cited_by_count:>100`
- Multiple filters (AND): `is_oa:true,cited_by_count:>100`
- OR via pipe: `type:journal|type:repository`
- Negation: `not type:dataset`

Combine with `OA_SEARCH` (full-text) for hybrid queries. For complex
bibliometric aggregations, use `OA_GROUP_BY` (e.g. `OA_GROUP_BY=primary_topic.id`).

## Limits

- Without key: ~10 req/s.
- With key: 100 req/s.
- Page size cap: 200 results per page. Use `OA_PAGE` for pagination, or
  `OA_PER_PAGE=200` to minimize round-trips.

## License

Plugin manifest + this skill file: MIT.
Vendored scripts: Apache 2.0.
OpenAlex data terms: <https://docs.openalex.org/api-guide-for-data-users/terms>.
PDF download billing: <https://docs.openalex.org/api-guide-for-data-users/download-pdfs>.
