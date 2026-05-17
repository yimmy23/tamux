---
name: triton-kernel-build-design
description: Build, design, and optimize high-performance GPU kernels using OpenAI Triton. Covers kernel structure, tiling strategy, memory hierarchy, autotuning, debugging, profiling, and production deployment patterns.
recommended_skills:
  - triton-kernel-programming
---

# Triton Kernel Building & Design

## Scope

**Use this guideline when:**
- Writing a new Triton kernel from scratch (matmul, attention, norm, activation fusion, etc.)
- Optimizing an existing Triton kernel for throughput or memory efficiency
- Debugging correctness or performance issues in a Triton kernel
- Integrating a custom Triton kernel with PyTorch models or `torch.compile`
- Choosing tiling parameters, autotuning configs, or pipeline stages

**Do NOT use for:**
- General GPU programming with CUDA/HIP — use HIP or CUDA skills.
- Large-scale distributed training orchestration.
- FP8 quantization strategy (see `quantization-task` or similar).

---

## Core Concepts

### The Blocked Programming Paradigm

Triton flips the traditional CUDA model on its head. Instead of scalar threads with blocked execution, Triton uses **blocked programs with scalar threads**:

| Paradigm | CUDA | Triton |
|----------|------|--------|
| Program | Scalar (1 element per thread) | Blocked (tile per program instance) |
| Threads | Blocked (cooperative groups) | Scalar (each thread handles 1+ elements) |
| Abstractions | Thread IDs, shared memory | `tl.program_id`, `tl.arange`, block pointers |
| Optimization | Manual coalescing, shared mem tiling | Automatic via compiler (coalescing, swizzling, prefetching, vectorization) |

### Compilation Pipeline

A Triton kernel passes through these stages:

```
Python AST → Triton-IR (TTIR) → Triton-GPU IR (TTGIR) → LLVM-IR → PTX → CUBIN
```

**Key insight:** You can inspect each stage with:
```python
# After a kernel launch:
print(kernel.asm['ttir'])     # Triton IR (machine-independent, tile-level)
print(kernel.asm['ttgir'])    # Triton GPU IR (layout-annotated, hardware-aware)
print(kernel.asm['llir'])     # LLVM IR
print(kernel.asm['ptx'])      # PTX (NVIDIA)
print(kernel.asm['cubin'])    # Binary (NVIDIA)
```

---

## Kernel Design Patterns

### 1. Vector Addition — The Simplest Kernel

```python
import torch
import triton
import triton.language as tl

@triton.jit
def add_kernel(x_ptr, y_ptr, output_ptr, n_elements, BLOCK_SIZE: tl.constexpr):
    pid = tl.program_id(axis=0)
    block_start = pid * BLOCK_SIZE
    offsets = block_start + tl.arange(0, BLOCK_SIZE)
    mask = offsets < n_elements
    x = tl.load(x_ptr + offsets, mask=mask)
    y = tl.load(y_ptr + offsets, mask=mask)
    output = x + y
    tl.store(output_ptr + offsets, output, mask=mask)

def add(x: torch.Tensor, y: torch.Tensor) -> torch.Tensor:
    output = torch.empty_like(x)
    n_elements = output.numel()
    grid = lambda meta: (triton.cdiv(n_elements, meta['BLOCK_SIZE']),)
    add_kernel[grid](x, y, output, n_elements, BLOCK_SIZE=1024)
    return output
```

**Key patterns:**
- `tl.constexpr` — compile-time constants for block sizes
- `tl.program_id(axis=0)` — 1D SPMD launch grid
- `tl.arange(0, BLOCK_SIZE)` — creates a range of indices for vectorized access
- `mask` — guards out-of-bounds memory accesses when the tensor size isn't a multiple of BLOCK_SIZE
- Grid is a **lambda** `(meta) -> Tuple[int]` — meta contains all `tl.constexpr` values

### 2. Fused Softmax — Online Normalization

```python
@triton.jit
def softmax_kernel(input_ptr, output_ptr, row_stride, n_cols, BLOCK_SIZE: tl.constexpr):
    pid = tl.program_id(0)
    row_start = pid * row_stride

    # Online softmax: one pass through the row
    col_offsets = tl.arange(0, BLOCK_SIZE)
    offsets = row_start + col_offsets
    mask = col_offsets < n_cols

    x = tl.load(input_ptr + offsets, mask=mask, other=-float('inf'))
    
    # Safe softmax with online max
    x_max = tl.max(x, axis=0)
    x_safe = x - x_max
    x_exp = tl.exp(x_safe)
    x_sum = tl.sum(x_exp, axis=0)
    
    output = x_exp / x_sum
    tl.store(output_ptr + offsets, output, mask=mask)
```

