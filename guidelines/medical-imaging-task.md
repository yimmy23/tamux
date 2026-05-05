---
name: medical-imaging-task
description: "Use when working with medical images: CT, MRI, X-ray, ultrasound. Covers DICOM handling, NIfTI, preprocessing, segmentation, registration, and AI model inference."
recommended_skills:
  - monai
  - dicom-pipeline
  - nibabel
  - pydicom
  - nnunet
recommended_guidelines:
  - clinical-research-task
  - scientific-data-analysis-task
---

## Overview

Medical imaging analysis requires understanding image formats, coordinate systems, modality-specific preprocessing, and validation against clinical ground truth. This guideline orchestrates imaging workflows.

## Workflow

1. Ingest images: determine modality (CT, MRI, US, XA), format (DICOM, NIfTI, PNG), and number of series.
2. Use `dicom-pipeline` for DICOM parsing, anonymization, and DICOMweb queries.
3. Use `nibabel` for NIfTI/GIFTI/CIFTI neuroimaging data.
4. Normalize: check orientation (RAS/LAS), spacing, intensity ranges, and field of view before any processing.
5. Preprocess: resample to isotropic spacing, clip intensity outliers, normalize to [0,1] or z-score.
6. Segment using `monai` for deep learning (UNet, UNETR, SwinUNETR) or `nnunet` for self-configuring segmentation.
7. Register images using `monai` or SimpleITK when multi-timepoint or multi-modality alignment is needed.
8. Evaluate: Dice, Hausdorff distance, volume agreement against reference standard.
9. Document preprocessing parameters, model version, and evaluation metrics.

## Quality Gate

Medical imaging analysis is complete when preprocessing is documented, segmentation quality is quantified, and results are clinically interpretable.