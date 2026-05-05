---
name: weaviate
description: "Weaviate — open-source vector database with built-in ML. Hybrid search (vector + keyword), generative search, graph connections, multi-modal (text + image), and automatic schema inference."
tags: [vector-database, hybrid-search, rag-retrieval, embedding-indexes, weaviate]
---
## Overview

Weaviate is an open-source vector database with built-in vectorization modules (OpenAI, Cohere, HuggingFace, Transformers, CLIP, multi-modal). Supports hybrid search (vector + BM25 keyword), generative search (RAG with LLM integration), and multi-modal data.

## Installation

```bash
docker run -p 8080:8080 semitechnologies/weaviate:latest
```

## Python Client

```python
import weaviate
import weaviate.classes as wvc

client = weaviate.connect_to_local()
collection = client.collections.create(
    name="Documents",
    vectorizer_config=wvc.config.Configure.Vectorizer.text2vec_transformers(),
)
collection.data.insert({
    "title": "Paris",
    "content": "Paris is the capital of France. It is known for the Eiffel Tower.",
})

# Hybrid search (vector + keyword)
response = collection.query.hybrid(query="French capital", limit=5)
for obj in response.objects:
    print(obj.properties)
```

## References
- [Weaviate docs](https://weaviate.io/developers/weaviate)
- [Weaviate GitHub](https://github.com/weaviate/weaviate)