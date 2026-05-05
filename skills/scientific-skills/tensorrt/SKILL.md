---
name: tensorrt
description: "NVIDIA TensorRT — deep learning inference optimizer. FP16/INT8/INT4 quantization, kernel auto-tuning, layer fusion, and dynamic shapes. Max throughput on NVIDIA GPUs for production inference."
tags: [inference-optimization, quantized-inference, nvidia-deployment, engine-building, tensorrt]
---
## Overview

TensorRT is NVIDIA's high-performance inference optimizer and runtime for deploying deep learning models on NVIDIA GPUs. Use it when you need lower latency, higher throughput, FP16/INT8 optimization, or production GPU serving from ONNX or TensorFlow/PyTorch exports.

## When to Use

Use this skill when:
- a model already works in PyTorch/TensorFlow but inference is too slow,
- you need FP16 or INT8 deployment on NVIDIA GPUs,
- you are deploying vision, NLP, or embedding models in production,
- you want to serve optimized engines via Triton Inference Server,
- or you need to benchmark GPU inference carefully instead of guessing.

## Install / Environment

TensorRT is usually installed via NVIDIA packages, Docker images, or NGC containers rather than plain pip.

Typical paths:

```bash
# Inside NVIDIA container ecosystems
# Use an NGC PyTorch or TensorRT container

# ONNX graph simplification often helps before conversion
uv pip install onnx onnxruntime onnxsim polygraphy
```

## Fastest Common Workflow: ONNX -> TensorRT Engine

1. Export model to ONNX.
2. Validate ONNX with ONNX Runtime.
3. Build TensorRT engine with `trtexec`.
4. Benchmark latency/throughput.
5. Integrate engine into app or Triton.

## Export from PyTorch to ONNX

```python
import torch

dummy = torch.randn(1, 3, 224, 224, device='cuda')
model.eval()

torch.onnx.export(
    model,
    dummy,
    'model.onnx',
    input_names=['input'],
    output_names=['logits'],
    dynamic_axes={'input': {0: 'batch'}, 'logits': {0: 'batch'}},
    opset_version=17,
)
```

## Validate the ONNX model first

```bash
python - <<'PY'
import onnx
m = onnx.load('model.onnx')
onnx.checker.check_model(m)
print('ONNX OK')
PY
```

## Build an FP16 engine

```bash
trtexec   --onnx=model.onnx   --saveEngine=model_fp16.plan   --fp16   --workspace=4096   --minShapes=input:1x3x224x224   --optShapes=input:8x3x224x224   --maxShapes=input:32x3x224x224
```

## INT8 optimization

Use INT8 only when you have either:
- good calibration data, or
- quantization-aware-prepared/exported graph.

```bash
trtexec   --onnx=model.onnx   --saveEngine=model_int8.plan   --int8   --fp16
```

## Benchmarking

```bash
trtexec --loadEngine=model_fp16.plan --shapes=input:8x3x224x224
```

Check:
- mean latency
- throughput
- GPU memory
- whether kernels are actually using Tensor Cores

## Triton deployment

Recommended production layout:

```text
model_repository/
  my_model/
    1/
      model.plan
    config.pbtxt
```

Minimal `config.pbtxt`:

```text
name: "my_model"
platform: "tensorrt_plan"
max_batch_size: 32
input [
  {
    name: "input"
    data_type: TYPE_FP32
    dims: [ 3, 224, 224 ]
  }
]
output [
  {
    name: "logits"
    data_type: TYPE_FP32
    dims: [ 1000 ]
  }
]
```

## Common failure modes

- ONNX exports unsupported ops -> simplify graph or change export path
- dynamic shapes missing -> engine only works for one batch/shape
- INT8 accuracy collapse -> calibrate properly or stay on FP16
- preprocessing mismatch -> model seems broken but input normalization is wrong
- engine built on one GPU architecture and reused on incompatible target

## Verification checklist

- compare TensorRT output vs PyTorch on the same test batch
- measure top-1/top-k or task metric after conversion
- benchmark multiple batch sizes, not just batch=1
- test warm and cold runs separately
- save build commands alongside the engine artifact
