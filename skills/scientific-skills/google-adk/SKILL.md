---
name: google-adk
description: "Google Agent Development Kit (ADK). Code-first Python toolkit for building, evaluating, and deploying AI agents. Multi-agent orchestration, tool integration, built-in evaluation, and deployment to Vertex AI."
tags: [google-adk, agent-framework, multi-agent, vertex-ai, google, python, zorai]
---
## Overview

Google Agent Development Kit (ADK) is a code-first Python framework for building AI agents powered by Gemini. Supports tool integration, handoffs between agents, guardrails, multi-agent graphs, and deployment to Vertex AI Agent Builder for production serving.

## Installation

```bash
uv pip install google-adk
```

## Basic Agent

```python
from google.adk.agents import Agent
from google.adk.tools import FunctionTool

def get_weather(location: str) -> str:
    return f"The weather in {location} is 22C and sunny."

agent = Agent(
    name="weather_agent",
    model="gemini-2.0-flash",
    instruction="You are a helpful weather assistant.",
    tools=[FunctionTool(get_weather)],
)

response = agent.run("What's the weather in Paris?")
print(response.content)
```

## Multi-Agent Handoff

```python
research = Agent(name="researcher", model="gemini-2.0-flash", ...)
writer = Agent(name="writer", model="gemini-2.0-flash", ...)
reviewer = Agent(name="reviewer", model="gemini-2.0-flash", ...)

from google.adk.runners import Runner
runner = Runner(agents=[research, writer, reviewer])
result = runner.run("Research and write about quantum computing.")
```

## References
- [Google ADK docs](https://cloud.google.com/agent-development-kit/docs)
- [ADK GitHub](https://github.com/google/adk-python)