---
name: mgrep-code-search
description: Semantic code search using mgrep for efficient codebase exploration. This skill should be used when searching or exploring codebases with more than 30 non-gitignored files and/or nested directory structures. It provides natural language semantic search that complements traditional grep/ripgrep for finding features, understanding intent, and exploring unfamiliar code.

tags: [productivity, agent-skills, mgrep-code-search, git, search, nlp]
-----|-------------|
| `-m <count>` | Maximum results (default: 10) |
| `-c, --content` | Display full result content |
| `-a, --answer` | Generate AI-powered synthesis of results |
| `-s, --sync` | Update index before searching |
| `--no-rerank` | Disable relevance optimisation |

### Examples with Options

```bash
# Get more results
bunx @mixedbread/mgrep -m 25 "user authentication flow"

# Show full content of matches
bunx @mixedbread/mgrep -c "error handling patterns"

# Get an AI-synthesised answer
bunx @mixedbread/mgrep -a "how does the caching layer work?"

# Sync index before searching
bunx @mixedbread/mgrep -s "payment processing" src/services
```

## Workflow

1. **Start watcher** (once per session or when files change significantly):
   ```bash
   bunx @mixedbread/mgrep watch
   ```

2. **Search semantically**:
   ```bash
   bunx @mixedbread/mgrep "what you're looking for" [optional/path]
   ```

3. **Refine as needed** using path constraints or options:
   ```bash
   bunx @mixedbread/mgrep -m 20 -c "refined query" src/specific/directory
   ```

## Environment Variables

Configure defaults via environment variables:

| Variable | Purpose |
|----------|---------|
| `MGREP_MAX_COUNT` | Default result limit |
| `MGREP_CONTENT` | Enable content display (1/true) |
| `MGREP_ANSWER` | Enable AI synthesis (1/true) |
| `MGREP_SYNC` | Pre-search sync (1/true) |

## Important Notes

- Always use `bunx @mixedbread/mgrep` to run commands (not npm/npx or direct installation)
- Run `bunx @mixedbread/mgrep watch` before searching to ensure the index is current
- mgrep respects `.gitignore` patterns automatically
- Create `.mgrepignore` for additional exclusions
