---
name: openai-agents-sdk
description: "OpenAI Agents SDK — build agentic workflows with handoffs, guardrails, and tool integration. Single-agent to multi-agent orchestration. Tracing and observability. Python-first SDK from OpenAI."
tags: [openai, agents-sdk, agent-framework, handoffs, guardrails, orchestration, zorai]
---
## Overview

OpenAI Agents SDK provides a lightweight framework for building agentic AI workflows. Supports tool use, handoffs (agent-to-agent routing), guardrails, and tracing — built on the OpenAI API's native agent primitives.

## Installation

```bash
uv pip install openai-agents
```

## Basic Agent

```python
from agents import Agent, Runner

agent = Agent(name="Helper", instructions="You are a helpful assistant.")
result = Runner.run_sync(agent, "What is the capital of France?")
print(result.final_output)
```

## Tool Use

```python
from agents import Agent, Runner, function_tool

@function_tool
def get_weather(city: str) -> str:
    return f"Weather in {city}: sunny, 22°C"

agent = Agent(name="WeatherBot", instructions="Use tools to answer weather queries.", tools=[get_weather])
result = Runner.run_sync(agent, "What's the weather in London?")
print(result.final_output)
```

## Handoffs

```python
from agents import Agent, Runner

triage = Agent(name="Triage", instructions="Route to the right specialist.")
billing = Agent(name="Billing", instructions="Handle billing questions.")
triage.handoffs = [billing]

result = Runner.run_sync(triage, "My invoice didn't arrive")
```

## References
- [OpenAI Agents SDK docs](https://openai.github.io/openai-agents-python/)
- [OpenAI Agents GitHub](https://github.com/openai/openai-agents-python)