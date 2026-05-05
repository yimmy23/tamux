---
name: clip
description: "OpenAI CLIP — contrastive language-image pre-training. Zero-shot image classification, image-text similarity, concept search, and cross-modal retrieval. Embed images and text into shared space."
tags: [clip, multimodal, image-text, zero-shot, embeddings, openai, zorai]
---
## Overview

OpenAI CLIP (Contrastive Language-Image Pre-training) learns joint text-image representations. Enables zero-shot image classification, image-text similarity, cross-modal search, and image captioning without task-specific training.

## Installation

```bash
uv pip install openai-clip
```

## Zero-Shot Classification

```python
import clip
import torch

model, preprocess = clip.load("ViT-B/32")
image = preprocess(load_image("photo.jpg")).unsqueeze(0)
text = clip.tokenize(["a dog", "a cat", "a bird"])

with torch.no_grad():
    logits, _ = model(image, text)
    probs = logits.softmax(dim=-1)

print(f"Predicted: class {probs.argmax().item()} with {probs.max():.2%} confidence")
```

## Text-Image Similarity

```python
images = torch.stack([preprocess(img) for img in [load_image("a.jpg"), load_image("b.jpg")]])
texts = clip.tokenize(["sunset", "ocean", "mountain"])

with torch.no_grad():
    similarity = model(images, texts)[0].softmax(dim=-1)
```

## References
- [CLIP GitHub](https://github.com/openai/CLIP)
- [CLIP paper](https://arxiv.org/abs/2103.00020)