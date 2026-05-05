---
name: phylogenetics
description: Build and analyze phylogenetic trees using MAFFT (multiple alignment), IQ-TREE 2 (maximum likelihood), and FastTree (fast NJ/ML). Visualize with ETE3 or FigTree. For evolutionary analysis, microbial genomics, viral phylodynamics, protein family analysis, and molecular clock studies.
license: Unknown
tags: [scientific-skills, phylogenetics, cheminformatics, machine-learning, bioinformatics]
metadata:
    skill-author: Kuan-lin Huang
----|-------------|---------|
| `GTR+G4` | General Time Reversible + Gamma | Most flexible DNA model |
| `HKY+G4` | Hasegawa-Kishino-Yano + Gamma | Two-rate model (common) |
| `TrN+G4` | Tamura-Nei | Unequal transitions |
| `JC` | Jukes-Cantor | Simplest; all rates equal |

### Protein Models

| Model | Description | Use case |
|-------|-------------|---------|
| `LG+G4` | Le-Gascuel + Gamma | Best average protein model |
| `WAG+G4` | Whelan-Goldman | Widely used |
| `JTT+G4` | Jones-Taylor-Thornton | Classical model |
| `Q.pfam+G4` | pfam-trained | For Pfam-like protein families |
| `Q.bird+G4` | Bird-specific | Vertebrate proteins |

**Tip:** Use `-m TEST` to let IQ-TREE automatically select the best model.

## Best Practices

- **Alignment quality first**: Poor alignment → unreliable trees; check alignment manually
- **Use `linsi` for small (<200 seq), `fftns` or `auto` for large alignments**
- **Model selection**: Always use `-m TEST` for IQ-TREE unless you have a specific reason
- **Bootstrap**: Use ≥1000 ultrafast bootstraps (`-B 1000`) for branch support
- **Root the tree**: Unrooted trees can be misleading; use outgroup or midpoint rooting
- **FastTree for >5000 sequences**: IQ-TREE becomes slow; FastTree is 10–100× faster
- **Trim long alignments**: TrimAl removes unreliable columns; improves tree accuracy
- **Check for recombination** in viral/bacterial sequences before building trees (`RDP4`, `GARD`)

## Additional Resources

- **MAFFT**: https://mafft.cbrc.jp/alignment/software/
- **IQ-TREE 2**: http://www.iqtree.org/ | Tutorial: https://www.iqtree.org/workshop/molevol2022
- **FastTree**: http://www.microbesonline.org/fasttree/
- **ETE3**: http://etetoolkit.org/
- **FigTree** (GUI visualization): https://tree.bio.ed.ac.uk/software/figtree/
- **iTOL** (web visualization): https://itol.embl.de/
- **MUSCLE** (alternative aligner): https://www.drive5.com/muscle/
- **TrimAl** (alignment trimming): https://vicfero.github.io/trimal/
