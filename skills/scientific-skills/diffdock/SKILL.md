---
name: diffdock
description: Diffusion-based molecular docking. Predict protein-ligand binding poses from PDB/SMILES, confidence scores, virtual screening, for structure-based drug design. Not for affinity prediction.
license: MIT license
tags: [protein-ligand-docking, binding-pose-prediction, diffusion-docking, virtual-screening, diffdock]
metadata:
    skill-author: K-Dense Inc.
---------|------------------|----------------|
| **> 0** | High | Strong prediction, likely accurate |
| **-1.5 to 0** | Moderate | Reasonable prediction, validate carefully |
| **< -1.5** | Low | Uncertain prediction, requires validation |

**Critical Notes:**
1. **Confidence ≠ Affinity**: High confidence means model certainty about structure, NOT strong binding
2. **Context Matters**: Adjust expectations for:
   - Large ligands (>500 Da): Lower confidence expected
   - Multiple protein chains: May decrease confidence
   - Novel protein families: May underperform
3. **Multiple Samples**: Review top 3-5 predictions, look for consensus

**For detailed guidance:** Read `references/confidence_and_limitations.md` using the Read tool

## Parameter Customization

### Using Custom Configuration

Create custom configuration for specific use cases:

```bash
# Copy template
cp assets/custom_inference_config.yaml my_config.yaml

# Edit parameters (see template for presets)
# Then run with custom config
python -m inference \
  --config my_config.yaml \
  --protein_ligand_csv input.csv \
  --out_dir results/
```

### Key Parameters to Adjust

**Sampling Density:**
- `samples_per_complex: 10` → Increase to 20-40 for difficult cases
- More samples = better coverage but longer runtime

**Inference Steps:**
- `inference_steps: 20` → Increase to 25-30 for higher accuracy
- More steps = potentially better quality but slower

**Temperature Parameters (control diversity):**
- `temp_sampling_tor: 7.04` → Increase for flexible ligands (8-10)
- `temp_sampling_tor: 7.04` → Decrease for rigid ligands (5-6)
- Higher temperature = more diverse poses

**Presets Available in Template:**
1. High Accuracy: More samples + steps, lower temperature
2. Fast Screening: Fewer samples, faster
3. Flexible Ligands: Increased torsion temperature
4. Rigid Ligands: Decreased torsion temperature

**For complete parameter reference:** Read `references/parameters_reference.md` using the Read tool

## Advanced Techniques

### Ensemble Docking (Protein Flexibility)

For proteins with known flexibility, dock to multiple conformations:

```python
# Create ensemble CSV
import pandas as pd

conformations = ["conf1.pdb", "conf2.pdb", "conf3.pdb"]
ligand = "CC(=O)Oc1ccccc1C(=O)O"

data = {
    "complex_name": [f"ensemble_{i}" for i in range(len(conformations))],
    "protein_path": conformations,
    "ligand_description": [ligand] * len(conformations),
    "protein_sequence": [""] * len(conformations)
}

pd.DataFrame(data).to_csv("ensemble_input.csv", index=False)
```

Run docking with increased sampling:
```bash
python -m inference \
  --config default_inference_args.yaml \
  --protein_ligand_csv ensemble_input.csv \
  --samples_per_complex 20 \
  --out_dir results/ensemble/
```

### Integration with Scoring Functions

DiffDock generates poses; combine with other tools for affinity:

**GNINA (Fast neural network scoring):**
```bash
for pose in results/*.sdf; do
    gnina -r protein.pdb -l "$pose" --score_only
done
```

**MM/GBSA (More accurate, slower):**
Use AmberTools MMPBSA.py or gmx_MMPBSA after energy minimization

**Free Energy Calculations (Most accurate):**
Use OpenMM + OpenFE or GROMACS for FEP/TI calculations

**Recommended Workflow:**
1. DiffDock → Generate poses with confidence scores
2. Visual inspection → Check structural plausibility
3. GNINA or MM/GBSA → Rescore and rank by affinity
4. Experimental validation → Biochemical assays

## Limitations and Scope

**DiffDock IS Designed For:**
- Small molecule ligands (typically 100-1000 Da)
- Drug-like organic compounds
- Small peptides (<20 residues)
- Single or multi-chain proteins

**DiffDock IS NOT Designed For:**
- Large biomolecules (protein-protein docking) → Use DiffDock-PP or AlphaFold-Multimer
- Large peptides (>20 residues) → Use alternative methods
- Covalent docking → Use specialized covalent docking tools
- Binding affinity prediction → Combine with scoring functions
- Membrane proteins → Not specifically trained, use with caution

**For complete limitations:** Read `references/confidence_and_limitations.md` using the Read tool

## Troubleshooting

### Common Issues

**Issue: Low confidence scores across all predictions**
- Cause: Large/unusual ligands, unclear binding site, protein flexibility
- Solution: Increase `samples_per_complex` (20-40), try ensemble docking, validate protein structure

