---
name: chromadb
description: "Chroma — AI-native embedding database. In-process, lightweight vector store with automatic embedding, metadata filtering, and full-text search. Simplest path from prototype to production RAG."
tags: [chromadb, vector-database, embeddings, rag, semantic-search, python, zorai]
---
## Overview

Chroma is an AI-native embedding database optimized for RAG workflows. Lightweight, in-process, with automatic embedding via sentence-transformers, metadata filtering, and semantic search — no separate server required. Fastest path from prototype to production.

## Installation

```bash
uv pip install chromadb
```

## Basic Usage

```python
import chromadb

client = chromadb.PersistentClient(path="./chroma_data")
collection = client.create_collection(name="documents")

# Add documents with metadata
collection.add(
    documents=["Paris is the capital of France.", "Berlin is the capital of Germany."],
    metadatas=[{"country": "France"}, {"country": "Germany"}],
    ids=["doc1", "doc2"],
)

# Query with filter
results = collection.query(
    query_texts=["What is the capital of France?"],
    n_results=3,
    where={"country": "France"},
)
print(results["documents"][0])
```

## References
- [Chroma docs](https://docs.trychroma.com/)
- [Chroma GitHub](https://github.com/chroma-core/chroma)