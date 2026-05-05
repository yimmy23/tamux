---
name: crewai
description: "CrewAI — multi-agent AI framework. Role-based agents with defined goals, tools, and memory. Hierarchical and sequential task execution. Human input delegation and process orchestration."
tags: [crewai, multi-agent, agent-framework, collaboration, llm, orchestration, zorai]
---
## Overview

CrewAI enables role-based multi-agent AI systems. Agents have defined goals, tools, backstories, and memory. Tasks are assigned to specific agents with expected outputs. Supports sequential and hierarchical execution.

## Installation

```bash
uv pip install crewai
```

## Research Crew

```python
from crewai import Agent, Task, Crew

researcher = Agent(
    role="Research Analyst",
    goal="Find latest developments in AI agents",
    backstory="Expert at finding relevant information",
)

writer = Agent(
    role="Technical Writer",
    goal="Write clear summary of findings",
    backstory="Skilled at explaining technical topics",
)

task1 = Task(description="Search for latest AI agent frameworks in 2025",
             expected_output="List of frameworks with key features",
             agent=researcher)

task2 = Task(description="Write a 3-paragraph summary", expected_output="Markdown report", agent=writer)

crew = Crew(agents=[researcher, writer], tasks=[task1, task2])
result = crew.kickoff()
print(result)
```

## With Tools

```python
from crewai_tools import SerperDevTool
researcher = Agent(
    role="Research Analyst",
    tools=[SerperDevTool()],
)
```

## References
- [CrewAI docs](https://docs.crewai.com/)
- [CrewAI GitHub](https://github.com/crewAIInc/crewAI)