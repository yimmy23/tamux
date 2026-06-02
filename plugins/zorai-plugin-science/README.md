# zorai-plugin-science

First-class zorai integration for the
[google-deepmind/science-skills](https://github.com/google-deepmind/science-skills)
bundle (vendored in-repo at `skills/scientific-skills-gdm/`). Ships **11
full sub-plugins** and **23 long-tail stub sub-plugins** from one npm
package.

## Full sub-plugins (hand-written, offline-mock-tested)

| Sub-plugin | Surface | Auth | Notes |
|---|---|---|---|
| `alphagenome` | Managed Python | `ALPHAGENOME_API_KEY` | Single-variant effect scoring, splicing, ontology resolution. Compute-heavy. |
| `alphafold-database` | Managed Python | none | Fetch predicted structures by UniProt ID, analyze pLDDT / PAE. |
| `uniprot` | Managed Python | none | Protein metadata, function, taxonomy, sequences across UniProtKB/UniParc/UniRef. |
| `clinvar` | Managed Python | optional `CLINVAR_API_KEY` (raises NCBI rate limit) | Pathogenicity classifications, clinical significance, evidence rationales. |
| `chembl` | Managed Python | none | Bioactive molecules, drug targets, IC50/Ki, structures. |
| `openalex` | Managed Python | optional `OPENALEX_API_KEY` (raises rate limit) | Scholarly works / authors / institutions / topics; bibliometrics; OA-PDF download (**$0.01 per request**). |
| `ensembl` | Managed Python | none | Gene/transcript/protein ID resolution, cross-references, sequence retrieval, gene structure, Variant Effect Predictor (VEP). The primary ID translator in genomics. |
| `reactome` | Managed Python | none | Pathway analysis, gene list enrichment, reaction participants, pathway hierarchy, diagram export, knowledgebase search. |
| `gnomad` | Managed Python | none | Allele frequencies (pLoF, missense), gene constraint (pLI, LOEUF), variant search by gene/region. Strict 10 req/min rate limit. |
| `pdb-database` | Managed Python | none | RCSB Protein Data Bank — search, sequence/structure/chemical similarity, download coordinate files (mmCIF/PDB), GraphQL metadata. Pairs with `alphafold-database` (predicted vs experimental). |
| `ncbi-sequence-fetch` | Managed Python | optional `NCBI_API_KEY` (raises E-utilities rate limit 3→10 req/s) | NCBI E-utilities — fetch protein/nucleotide by accession, gene+organism search, CDS→protein translation, cross-database linking. The central sequence-retrieval tool. |

## Long-tail stub sub-plugins (auto-generated, AST-tested only)

These 23 skills are exposed as stubs so they show up in `zorai plugin ls`
and the agent can invoke them via the standard pattern:

```bash
SUB_PLUGIN_ARGS="<deepmind-subcommand-and-its-flags>" \
/<sub-plugin>.run
```

The stub forwards through Zorai's managed Python runner as
`python scripts/<entry-script>.py <args>`. The
canonical workflow lives in `skills/scientific-skills-gdm/<skill>/SKILL.md`
in the repo — read that first. Stub plugin surface is auto-generated
by `tools/generate_longtail_stubs.py` from the deepmind bundle.

| Sub-plugin | Domain | Deepmind entry script |
|---|---|---|
| `clinical-trials-database` | Clinical trials | `clinical_trials_api.py` |
| `dbsnp-database` | SNP lookup | `dbsnp_cli.py` |
| `embl-ebi-ols` | Ontology lookup | `get_individual.py` |
| `encode-ccres-database` | Regulatory genomics | `encode_portal_api.py` |
| `foldseek-structural-search` | Structural search | `search.py` (needs foldseek binary) |
| `gtex-database` | Tissue expression | `gtex_cli.py` |
| `human-protein-atlas-database` | Tissue/subcellular localization | `hpa_cli.py` |
| `interpro-database` | Protein families/domains | `interpro_client.py` |
| `jaspar-database` | TF binding profiles | `jaspar_api.py` |
| `literature-search-arxiv` | arXiv preprints | `download_paper.py` |
| `literature-search-biorxiv` | bioRxiv preprints | `search_by_dates.py` |
| `literature-search-europepmc` | EuropePMC abstracts | `europepmc_api.py` |
| `openfda-database` | FDA adverse events / labels | `openfda_query.py` |
| `opentargets-database` | Drug-target associations | `query_opentargets.py` |
| `protein-sequence-msa` | Multiple sequence alignment | `msa_align.py` |
| `protein-sequence-similarity-search` | Sequence similarity (MMseqs2) | `mmseqs2_search.py` |
| `pubchem-database` | Chemical compounds | `pubchem_api.py` |
| `pubmed-database` | PubMed literature | `pubmed_api.py` |
| `pymol` | Structural visualization | _(no Python entry; uses pymol binary)_ |
| `quickgo-database` | GO annotations | `quickgo_tool.py` |
| `string-database` | Protein-protein interactions | `string_cli.py` |
| `ucsc-conservation-and-tfbs` | Conservation + TFBS | `get_conservation.py` |
| `unibind-database` | TF binding sites | `unibind_api.py` |

The remaining 32 sub-skills in the bundle (PubMed, Ensembl, OpenAlex, gnomAD,
Reactome, PDB, etc.) are reachable through the vendored
`skills/scientific-skills-gdm/` tree. The router skill at
`skills/scientific-skills-gdm/SKILL.md` lists all of them.

## Install

```bash
zorai plugin add ./plugins/zorai-plugin-science
```

All 34 sub-plugins are registered. Verify:

```bash
zorai plugin ls            # lists the full and long-tail science sub-plugins
zorai plugin commands      # one command per script (e.g. /alphagenome score-variant ...)
```

## Auth

Only `alphagenome` and (optionally) `clinvar` and `openalex` need API keys:

- **`alphagenome`** — required. Sign up at <https://deepmind.google.com/science/alphagenome/>,
  paste the key into the `ALPHAGENOME_API_KEY` setting. The plugin never
  echoes the key back into the agent's context.
- **`clinvar`** — optional. Without it, NCBI E-utilities rate-limits to ~3
  req/s. With it, the limit jumps to ~10 req/s. The plugin surfaces a
  `CLINVAR_API_KEY` setting; the deepmind script will pick it up via
  `dotenv` if you also write it to `~/.env`.
- **`openalex`** — optional. Without it, OpenAlex allows ~10 req/s and tags
  you as anonymous. With a key, you get a higher rate limit and the polite-
  pool header is set. Sign up at <https://openalex.org/>.

The remaining seven full sub-plugins (`alphafold-database`, `uniprot`, `chembl`, `ensembl`, `reactome`, `gnomad`, `pdb-database`)
hit public anonymous endpoints. Most stub sub-plugins also hit anonymous endpoints; check each
deepmind `SKILL.md` for the per-skill auth requirements.

## Commands

Each sub-plugin exposes one zorai command per deepmind script. See the
per-sub-plugin skill for the full invocation surface and parameter list:

- `plugins/zorai-plugin-science/alphagenome/skills/alphagenome.md`
- `plugins/zorai-plugin-science/alphafold-database/skills/alphafold-database.md`
- `plugins/zorai-plugin-science/uniprot/skills/uniprot.md`
- `plugins/zorai-plugin-science/clinvar/skills/clinvar.md`
- `plugins/zorai-plugin-science/chembl/skills/chembl.md`
- `plugins/zorai-plugin-science/openalex/skills/openalex.md`
- `plugins/zorai-plugin-science/ensembl/skills/ensembl.md`
- `plugins/zorai-plugin-science/reactome/skills/reactome.md`

The agent reads these skills automatically when the plugin is enabled.

## Cost / rate-limit warnings

- **`/alphagenome score-variant`** and **`/alphagenome predict-variant`**
  hit the AlphaGenome API. Each call is a real Google Cloud inference and
  may incur cost. **Always confirm with the user before invoking** if
  you're running a batch (looping over variants, etc.). The
  `alphagenome_single_variant_analysis` SKILL.md has a multi-variant
  workflow that spawns sub-agents — that pattern is rate-limited by the
  upstream, not the plugin.
- **`/openalex download-pdf`** — OpenAlex charges **$0.01 per PDF download**.
  **Always confirm with the user before invoking** in bulk.
- **`/clinvar *`** — NCBI E-utilities has a 3 req/s anonymous rate limit.
  The plugin does not enforce a delay; the deepmind script handles backoff.

## Layout

```text
plugins/zorai-plugin-science/
  package.json                # this npm package; files: [sub-plugin dirs]
  README.md                   # this file
  alphagenome/                # sub-plugin
    plugin.json               # settings, commands, skills
    skills/alphagenome.md     # agent-facing instructions
    scripts/                  # COPIES of deepmind scripts (sync via sync-from-bundle.sh)
    scienceskillscommon/      # local runtime package copied by nested install
  alphafold-database/         # sub-plugin
  uniprot/                    # sub-plugin
  clinvar/                    # sub-plugin
  chembl/                     # full sub-plugin
  openalex/                  # full sub-plugin
  ensembl/                   # full sub-plugin
  reactome/                  # full sub-plugin
  gnomad/                    # full sub-plugin
  clinical-trials-database/  # stub sub-plugin (auto-generated)
  dbsnp-database/            # ... 24 more stubs
  tools/
    generate_longtail_stubs.py  # regenerates the long-tail stub plugin.json + skills/<id>.md files
```

The deepmind `scripts/*.py` files are owned by the corresponding sub-plugin
(vendored at build time, refreshed via `sync-from-bundle.sh`). The
canonical source of truth is the bundle at
`skills/scientific-skills-gdm/<skill_name>/` — when the upstream bundle is
re-vendored, re-run the sync script from `plugins/zorai-plugin-science/`.

## License

This zorai plugin package: MIT.

Vendored deepmind scripts: Apache License 2.0 (see
`skills/scientific-skills-gdm/LICENSE` and the per-script header). Per-data-source
terms apply; see `skills/scientific-skills-gdm/SKILL_LICENSES.md`.
