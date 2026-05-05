---
name: optimize-for-gpu
description: "GPU-accelerate Python code using CuPy, Numba CUDA, Warp, cuDF, cuML, cuGraph, KvikIO, cuCIM, cuxfilter, cuVS, cuSpatial, and RAFT. Use whenever the user mentions GPU/CUDA/NVIDIA acceleration, or wants to speed up NumPy, pandas, scikit-learn, scikit-image, NetworkX, GeoPandas, or Faiss workloads. Covers physics simulation, differentiable rendering, mesh ray casting, particle systems (DEM/SPH/fluids), vector/similarity search, GPUDirect Storage file IO, interactive dashboards, geospatial analysis, medical imaging, and sparse eigensolvers. Also use when you see CPU-bound Python code (loops, large arrays, ML pipelines, graph analytics, image processing) that would benefit from GPU acceleration, even if not explicitly requested."
tags: [gpu-acceleration, cupy-numba-cuda, rapids-ecosystem, gpu-ml-pipelines, optimize-for-gpu]
metadata:
  author: K-Dense, Inc.
---|-------------|
| `references/cupy.md` | User has NumPy/SciPy code, or needs array operations on GPU |
| `references/numba.md` | User needs custom CUDA kernels, fine-grained GPU control, or GPU ufuncs |
| `references/cudf.md` | User has pandas code, or needs dataframe operations on GPU |
| `references/cuml.md` | User has scikit-learn code, or needs ML training/inference/preprocessing on GPU |
| `references/cugraph.md` | User has NetworkX code, or needs graph analytics on GPU |
| `references/warp.md` | User needs GPU simulation, spatial computing, mesh/volume queries, differentiable programming, or robotics |
| `references/kvikio.md` | User needs high-performance file IO to/from GPU, GPUDirect Storage, reading S3/HTTP to GPU, or Zarr on GPU |
| `references/cuxfilter.md` | User wants GPU-accelerated interactive dashboards, cross-filtering, or EDA visualization |
| `references/cucim.md` | User has scikit-image code, or needs image processing, digital pathology, or WSI reading on GPU |
| `references/cuvs.md` | User needs vector search, nearest neighbors, similarity search, or RAG retrieval on GPU |
| `references/cuspatial.md` | User has GeoPandas/shapely code, or needs spatial joins, distance calculations, or trajectory analysis on GPU |
| `references/raft.md` | User needs sparse eigensolvers, device memory management, or multi-GPU primitives |

Read the specific reference before writing code — they contain detailed API patterns, optimization techniques, and pitfalls specific to each library.
