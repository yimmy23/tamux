---
name: scientific-database-lookup-task
description: Use for public database lookups across biomedical, chemistry, genomics, materials, economics, patents, clinical trials, or regulatory data.
recommended_skills:
  - database-lookup
  - paper-lookup
  - research-lookup
  - clinical-decision-support
  - usfiscaldata
---

# Scientific Database Lookup Task Guideline

Database lookup work should preserve identifiers, provenance, and query parameters.

## Workflow

1. Identify the entity type, identifier system, target databases, required fields, and output schema.
2. Use `database-lookup` for public scientific, biomedical, materials, regulatory, economic, and demographics databases.
3. Use `paper-lookup` when database records must be linked to papers, DOIs, PubMed IDs, or citation graphs.
4. Preserve raw identifiers and map synonyms only when the mapping source is explicit.
5. Return source database, query terms, accession IDs, timestamps, and confidence or ambiguity notes.
6. Avoid merging records from different databases unless the join key and assumptions are clear.

## Quality Gate

Do not collapse similarly named genes, compounds, trials, companies, or datasets without verifying identifiers.
