---
name: glycoengineering
description: Analyze and engineer protein glycosylation. Scan sequences for N-glycosylation sequons (N-X-S/T), predict O-glycosylation hotspots, and access curated glycoengineering tools (NetOGlyc, GlycoShield, GlycoWorkbench). For glycoprotein engineering, therapeutic antibody optimization, and vaccine design.
license: Unknown
tags: [scientific-skills, glycoengineering, experimental-design, performance]
metadata:
    skill-author: Kuan-lin Huang
---|----------|-------|
| Enhance ADCC | Defucosylation at Fc Asn297 | Afucosylated IgG1 has ~50× better FcγRIIIa binding |
| Reduce immunogenicity | Remove non-human glycans | Eliminate α-Gal, NGNA epitopes |
| Improve PK half-life | Sialylation | Sialylated glycans extend half-life |
| Reduce inflammation | Hypersialylation | IVIG anti-inflammatory mechanism |
| Create glycan shield | Add N-glycosites to surface | Masks vulnerable epitopes (vaccine design) |

### Common Mutations Used

| Mutation | Effect |
|----------|--------|
| N297A/Q (IgG1) | Removes Fc glycosylation (aglycosyl) |
| N297D (IgG1) | Removes Fc glycosylation |
| S298A/E333A/K334A | Increases FcγRIIIa binding |
| F243L (IgG1) | Increases defucosylation |
| T299A | Removes Fc glycosylation |

## Glycan Notation

### IUPAC Condensed Notation (Monosaccharide abbreviations)

| Symbol | Full Name | Type |
|--------|-----------|------|
| Glc | Glucose | Hexose |
| GlcNAc | N-Acetylglucosamine | HexNAc |
| Man | Mannose | Hexose |
| Gal | Galactose | Hexose |
| Fuc | Fucose | Deoxyhexose |
| Neu5Ac | N-Acetylneuraminic acid (Sialic acid) | Sialic acid |
| GalNAc | N-Acetylgalactosamine | HexNAc |

### Complex N-Glycan Structure

```
Typical complex biantennary N-glycan:
Neu5Ac-Gal-GlcNAc-Man\
                       Man-GlcNAc-GlcNAc-[Asn]
Neu5Ac-Gal-GlcNAc-Man/
(±Core Fuc at innermost GlcNAc)
```

## Best Practices

- **Start with NetNGlyc/NetOGlyc** for computational prediction before experimental validation
- **Verify with mass spectrometry**: Glycoproteomics (Byonic, Mascot) for site-specific glycan profiling
- **Consider site context**: Not all predicted sequons are actually glycosylated (accessibility, cell type, protein conformation)
- **For antibodies**: Fc N297 glycan is critical — always characterize this site first
- **Use GlyConnect** to check if your protein of interest has experimentally verified glycosylation data

## Additional Resources

- **GlyTouCan** (glycan structure repository): https://glytoucan.org/
- **GlyConnect**: https://glyconnect.expasy.org/
- **CFG Functional Glycomics**: http://www.functionalglycomics.org/
- **DTU Health Tech servers** (NetNGlyc, NetOGlyc): https://services.healthtech.dtu.dk/
- **GlycoWorkbench**: https://glycoworkbench.software.informer.com/
- **Review**: Apweiler R et al. (1999) Biochim Biophys Acta. PMID: 10564035
- **Therapeutic glycoengineering review**: Jefferis R (2009) Nature Reviews Drug Discovery. PMID: 19448661
