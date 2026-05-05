---
name: lancedb
description: "LanceDB — serverless vector database for AI. Columnar storage on Lance format, zero-copy access, multimodal search (text + images + audio), and direct DataFrame integration. No separate server."
tags: [lancedb, vector-database, embedded, multimodal, embeddings, python, zorai]
---
## Overview

LanceDB is a developer-friendly, serverless vector database built on the Lance columnar format. It supports multimodal search (text, image, audio embeddings), hybrid search, and efficient streaming ingestion without a separate server process.

## Installation

```bash
uv pip install lancedb
```

## Create and Query

```python
import lancedb
import numpy as np

db = lancedb.connect("./my_lancedb")
table = db.create_table("vectors", [
    {"vector": np.random.rand(128), "text": "hello world"},
    {"vector": np.random.rand(128), "text": "goodbye moon"},
])

results = table.search(np.random.rand(128)).limit(5).to_list()
print([r["text"] for r in results])
```

## Open-Clip Embeddings

```python
import lancedb
from lancedb.embeddings import with_open_clip

@with_open_clip
class Images:
    image: str
    vector: list

table = db.create_table("images", schema=Images)
table.add([{"image": "photo.jpg"}, {"image": "diagram.png"}])
results = table.search("sunset landscape").limit(3).to_pandas()
```

## References
- [LanceDB docs](https://lancedb.github.io/lancedb/)
- [Lance format](https://lancedb.github.io/lance/)