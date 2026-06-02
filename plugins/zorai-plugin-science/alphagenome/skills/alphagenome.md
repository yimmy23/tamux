---
name: alphagenome
description: >
  Use when the user asks about non-coding variant effects, pathogenicity, clinical
  significance, disease associations, functional effects, gene expression changes,
  splicing disruption, or regulatory effects in promoters and enhancers. Also use
  for resolving biological terms to tissue/cell-type ontologies (UBERON / CL / EFO)
  or analyzing variants in chr:pos:ref>alt format. Backed by the DeepMind
  AlphaGenome API. Each invocation is a real Google Cloud inference — confirm
  with the user before looping over many variants.
---

# AlphaGenome Plugin

Use the **alphagenome** plugin for single-variant effect analysis on the
DeepMind AlphaGenome service. All commands invoke the deepmind Python scripts
vendored in this plugin's `scripts/` directory.

## Auth

- `ALPHAGENOME_API_KEY` setting is required. Get one at
  <https://deepmind.google.com/science/alphagenome/>. The plugin **never
  surfaces the key in the agent context** — scripts load it via `dotenv`.

## Variants

Variants are expressed as four flags: `--chrom`, `--pos`, `--ref`, `--alt`
(e.g. `chr21:46126238:G>C` becomes `--chrom chr21 --pos 46126238 --ref G --alt C`).
The plugin forwards them as `AG_CHROM` / `AG_POS` / `AG_REF` / `AG_ALT` env vars.

## Commands

### `/alphagenome.visualize-variant-effects`

Visualize REF vs ALT allele effects across modalities (expression, regulatory,
splicing) for a single variant. Produces matplotlib outputs.

Required env: `AG_CHROM AG_POS AG_REF AG_ALT AG_GENE AG_ONTOLOGY`.
Optional: `AG_TISSUE AG_TRACKS AG_TF AG_VIEW AG_OUTPUT_DIR` (default `./ag_output`).

Example:

```bash
AG_CHROM=chr21 AG_POS=46126238 AG_REF=G AG_ALT=C \
AG_GENE=COL6A2 AG_ONTOLOGY=UBERON:0001134 \
AG_TISSUE=muscle AG_TRACKS=splicing \
/alphagenome.visualize-variant-effects
```

### `/alphagenome.interpret-splicing`

Quantitative splicing analysis (delta scores, junctions) for a single variant.

Required env: `AG_CHROM AG_POS AG_REF AG_ALT AG_GENE AG_ONTOLOGY`.
Optional: `AG_OUTPUT_DIR`.

### `/alphagenome.analyze-ism`

In-Silico Mutagenesis SeqLogo generation for a variant and tissue.

Required env: `AG_CHROM AG_POS AG_REF AG_ALT AG_TISSUE AG_ONTOLOGY`.
Optional: `AG_MODALITY` (default `DNASE`), `AG_OUTPUT_DIR`.

### `/alphagenome.lookup-gene-info`

Comprehensive gene and transcript lookup using local GTF data (offline — no
external API). Useful before scoring to confirm the gene exists in the local
GTF and to discover transcript IDs.

Required env: `AG_GENE`. Optional: `AG_TRANSCRIPT`, `AG_FEATURE`.

### `/alphagenome.resolve-ontology-terms`

Resolve biological terms to ontology CURIEs. Accepts a comma-separated list.

Required env: `AG_TERMS` (e.g. `"liver,CD8 T cell,hepatocyte"`).
Optional: `AG_OUTPUT_DIR`.

### `/alphagenome.generate-ontology-mapping`

Build a custom ontology mapping file from a set of tissue/cell-type terms.

Required env: `AG_INPUT` (path to a JSON file of terms).
Optional: `AG_OUTPUT_DIR` (default `./ag_output`).

### `/alphagenome.visualize-genome-tracks`

Region-level (no variant) genome track visualization.

Required env: `AG_CHROM AG_START AG_END AG_ONTOLOGY`. Optional: `AG_OUTPUT_DIR`.

## Cost / rate limit

Each `score_variant` / `predict_variant` call is a real Google Cloud
inference. **Always confirm with the user before invoking in a loop** (e.g.
for VEP-style batch annotation). The deepmind SKILL.md describes a
multi-variant workflow that spawns sub-agents — that pattern is rate-limited
by upstream, not the plugin.

## Workflow

The full AlphaGenome analysis workflow is in
`skills/scientific-skills-gdm/alphagenome_single_variant_analysis/SKILL.md`.
Steps (abbreviated):

1. **Review golden examples** under `examples/splicing/`, `examples/regulatory/`,
   `examples/negative_result_GATA4/`, etc. Pick the closest match.
2. **Resolve tissues and modalities** via `/alphagenome.resolve-ontology-terms`.
3. **Visualize** with `/alphagenome.visualize-variant-effects` and read the
   outputs.
4. **Interpret** using `docs/interpretation-guide.md` (read it before
   claiming significance).
5. **Write a report** to `report.md` using the templates in
   `docs/report-templates.md`.
6. **Self-critique** by re-reading `report.md` and verifying every link
   and claim.

## License

Plugin manifest + this skill file: MIT.
Vendored scripts: Apache 2.0 (see `scripts/*.py` headers).
AlphaGenome API terms: <https://deepmind.google.com/science/alphagenome/>.
