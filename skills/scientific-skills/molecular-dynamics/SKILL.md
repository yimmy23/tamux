---
name: molecular-dynamics
description: Run and analyze molecular dynamics simulations with OpenMM and MDAnalysis. Set up protein/small molecule systems, define force fields, run energy minimization and production MD, analyze trajectories (RMSD, RMSF, contact maps, free energy surfaces). For structural biology, drug binding, and biophysics.
license: MIT
tags: [scientific-skills, molecular-dynamics, cheminformatics]
metadata:
    skill-author: Kuan-lin Huang
-----|------------------------|-------------|
| Standard proteins | AMBER14 (`amber14-all.xml`) | TIP3P-FB |
| Proteins + small molecules | AMBER14 + GAFF2 | TIP3P-FB |
| Membrane proteins | CHARMM36m | TIP3P |
| Nucleic acids | AMBER99-bsc1 or AMBER14 | TIP3P |
| Disordered proteins | ff19SB or CHARMM36m | TIP3P |

## System Preparation Tools

### PDBFixer (for raw PDB files)

```python
from pdbfixer import PDBFixer
from openmm.app import PDBFile

def fix_pdb(input_pdb, output_pdb, ph=7.0):
    """Fix common PDB issues: missing residues, atoms, add H, standardize."""
    fixer = PDBFixer(filename=input_pdb)
    fixer.findMissingResidues()
    fixer.findNonstandardResidues()
    fixer.replaceNonstandardResidues()
    fixer.removeHeterogens(True)    # Remove water/ligands
    fixer.findMissingAtoms()
    fixer.addMissingAtoms()
    fixer.addMissingHydrogens(ph)

    with open(output_pdb, 'w') as f:
        PDBFile.writeFile(fixer.topology, fixer.positions, f)

    return output_pdb
```

### GAFF2 for Small Molecules (via OpenFF Toolkit)

```python
# For ligand parameterization, use OpenFF toolkit or ACPYPE
# pip install openff-toolkit
from openff.toolkit import Molecule, ForceField as OFFForceField
from openff.interchange import Interchange

def parameterize_ligand(smiles, ff_name="openff-2.0.0.offxml"):
    """Generate GAFF2/OpenFF parameters for a small molecule."""
    mol = Molecule.from_smiles(smiles)
    mol.generate_conformers(n_conformers=1)

    off_ff = OFFForceField(ff_name)
    interchange = off_ff.create_interchange(mol.to_topology())
    return interchange
```

## Best Practices

- **Always minimize before MD**: Raw PDB structures have steric clashes
- **Equilibrate before production**: NVT (50–100 ps) → NPT (100–500 ps) → Production
- **Use GPU**: Simulations are 10–100× faster on GPU (CUDA/OpenCL)
- **2 fs timestep with HBonds constraints**: Standard; use 4 fs with HMR (hydrogen mass repartitioning)
- **Analyze only equilibrated trajectory**: Discard first 20–50% as equilibration
- **Save checkpoints**: MD runs can fail; checkpoints allow restart
- **Periodic boundary conditions**: Required for solvated systems
- **PME for electrostatics**: More accurate than cutoff methods for charged systems

## Additional Resources

- **OpenMM documentation**: https://openmm.org/documentation.html
- **MDAnalysis user guide**: https://docs.mdanalysis.org/
- **GROMACS** (alternative MD engine): https://manual.gromacs.org/
- **NAMD** (alternative): https://www.ks.uiuc.edu/Research/namd/
- **CHARMM-GUI** (web-based system builder): https://charmm-gui.org/
- **AmberTools** (free Amber tools): https://ambermd.org/AmberTools.php
- **OpenMM paper**: Eastman P et al. (2017) PLOS Computational Biology. PMID: 28278240
- **MDAnalysis paper**: Michaud-Agrawal N et al. (2011) J Computational Chemistry. PMID: 21500218