**Issue: Out of memory errors**
- Cause: GPU memory insufficient for batch size
- Solution: Reduce `--batch_size 2` or process fewer complexes at once

**Issue: Slow performance**
- Cause: Running on CPU instead of GPU
- Solution: Verify CUDA with `python -c "import torch; print(torch.cuda.is_available())"`, use GPU

**Issue: Unrealistic binding poses**
- Cause: Poor protein preparation, ligand too large, wrong binding site
- Solution: Check protein for missing residues, remove far waters, consider specifying binding site

**Issue: "Module not found" errors**
- Cause: Missing dependencies or wrong environment
- Solution: Run `python scripts/setup_check.py` to diagnose

### Performance Optimization

**For Best Results:**
1. Use GPU (essential for practical use)
2. Pre-compute ESM embeddings for repeated protein use
3. Batch process multiple complexes together
4. Start with default parameters, then tune if needed
5. Validate protein structures (resolve missing residues)
6. Use canonical SMILES for ligands

## Graphical User Interface

For interactive use, launch the web interface:

```bash
python app/main.py
# Navigate to http://localhost:7860
```

Or use the online demo without installation:
- https://huggingface.co/spaces/reginabarzilaygroup/DiffDock-Web

## Resources

### Helper Scripts (`scripts/`)

**`prepare_batch_csv.py`**: Create and validate batch input CSV files
- Create templates with example entries
- Validate file paths and SMILES strings
- Check for required columns and format issues

**`analyze_results.py`**: Analyze confidence scores and rank predictions
- Parse results from single or batch runs
- Generate statistical summaries
- Export to CSV for downstream analysis
- Identify top predictions across complexes

**`setup_check.py`**: Verify DiffDock environment setup
- Check Python version and dependencies
- Verify PyTorch and CUDA availability
- Test RDKit and PyTorch Geometric installation
- Provide installation instructions if needed

### Reference Documentation (`references/`)

**`parameters_reference.md`**: Complete parameter documentation
- All command-line options and configuration parameters
- Default values and acceptable ranges
- Temperature parameters for controlling diversity
- Model checkpoint locations and version flags

Read this file when users need:
- Detailed parameter explanations
- Fine-tuning guidance for specific systems
- Alternative sampling strategies

**`confidence_and_limitations.md`**: Confidence score interpretation and tool limitations
- Detailed confidence score interpretation
- When to trust predictions
- Scope and limitations of DiffDock
- Integration with complementary tools
- Troubleshooting prediction quality

Read this file when users need:
- Help interpreting confidence scores
- Understanding when NOT to use DiffDock
- Guidance on combining with other tools
- Validation strategies

**`workflows_examples.md`**: Comprehensive workflow examples
- Detailed installation instructions
- Step-by-step examples for all workflows
- Advanced integration patterns
- Troubleshooting common issues
- Best practices and optimization tips

Read this file when users need:
- Complete workflow examples with code
- Integration with GNINA, OpenMM, or other tools
- Virtual screening workflows
- Ensemble docking procedures

### Assets (`assets/`)

**`batch_template.csv`**: Template for batch processing
- Pre-formatted CSV with required columns
- Example entries showing different input types
- Ready to customize with actual data

**`custom_inference_config.yaml`**: Configuration template
- Annotated YAML with all parameters
- Four preset configurations for common use cases
- Detailed comments explaining each parameter
- Ready to customize and use

## Best Practices

1. **Always verify environment** with `setup_check.py` before starting large jobs
2. **Validate batch CSVs** with `prepare_batch_csv.py` to catch errors early
3. **Start with defaults** then tune parameters based on system-specific needs
4. **Generate multiple samples** (10-40) for robust predictions
5. **Visual inspection** of top poses before downstream analysis
6. **Combine with scoring** functions for affinity assessment
7. **Use confidence scores** for initial ranking, not final decisions
8. **Pre-compute embeddings** for virtual screening campaigns
9. **Document parameters** used for reproducibility
10. **Validate results** experimentally when possible

## Citations

When using DiffDock, cite the appropriate papers:

**DiffDock-L (current default model):**
```
Stärk et al. (2024) "DiffDock-L: Improving Molecular Docking with Diffusion Models"
arXiv:2402.18396
```

**Original DiffDock:**
```
Corso et al. (2023) "DiffDock: Diffusion Steps, Twists, and Turns for Molecular Docking"
ICLR 2023, arXiv:2210.01776
```

## Additional Resources

- **GitHub Repository**: https://github.com/gcorso/DiffDock
- **Online Demo**: https://huggingface.co/spaces/reginabarzilaygroup/DiffDock-Web
- **DiffDock-L Paper**: https://arxiv.org/abs/2402.18396
- **Original Paper**: https://arxiv.org/abs/2210.01776

