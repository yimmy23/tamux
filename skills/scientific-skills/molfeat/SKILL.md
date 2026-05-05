---
name: molfeat
description: Molecular featurization for ML (100+ featurizers). ECFP, MACCS, descriptors, pretrained models (ChemBERTa), convert SMILES to features, for QSAR and molecular ML.
license: Apache-2.0 license
tags: [scientific-skills, molfeat, machine-learning, cheminformatics]
metadata:
    skill-author: K-Dense Inc.
---------|------|------------|-------|----------|
| `ecfp` | Fingerprint | 2048 | Fast | General purpose |
| `maccs` | Fingerprint | 167 | Very fast | Scaffold similarity |
| `desc2D` | Descriptors | 200+ | Fast | Interpretable models |
| `mordred` | Descriptors | 1800+ | Medium | Comprehensive features |
| `map4` | Fingerprint | 1024 | Fast | Large-scale screening |
| `ChemBERTa-77M-MLM` | Deep learning | 768 | Slow* | Transfer learning |
| `gin-supervised-masking` | GNN | Variable | Slow* | Graph-based models |

*First run is slow; subsequent runs benefit from caching

## Resources

This skill includes comprehensive reference documentation:

### references/api_reference.md
Complete API documentation covering:
- `molfeat.calc` - All calculator classes and parameters
- `molfeat.trans` - Transformer classes and methods
- `molfeat.store` - ModelStore usage
- Common patterns and integration examples
- Performance optimization tips

**When to load:** Reference when implementing specific calculators, understanding transformer parameters, or integrating with scikit-learn/PyTorch.

### references/available_featurizers.md
Comprehensive catalog of all 100+ featurizers organized by category:
- Transformer-based language models (ChemBERTa, ChemGPT)
- Graph neural networks (GIN, Graphormer)
- Molecular descriptors (RDKit, Mordred)
- Fingerprints (ECFP, MACCS, MAP4, and 15+ others)
- Pharmacophore descriptors (CATS, Gobbi)
- Shape descriptors (USR, ElectroShape)
- Scaffold-based descriptors

**When to load:** Reference when selecting the optimal featurizer for a specific task, exploring available options, or understanding featurizer characteristics.

**Search tip:** Use grep to find specific featurizer types:
```bash
grep -i "chembert" references/available_featurizers.md
grep -i "pharmacophore" references/available_featurizers.md
```

### references/examples.md
Practical code examples for common scenarios:
- Installation and quick start
- Calculator and transformer examples
- Pretrained model usage
- Scikit-learn and PyTorch integration
- Virtual screening workflows
- QSAR model building
- Similarity searching
- Troubleshooting and best practices

**When to load:** Reference when implementing specific workflows, troubleshooting issues, or learning molfeat patterns.

## Troubleshooting

### Invalid Molecules
Enable error handling to skip invalid SMILES:
```python
transformer = MoleculeTransformer(
    calc,
    ignore_errors=True,
    verbose=True
)
```

### Memory Issues with Large Datasets
Process in chunks or use streaming approaches for datasets > 100K molecules.

### Pretrained Model Dependencies
Some models require additional packages. Install specific extras:
```bash
uv pip install "molfeat[transformer]"  # For ChemBERTa/ChemGPT
uv pip install "molfeat[dgl]"          # For GIN models
```

### Reproducibility
Save exact configurations and document versions:
```python
transformer.to_state_yaml_file("config.yml")
import molfeat
print(f"molfeat version: {molfeat.__version__}")
```

## Additional Resources

- **Official Documentation**: https://molfeat-docs.datamol.io/
- **GitHub Repository**: https://github.com/datamol-io/molfeat
- **PyPI Package**: https://pypi.org/project/molfeat/
- **Tutorial**: https://portal.valencelabs.com/datamol/post/types-of-featurizers-b1e8HHrbFMkbun6

