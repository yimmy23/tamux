---
name: lorem-ipsum
description: Generate lorem ipsum placeholder text. This skill should be used when users ask to generate lorem ipsum content, placeholder text, dummy text, or filler text. Supports various structures including plain paragraphs, headings with sections, lists, and continuous text. Output can be saved to a file or used directly as requested by the user.

tags: [productivity, agent-skills, lorem-ipsum, writing]
-----|-------------|
| `--paragraphs N` | Number of paragraphs (default: 3) |
| `--sentences N` | Sentences per paragraph (default: 5) |
| `--words N` | Approximate total word count |
| `--characters N` | Exact character count (truncates to match) |
| `--tokens N` | Estimated LLM token count (~4 chars/token) |
| `--continuous` | Output without paragraph breaks |
| `--headings N` | Number of sections with headings |
| `--bullets N` | Bullet points per section |
| `--numbered` | Use numbered lists instead of bullets |
| `--mixed N` | Realistic document with N sections, varied content types |
| `--output FILE` | Write to file instead of stdout |
| `--format FORMAT` | Output format: text, markdown, html (default: markdown) |

## Workflow

1. Interpret the user's request for length and structure
2. Run `scripts/generate.py` with appropriate options
3. If the user wants the output saved, use `--output` or redirect/pipe as needed
4. If the user wants it in clipboard, pipe to `pbcopy`
5. Display the result or confirm the file was written

## Examples

**"Generate 3 paragraphs of lorem ipsum"**
```bash
uv run scripts/generate.py --paragraphs 3
```

**"Create lorem ipsum with 3 headings and 2 paragraphs under each"**
```bash
uv run scripts/generate.py --headings 3 --paragraphs 6
```

**"Give me a document with bullet points"**
```bash
uv run scripts/generate.py --headings 3 --bullets 5
```

**"500 words of continuous lorem ipsum saved to ~/Desktop/placeholder.txt"**
```bash
uv run scripts/generate.py --words 500 --continuous --output ~/Desktop/placeholder.txt
```

**"Lorem ipsum with numbered lists in HTML format"**
```bash
uv run scripts/generate.py --headings 4 --bullets 5 --numbered --format html
```

**"Exactly 500 characters of lorem ipsum"**
```bash
uv run scripts/generate.py --characters 500
```

**"About 100 tokens worth of lorem ipsum with headings"**
```bash
uv run scripts/generate.py --tokens 100 --headings 2
```

**"A realistic document with mixed content"**
```bash
uv run scripts/generate.py --mixed 5
```
