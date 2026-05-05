---
name: autogen
description: "AutoGen (Microsoft) — multi-agent conversation framework. Agent-to-agent chat, code generation & execution, tool use, group chat, and human-in-the-loop. Build collaborative AI systems with specialized agents."
tags: [autogen, multi-agent, conversation, microsoft, llm, agent-framework, zorai]
---
## Overview

AutoGen (Microsoft) enables multi-agent conversations where specialized LLM agents collaborate, write and execute code, use tools, and solve problems together. Supports group chat, human-in-the-loop, and flexible agent topologies.

## Installation

```bash
uv pip install pyautogen
```

## Two-Agent Coding

```python
import autogen

config_list = [{"model": "gpt-4", "api_key": "sk-your-key"}]

assistant = autogen.AssistantAgent(
    name="coder",
    llm_config={"config_list": config_list},
)

user = autogen.UserProxyAgent(
    name="user",
    human_input_mode="NEVER",
    code_execution_config={"work_dir": "coding", "use_docker": False},
)

user.initiate_chat(
    assistant,
    message="Write a Python function to calculate Fibonacci numbers.",
)
```

## Group Chat

```python
from autogen import GroupChat, GroupChatManager

groupchat = GroupChat(agents=[engineer, critic, executor], messages=[], max_round=10)
manager = GroupChatManager(groupchat=groupchat, llm_config=llm_config)
```

## References
- [AutoGen docs](https://microsoft.github.io/autogen/)
- [AutoGen GitHub](https://github.com/microsoft/autogen)