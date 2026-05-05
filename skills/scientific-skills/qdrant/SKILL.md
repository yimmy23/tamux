---
name: qdrant
description: "Qdrant — vector similarity search engine. Payload filtering, quantized indexing, multi-tenant, and horizontal scaling. REST and gRPC API. Docker-native deployment for production RAG and recommendation."
tags: [qdrant, vector-database, similarity-search, embeddings, rag, infrastructure, zorai]
---
## Overview

Qdrant is a high-performance vector similarity search engine supporting dense and sparse vectors, payload indexing and filtering, scalar/PQ quantization, multi-tenancy, and horizontal scaling via clustering. REST and gRPC APIs with async support.

## Installation

```bash
docker run -p 6333:6333 qdrant/qdrant
```

## Python Client

```python
from qdrant_client import QdrantClient, models
import numpy as np

client = QdrantClient("localhost", port=6333)
client.create_collection("documents", vectors_config=models.VectorParams(
    size=384, distance=models.Distance.COSINE))

client.upsert("documents", points=[
    models.PointStruct(id=1, vector=np.random.rand(384).tolist(), payload={"text": "Paris is capital of France"}),
    models.PointStruct(id=2, vector=np.random.rand(384).tolist(), payload={"text": "Berlin is capital of Germany"}),
])

results = client.search("documents", query_vector=np.random.rand(384).tolist(), limit=5)
for hit in results:
    print(hit.payload["text"], hit.score)
```

## References
- [Qdrant docs](https://qdrant.tech/documentation/)
- [Qdrant GitHub](https://github.com/qdrant/qdrant)