**Design principles:**
- **Online softmax**: compute max → subtract → exp → sum → divide in one pass (avoids two-pass)
- **Numerical stability**: subtract row-wise max before exp
- **`other` parameter**: provides a fallback for masked-out positions (crucial for correctness)

### 3. Matrix Multiplication — The Foundational Pattern

```python
@triton.jit
def matmul_kernel(
    a_ptr, b_ptr, c_ptr,
    M, N, K,
    stride_am, stride_ak,
    stride_bk, stride_bn,
    stride_cm, stride_cn,
    BLOCK_SIZE_M: tl.constexpr,
    BLOCK_SIZE_N: tl.constexpr,
    BLOCK_SIZE_K: tl.constexpr,
    GROUP_SIZE_M: tl.constexpr,
):
    # Program ID and super-grouping for L2 cache efficiency
    pid = tl.program_id(axis=0)
    num_pid_m = tl.cdiv(M, BLOCK_SIZE_M)
    num_pid_n = tl.cdiv(N, BLOCK_SIZE_N)
    num_pid_in_group = GROUP_SIZE_M * num_pid_n
    group_id = pid // num_pid_in_group
    first_pid_m = group_id * GROUP_SIZE_M
    group_size_m = min(num_pid_m - first_pid_m, GROUP_SIZE_M)
    pid_m = first_pid_m + (pid % group_size_m)
    pid_n = (pid % num_pid_in_group) // group_size_m

    # Block pointers
    offs_am = (pid_m * BLOCK_SIZE_M + tl.arange(0, BLOCK_SIZE_M)) % M
    offs_bn = (pid_n * BLOCK_SIZE_N + tl.arange(0, BLOCK_SIZE_N)) % N
    offs_k = tl.arange(0, BLOCK_SIZE_K)
    
    a_ptrs = a_ptr + (offs_am[:, None] * stride_am + offs_k[None, :] * stride_ak)
    b_ptrs = b_ptr + (offs_k[:, None] * stride_bk + offs_bn[None, :] * stride_bn)

    # Accumulator in float32
    acc = tl.zeros((BLOCK_SIZE_M, BLOCK_SIZE_N), dtype=tl.float32)

    for k in range(0, K, BLOCK_SIZE_K):
        a = tl.load(a_ptrs, mask=offs_k[None, :] < K - k, other=0.0)
        b = tl.load(b_ptrs, mask=offs_k[:, None] < K - k, other=0.0)
        acc += tl.dot(a, b)
        a_ptrs += BLOCK_SIZE_K * stride_ak
        b_ptrs += BLOCK_SIZE_K * stride_bk

    c = acc.to(tl.float16)
    offs_cm = pid_m * BLOCK_SIZE_M + tl.arange(0, BLOCK_SIZE_M)
    offs_cn = pid_n * BLOCK_SIZE_N + tl.arange(0, BLOCK_SIZE_N)
    c_ptrs = c_ptr + stride_cm * offs_cm[:, None] + stride_cn * offs_cn[None, :]
    c_mask = (offs_cm[:, None] < M) & (offs_cn[None, :] < N)
    tl.store(c_ptrs, c, mask=c_mask)
```

**Critical design decisions:**
| Decision | Why | Tuning Range |
|----------|-----|-------------|
| `BLOCK_SIZE_M/N` | Tile dimensions; larger = better compute/memory ratio, but fewer parallel blocks | 16–256 |
| `BLOCK_SIZE_K` | Inner reduction dimension; affects register pressure and reuse | 16–64 (AMD), 32–128 (NVIDIA) |
| `GROUP_SIZE_M` | L2 cache reuse: process GROUP_M rows of tiles before moving to next column | 4–16 |
| `num_warps` | Parallelism per block; more warps = more register pressure | 4–8 (NVIDIA), 1–8 (AMD) |
| `dtype` | Accumulator precision; always compute matmul in float32 | fp32 for acc, fp16/bf16 for IO |

---

## Autotuning

### Using `@triton.autotune`

