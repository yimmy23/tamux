---
name: pyzotero
description: Interact with Zotero reference management libraries using the pyzotero Python client. Retrieve, create, update, and delete items, collections, tags, and attachments via the Zotero Web API v3. Use this skill when working with Zotero libraries programmatically, managing bibliographic references, exporting citations, searching library contents, uploading PDF attachments, or building research automation workflows that integrate with Zotero.
allowed-tools: Read Write Edit Bash
license: MIT License
tags: [scientific-skills, pyzotero, python, api, workflow, citation-management]
metadata:
    skill-author: K-Dense Inc.
---|----------|
| [references/authentication.md](references/authentication.md) | Credentials, library types, local mode |
| [references/read-api.md](references/read-api.md) | Retrieving items, collections, tags, groups |
| [references/search-params.md](references/search-params.md) | Filtering, sorting, search parameters |
| [references/write-api.md](references/write-api.md) | Creating, updating, deleting items |
| [references/collections.md](references/collections.md) | Collection CRUD operations |
| [references/tags.md](references/tags.md) | Tag retrieval and management |
| [references/files-attachments.md](references/files-attachments.md) | File retrieval and attachment uploads |
| [references/exports.md](references/exports.md) | BibTeX, CSL-JSON, bibliography export |
| [references/pagination.md](references/pagination.md) | follow(), everything(), generators |
| [references/full-text.md](references/full-text.md) | Full-text content indexing and retrieval |
| [references/saved-searches.md](references/saved-searches.md) | Saved search management |
| [references/cli.md](references/cli.md) | Command-line interface usage |
| [references/error-handling.md](references/error-handling.md) | Errors and exception handling |

## Common Patterns

### Fetch and modify an item
```python
item = zot.item('ITEMKEY')
item['data']['title'] = 'New Title'
zot.update_item(item)
```

### Create an item from a template
```python
template = zot.item_template('journalArticle')
template['title'] = 'My Paper'
template['creators'][0] = {'creatorType': 'author', 'firstName': 'Jane', 'lastName': 'Doe'}
zot.create_items([template])
```

### Export as BibTeX
```python
zot.add_parameters(format='bibtex')
bibtex = zot.top(limit=50)
# bibtex is a bibtexparser BibDatabase object
print(bibtex.entries)
```

### Local mode (read-only, no API key needed)
```python
zot = Zotero(library_id='123456', library_type='user', local=True)
items = zot.items()
```
