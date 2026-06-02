---
name: dbsnp-database
description: >
  Long-tail stub for the deepmind `dbsnp_database` skill. Use when you want to look up, map, and search for short genetic variants (SNPs, indels) in NCBI's dbSNP database. Resolves between rsIDs, genomic coordinates in VCF format, and HGVS strings. For an rsID, returns variant type, gene associations, clinical significance, allele frequencies, and genomic coordinates (GRCh38).
  For the full workflow, read
  `skills/scientific-skills-gdm/dbsnp_database/SKILL.md` in the repo.
---

# dbsnp-database (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/dbsnp_database/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `/dbsnp-database.run` command that forwards to the deepmind
Python entry script (`scripts/dbsnp_cli.py`).

```bash
DBSNP_DATABASE_ARGS="<deepmind-subcommand-and-its-flags>" \
/dbsnp-database.run
```

Replace `<deepmind-subcommand-and-its-flags>` with the subcommand and flags
described in the upstream skill.

## Available subcommands (from the deepmind script)

- `get-variant`
- `resolve-hgvs`
- `resolve-rsid`
- `resolve-variant`
- `search-region`

## Auth

See `skills/scientific-skills-gdm/dbsnp_database/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
