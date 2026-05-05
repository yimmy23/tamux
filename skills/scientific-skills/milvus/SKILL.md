---
name: milvus
description: "Milvus — cloud-native vector database for billion-scale similarity search. GPU-accelerated indexing, hybrid search, multi-vector, streaming, and time travel. Distributed deployment with Kubernetes."
tags: [milvus, vector-database, similarity-search, scale, embeddings, infrastructure, zorai]
---
## Overview

Milvus is a cloud-native vector database for billion-scale similarity search. Supports GPU-accelerated indexing (IVF, HNSW, DiskANN), hybrid search (dense + sparse), multi-vector, streaming ingestion, time travel, and distributed deployment with Kubernetes.

## Installation

```bash
docker compose -f https://github.com/milvus-io/milvus/releases/latest/download/milvus-standalone-docker-compose.yml up -d
```

## Python Client

```python
from pymilvus import connections, Collection, FieldSchema, CollectionSchema, DataType

connections.connect(host="localhost", port=19530)
schema = CollectionSchema([
    FieldSchema("id", DataType.INT64, is_primary=True),
    FieldSchema("embedding", DataType.FLOAT_VECTOR, dim=384),
    FieldSchema("text", DataType.VARCHAR, max_length=1000),
])
collection = Collection("documents", schema)
collection.create_index("embedding", {"index_type": "IVF_FLAT", "metric_type": "L2", "params": {"nlist": 128}})
collection.load()
```

## References
- [Milvus docs](https://milvus.io/docs)
- [Milvus GitHub](https://github.com/milvus-io/milvus)