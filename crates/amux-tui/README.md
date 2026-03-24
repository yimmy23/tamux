# amux-tui

A keyboard-first terminal UI for the amux AI daemon.

## Build & Run

```bash
cargo run -p amux-tui
```

Requires a running amux daemon (connects via Unix socket or TCP).

## Keyboard Shortcuts

### Navigation
| Key | Action |
|-----|--------|
| Tab / Shift+Tab | Cycle focus: Chat → Sidebar → Input |
| Ctrl+P | Open command palette |
| Ctrl+T | Open thread picker |
| Ctrl+B | Toggle sidebar visibility |
| / | Open command palette |

### Chat (when focused)
| Key | Action |
|-----|--------|
| ↑ / ↓ | Select message |
| PgUp / PgDn | Scroll chat |
| Ctrl+D / Ctrl+U | Half-page scroll |
| Home / End | Scroll to top / bottom |
| r | Toggle reasoning on selected message |
| e / Enter | Toggle tool call expansion |
| c | Copy selected message to clipboard |
| Esc | Clear selection |

### Input
| Key | Action |
|-----|--------|
| Enter | Send message |
| Ctrk+Enter | Insert newline |
| ← → ↑ ↓ | Move cursor in textarea |
| Ctrl+Backspace / Ctrl+W | Delete word backwards |
| Ctrl+U | Clear input |
| Ctrl+Z / Ctrl+Y | Undo / Redo |

### Streaming
| Key | Action |
|-----|--------|
| Esc | Show stop prompt |
| Esc Esc | Force stop (within 2s) |

### Error
| Key | Action |
|-----|--------|
| ! | Show last error, clear error dot |

## Slash Commands

| Command | Action |
|---------|--------|
| /settings | Open settings panel |
| /provider | Switch LLM provider |
| /model | Switch model |
| /effort | Set reasoning effort |
| /thread | Pick thread |
| /new | New conversation |
| /attach \<path\> | Attach file |
| /view | Cycle transcript mode |
| /help | Show keyboard shortcuts |
| /quit | Exit |

## Settings

7 tabs: Provider, Tools, Web Search, Chat, Gateway, Agent, Advanced

Agent settings load from the daemon and persist through daemon-side per-item updates.

## Architecture

- **ratatui** for terminal rendering
- **crossterm** for input/mouse events
- Decomposed state modules (`ChatState`, `InputState`, `ModalState`, `SidebarState`, `TaskState`, `ConfigState`, `ApprovalState`, `SettingsState`)
- Daemon communication via amux-protocol (Unix socket / TCP)
- 189+ unit tests