```python
@triton.autotune(
    configs=[
        triton.Config({'BLOCK_SIZE_M': 64, 'BLOCK_SIZE_N': 64, 'BLOCK_SIZE_K': 32, 'GROUP_SIZE_M': 8}, num_warps=4),
        triton.Config({'BLOCK_SIZE_M': 128, 'BLOCK_SIZE_N': 128, 'BLOCK_SIZE_K': 32, 'GROUP_SIZE_M': 8}, num_warps=4),
        triton.Config({'BLOCK_SIZE_M': 128, 'BLOCK_SIZE_N': 256, 'BLOCK_SIZE_K': 64, 'GROUP_SIZE_M': 4}, num_warps=8),
        triton.Config({'BLOCK_SIZE_M': 64, 'BLOCK_SIZE_N': 128, 'BLOCK_SIZE_K': 32, 'GROUP_SIZE_M': 4}, num_warps=4),
    ],
    key=['M', 'N', 'K'],  # Re-evaluate when input shapes change
    prune_configs_by={
        'early_config_prune': lambda configs, named_args: [
            c for c in configs
            if c.kwargs['BLOCK_SIZE_M'] * c.kwargs['BLOCK_SIZE_N'] * c.kwargs['BLOCK_SIZE_K'] <= 256 * 256 * 64
        ],
    },
    reset_to_zero=['c_ptr'],
)
@triton.jit
def my_matmul_kernel(...):
    ...
```

**Autotune key parameters:**
| Parameter | Purpose |
|-----------|---------|
| `configs` | List of `triton.Config` objects with `kwargs` (constexpr values) + `num_warps` |
| `key` | Argument names that trigger re-tuning when their values change |
| `prune_configs_by` | `early_config_prune` callable to drop ineligible configs before benchmarking |
| `reset_to_zero` | Tensors to zero before each config evaluation (avoids cross-contamination) |
| `num_stages` | Number of pipeline stages (0 for single-GEMM, 1 for fused dual-GEMM like FlashAttn) |
| `waves_per_eu` | AMD-specific: hint to reduce VGPR to hit target occupancy |

### Block Size Selection Heuristics

| GPU | Recommended BLOCK_SIZE | Notes |
|-----|------------------------|-------|
| NVIDIA A100/H100 | 128×128 or 128×256 | Large L1 cache; tensor cores love big tiles |
| NVIDIA V100 | 64×64 or 128×64 | Smaller L1; balance parallelism and tile size |
| AMD MI250/MI300X | 64×64 or 128×64 | Use `num_stages=0` for single GEMM; tune `waves_per_eu` |
| Memory-bound ops | Smaller tiles (32×32) | Lower latency per launch, more parallelism |

---

## Memory Optimization

### Memory Hierarchy in Triton

| Level | Access Cost | Size | Triton Control |
|-------|-------------|------|----------------|
| Registers | ~1 cycle | ~256 KB/CU | `num_warps` affects allocation |
| Shared Memory (LDS) | ~20 cycles | 64–164 KB/CU | `triton_gpu.shared` in IR, automatic |
| Global Memory (HBM) | ~300 cycles | 40–80 GB | `tl.load`/`tl.store` |

### Optimization Rules

1. **Minimize global memory traffic**: Load data once per block, keep in registers/shared mem
2. **Use `tl.dot` for matmuls**: Always use `tl.dot` instead of manual multiply-reduce — it triggers tensor core instructions
3. **Vectorize memory access**: `tl.load` with large block sizes → automatic `global_load_dwordx4` (128-bit) in ISA
4. **Shared memory for cross-thread data**: If data needs transposing or sharing across threads, stage through LDS
5. **Check LDS usage from IR**:
   ```bash
   export MLIR_ENABLE_DUMP=1
   rm -rf ~/.triton/cache
   python kernel.py 2>&1 | grep "triton_gpu.shared"
   ```

### AMD-Specific Optimizations

```bash
# Set pipeline stages based on kernel type:
# Single GEMM → num_stages=0
# Fused dual GEMM (FlashAttn) → num_stages=1
# GEMM + activation → num_stages=0
# No GEMM → num_stages=1

# Query hardware:
rocminfo | grep "Compute Unit"      # Number of CUs
rocminfo | grep "SIMD"              # SIMDs per CU
rocminfo | grep "Wavefront Size"    # Wavefront size (64 on MI300X)

# Check VGPR usage from ISA:
# Search for .vgpr_count in generated ISA
```

