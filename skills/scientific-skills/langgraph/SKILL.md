---
name: langgraph
description: "LangGraph — orchestrate LLM agents as stateful graphs. Multi-agent coordination, persistent state, human-in-the-loop, streaming, checkpointing, and conditional control flow. Build complex agent workflows."
tags: [langgraph, agent-orchestration, state-machine, multi-agent, langchain, llm, zorai]
---
## Overview

LangGraph builds stateful, multi-step agent workflows as graphs. Supports conditional routing, human-in-the-loop checkpoints, persistent state, streaming, and multi-agent orchestration. The graph-based design enables complex, controllable agent behavior.

## Installation

```bash
uv pip install langgraph
```

## Simple Graph

```python
from typing import TypedDict
from langgraph.graph import StateGraph, END

class AgentState(TypedDict):
    messages: list
    next_step: str

def research(state):
    return {"messages": state["messages"], "next_step": "write"}

def write(state):
    return {"messages": state["messages"], "next_step": "review"}

def review(state):
    return {"messages": state["messages"], "next_step": "__end__"}

graph = StateGraph(AgentState)
graph.add_node("research", research)
graph.add_node("write", write)
graph.add_node("review", review)
graph.set_entry_point("research")
graph.add_edge("research", "write")
graph.add_conditional_edges("write", lambda s: s["next_step"])
graph.add_edge("review", END)
app = graph.compile()

result = app.invoke({"messages": [], "next_step": ""})
```

## References
- [LangGraph docs](https://langchain-ai.github.io/langgraph/)
- [LangGraph GitHub](https://github.com/langchain-ai/langgraph)