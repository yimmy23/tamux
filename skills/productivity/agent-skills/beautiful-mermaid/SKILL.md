---
name: beautiful-mermaid
description: Render Mermaid diagrams as SVG and PNG using the Beautiful Mermaid library. Use when the user asks to render a Mermaid diagram.

tags: [productivity, agent-skills, beautiful-mermaid, svg, mermaid, diagram]
-------|------------|----------|
| default | Light grey | General use |
| dracula | Dark purple | Dark mode preference |
| tokyo-night | Dark blue | Modern dark aesthetic |
| tokyo-night-storm | Darker blue | Higher contrast |
| nord | Dark arctic | Muted, calm visuals |
| nord-light | Light arctic | Light mode with soft tones |
| github-dark | GitHub dark | Matches GitHub UI |
| github-light | GitHub light | Matches GitHub UI |
| catppuccin-latte | Warm light | Soft pastel aesthetic |
| solarized | Tan/cream | Solarized colour scheme |
| one-dark | Atom dark | Atom editor aesthetic |
| zinc-dark | Neutral dark | Minimal, no colour bias |

## Troubleshooting

### Theme not applied

Check the render script output for the `bg` and `fg` values, or inspect the SVG's opening tag for `--bg` and `--fg` CSS custom properties.

### Diagram appears cut off or incomplete

- Check edge label syntax — use `-->|label|` pipe notation, not `-- label -->`
- Verify all node IDs are unique
- Check for unclosed brackets in node labels

### Render produces empty or malformed SVG

- Validate Mermaid syntax at https://mermaid.live before rendering
- Check for special characters that need escaping (wrap in quotes)
- Ensure flowchart direction is specified (`graph TD`, `graph LR`, etc.)