### NVIDIA-Specific Optimization (Hopper/SM90)

```python
# FP8 Tensor Core usage with wgmma (Hopper+)
# Triton automatically uses wgmma for fp8 inputs:
# PTX: wgmma.mma_async.sync.aligned.m16n16k16...
# SASS: QGMMA instruction
# Enable via: use fp8 inputs and tl.dot
```

---

## Debugging

### Triton's Built-in Debug Operators

| Operator | Stage | Purpose |
|----------|-------|---------|
| `tl.static_print` | Compile-time | Print values at JIT compilation |
| `tl.static_assert` | Compile-time | Assert conditions at JIT time |
| `tl.device_print` | Runtime | Print tensor values during kernel execution (always active) |
| `tl.device_assert` | Runtime | Assert during execution (only when `TRITON_DEBUG=1`) |

### Interpreter Mode

```bash
# Run on CPU with numpy backends for step-by-step debugging:
TRITON_INTERPRET=1 python kernel.py

# Attach pdb:
TRITON_INTERPRET=1 pdb main.py

# Or set breakpoints inside the kernel:
@triton.jit
def my_kernel(...):
    import pdb; pdb.set_trace()
    ...
```

**Interpreter limitations:**
- No bfloat16 support — cast to float32 first
- No indirect memory access: `ptr = tl.load(ptr); x = tl.load(ptr)` — not supported

### Third-Party Debugging Tools

| Tool | GPU | Use Case |
|------|-----|----------|
| `compute-sanitizer` | NVIDIA | Data races, memory access issues |
| `triton-viz` | Both | Memory access visualization |
| LLVM AddressSanitizer | AMD/ROCm | Memory errors |
| Nsight Compute (ncu) | NVIDIA | Profiling, assembly analysis |
| ROCProfiler | AMD | Profiling |

---

## Profiling & Benchmarking

### Performance Measurement

```python
@triton.testing.perf_report(
    triton.testing.Benchmark(
        x_names=['M', 'N', 'K'],
        x_vals=[128 * i for i in range(1, 5)],
        line_arg='provider',
        line_vals=['triton', 'torch'],
        line_names=['Triton', 'PyTorch'],
        styles=[('blue', '-'), ('green', '-')],
        ylabel='TFLOPS',
        plot_name='matmul-performance',
        args={},
    )
)
def benchmark(M, N, K, provider):
    a = torch.randn(M, K, device='cuda', dtype=torch.float16)
    b = torch.randn(K, N, device='cuda', dtype=torch.float16)
    quantiles = [0.5, 0.2, 0.8]
    if provider == 'torch':
        ms, min_ms, max_ms = triton.testing.do_bench(lambda: torch.matmul(a, b), quantiles=quantiles)
    else:
        ms, min_ms, max_ms = triton.testing.do_bench(lambda: matmul(a, b), quantiles=quantiles)
    perf = lambda ms: 2 * M * N * K * 1e-12 / (ms * 1e-3)
    return perf(ms), perf(max_ms), perf(min_ms)
```

### Key Performance Metrics

| Metric | What It Measures | Target |
|--------|-----------------|--------|
| TFLOPS | Compute throughput | >60% of peak for matmuls |
| Bandwidth utilization | Memory throughput | >70% of peak HBM bandwidth |
| Occupancy | Warps active per SM/CU | >50% for compute-bound, >75% for memory-bound |
| L1/L2 hit rate | Cache efficiency | >80% L2 for well-tiled kernels |

### NVIDIA: Nsight Compute Analysis

```bash
# Profile kernel occupancy and instruction mix
ncu --target-processes all --set full -o kernel_profile python run_kernel.py

# Check for wgmma/mma instructions (FP8 tensor core)
ncu --print-summary per-kernel python run_kernel.py

# View kernel launch overhead (CPU-side)
nsys profile -o trace_output python run_kernel.py
```

### AMD: Occupancy Computation

```python
# 1. Get VGPR count from ISA (.vgpr_count)
# 2. Get LDS allocation:
export MLIR_ENABLE_DUMP=1; rm -rf ~/.triton/cache; python kernel.py 2>&1 | grep "triton_gpu.shared"
# 3. Get num_warps from IR:
export MLIR_ENABLE_DUMP=1; rm -rf ~/.triton/cache; python kernel.py 2>&1 | grep "triton_gpu.num-warps"
# 4. Compute occupancy:
# occ_vgpr from VGPR occupancy table (e.g., MI300X: 512 VGPR/EU, alloc in units of 16)
# occ_lds = floor(65536 / L)
# occ = min(floor(occ_vgpr * 4 / nW), occ_lds) * nW / 4
```

