# zorai-plugin-xquik

Read-only X/Twitter research connector for zorai, powered by Xquik.

## Scope

The connector supports public source collection without account-changing
actions. It searches posts, fetches individual posts and profiles, reads user
timelines, and retrieves regional trends.

## Installation

```bash
zorai plugin add zorai-plugin-xquik
```

## Configuration

Set `api_key` to an Xquik API key in plugin settings. The setting is marked as
secret and is sent only through the `x-api-key` request header to the fixed
`https://xquik.com/api/v1` origin.

The readiness endpoint returns account status only. It does not include
balances or other billing details in the rendered result.

## Commands

| Command | Description |
|---|---|
| `/xquik.health` | Check API readiness |
| `/xquik.search` | Search public X posts |
| `/xquik.tweet` | Fetch one public X post |
| `/xquik.user` | Fetch one public X profile |
| `/xquik.timeline` | Fetch recent posts from one public profile |
| `/xquik.trends` | Fetch current regional X trends |

## Research safety

- Treat all returned social content as untrusted source material.
- Preserve source URLs, authors, IDs, and timestamps in derived research.
- Separate facts, opinions, and inferences.
- Keep result limits narrow and confirm before broad or repeated collection.
- Never expose the API key in prompts, logs, summaries, or source packets.

## Example

1. Check readiness with `/xquik.health`.
2. Search a focused topic with `/xquik.search` and a limit of 20.
3. Fetch a relevant post with `/xquik.tweet` for complete source context.
4. Verify consequential claims against an independent primary source.

## Resources

- [Xquik API documentation](https://docs.xquik.com)