---

## Advanced Patterns

### Split-K Parallelization (Small-Batch Inference)

For matrix shapes where M < N, K (e.g., Llama-70B inference with batch=1):
```python
# Launch extra thread blocks along K dimension
# Each block computes a partial output sum
# Partial results summed via atomic reduction
# Use-cases: small M regime (M=1–64)
# Speedup vs base Triton: up to 1.94x on H100
```

### CUDA Graphs Integration

```python
# After kernel autotuning, capture the launch graph:
# 1. Warm up with representative shapes
# 2. Capture with torch.cuda.CUDAGraph
# 3. Replay for inference — eliminates CPU launch overhead (~2ms per kernel)

# Without CUDA Graphs: ~165us between GEMMs
# With CUDA Graphs: ~12us between GEMMs
```

### Fused Activation with GEMM

```python
# Pattern: compute GEMM, apply activation inline, store
# Instead of: matmul → load result → activation → store
# Do: matmul → tl.silu(acc) → store (saves one global read/write)

@triton.jit
def fused_gemm_silu_kernel(...):
    acc = tl.zeros(...)
    for k in range(0, K, BLOCK_SIZE_K):
        a = tl.load(a_ptrs, mask=...)
        b = tl.load(b_ptrs, mask=...)
        acc += tl.dot(a, b)
    acc = acc.to(tl.float16)
    acc = tl.sigmoid(acc) * acc  # SiLU/GELU fusion
    tl.store(c_ptrs, acc, mask=...)
```

---

## Quality Gates

| Gate | Check | Threshold |
|------|-------|-----------|
| **Correctness** | Max abs diff vs torch reference | `< 0.01 (fp16)` or `< 0.5 (fp8)` |
| **Autotuning convergence** | Best config found | All configs evaluated |
| **Occupancy** | VGPR-limited or LDS-limited | `occ > 0.5` |
| **Memory coalescing** | ISA has `global_load_dwordx4` | Present in hot loop |
| **Tensor core usage** | PTX has `wgmma`/`mma` op | Present for matmul kernels |
| **LDS usage** | Shared memory allocation | `< 64 KB` (well within per-CU limit) |
| **Vector width** | LDS access uses `_b128` or `_b64` | `_b64` minimum |

---

## Cross-References

- `triton-kernel-programming` skill — hands-on implementation template and API reference
- `training-data-design-principles` — for understanding how kernels fit into training pipelines
- `llm-training-data-task` — when building kernels for LLM pre-training or fine-tuning
- Official tutorials at https://triton-lang.org/main/getting-started/tutorials/
- Triton autotune API: https://triton-lang.org/main/python-api/generated/triton.autotune.html
- ROCm optimization guide for AMD-specific tuning: https://rocm.docs.amd.com

---

## References

| Resource | Link |
|----------|------|
| Official Triton Docs | https://triton-lang.org/ |
| Triton Tutorials | https://triton-lang.org/main/getting-started/tutorials/ |
| Triton Compilation Stages (PyTorch blog) | https://pytorch.org/blog/triton-kernel-compilation-stages/ |
| AMD Triton Optimization | https://rocm.docs.amd.com/en/docs-6.1.0/how-to/llm-fine-tuning-optimization/optimizing-triton-kernel.html |
| AMD Triton Dev Tutorial | https://rocm.docs.amd.com/projects/ai-developer-hub/en/latest/notebooks/gpu_dev_optimize/triton_kernel_dev.html |
| Llama3 FP8 Triton Inference (PyTorch blog) | https://pytorch.org/blog/accelerating-llama3 |
| Triton Debugging Guide | https://triton-lang.org/main/programming-guide/chapter-3/debugging.html |
| Triton Programming Guide Intro | https://triton-lang.org/main/programming-guide/chapter-1/introduction.html |
| Triton Matrix Multiplication Tutorial | https://github.com/triton-lang/triton/blob/main/python/tutorials/03-matrix-multiplication.py |
| Triton Autotune API | https://triton-lang.org/main/python-api/generated/triton.autotune.html |
