fn wrap_textarea_visual_line(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for ch in text.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        let next_width = current_width + ch_width;
        if !current.is_empty() && next_width > width {
            lines.push(std::mem::take(&mut current));
            current_width = 0;
        }
        current.push(ch);
        current_width += ch_width;
    }

    if current.is_empty() {
        lines.push(String::new());
    } else {
        lines.push(current);
    }

    lines
}

fn pad_visual_width(text: &str, width: usize) -> String {
    let visible_width = unicode_width::UnicodeWidthStr::width(text);
    if visible_width >= width {
        return text.to_string();
    }

    let mut padded = String::with_capacity(text.len() + (width - visible_width));
    padded.push_str(text);
    padded.push_str(&" ".repeat(width - visible_width));
    padded
}

fn render_wrapped_textarea_buffer(
    buffer: &str,
    cursor_line: usize,
    cursor_col: usize,
    width: usize,
) -> Vec<String> {
    let mut visual_lines = Vec::new();

    for (idx, raw_line) in buffer.split('\n').enumerate() {
        let rendered = if idx == cursor_line {
            render_edit_line_with_cursor(raw_line, cursor_col)
        } else {
            raw_line.to_string()
        };
        visual_lines.extend(wrap_textarea_visual_line(&rendered, width.max(1)));
    }

    if visual_lines.is_empty() {
        visual_lines.push(render_edit_line_with_cursor("", 0));
    }

    visual_lines
}

fn render_subagents_tab<'a>(
    content_width: u16,
    settings: &'a SettingsState,
    subagents: &'a crate::state::subagents::SubAgentsState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("  Sub-Agents", theme.fg_active)));
    lines.push(Line::from(Span::styled(
        "  Manage orchestration sub-agent definitions",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));

    if let Some(editor) = subagents.editor.as_ref() {
        let name_is_editing =
            settings.is_editing() && settings.editing_field() == Some("subagent_name");
        let role_is_editing =
            settings.is_editing() && settings.editing_field() == Some("subagent_role");
        let prompt_is_editing =
            settings.is_editing() && settings.editing_field() == Some("subagent_system_prompt");
        let name_value = if name_is_editing {
            format!("{}\u{2588}", settings.edit_buffer())
        } else {
            editor.name.clone()
        };
        let role_value = if role_is_editing {
            settings.edit_buffer().to_string()
        } else {
            editor.role.clone()
        };
        let prompt_value = if prompt_is_editing {
            settings.edit_buffer().to_string()
        } else {
            editor.system_prompt.clone()
        };
        let role_label = crate::state::subagents::find_role_preset(&role_value)
            .map(|preset| preset.label)
            .unwrap_or_else(|| {
                if role_value.trim().is_empty() {
                    "None"
                } else {
                    "Custom"
                }
            });
        let field_line = |selected: bool, label: &str, value: String| {
            Line::from(vec![
                Span::styled(
                    if selected { "> " } else { "  " },
                    if selected {
                        theme.fg_active
                    } else {
                        theme.fg_dim
                    },
                ),
                Span::styled(format!("{label:<14}"), theme.fg_dim),
                Span::styled(
                    value,
                    if selected {
                        theme.fg_active
                    } else {
                        Style::default().fg(Color::White)
                    },
                ),
            ])
        };

        lines.push(field_line(
            matches!(
                editor.field,
                crate::state::subagents::SubAgentEditorField::Name
            ),
            "Name",
            name_value,
        ));
        lines.push(field_line(
            matches!(
                editor.field,
                crate::state::subagents::SubAgentEditorField::Provider
            ),
            "Provider",
            if editor.provider.is_empty() {
                "Select provider".to_string()
            } else {
                editor.provider.clone()
            },
        ));
        lines.push(field_line(
            matches!(
                editor.field,
                crate::state::subagents::SubAgentEditorField::Model
            ),
            "Model",
            if editor.model.is_empty() {
                "Select model".to_string()
            } else {
                editor.model.clone()
            },
        ));
        lines.push(field_line(
            matches!(
                editor.field,
                crate::state::subagents::SubAgentEditorField::ReasoningEffort
            ),
            "Reasoning",
            editor
                .reasoning_effort
                .clone()
                .unwrap_or_else(|| "None".to_string()),
        ));
        lines.push(field_line(
            matches!(
                editor.field,
                crate::state::subagents::SubAgentEditorField::Role
            ),
            "Role",
            format!(
                "{role_label} ({})",
                if role_value.is_empty() {
                    "none"
                } else {
                    &role_value
                }
            ),
        ));
        lines.push(Line::raw(""));
        if prompt_is_editing && settings.is_textarea() {
            let prompt_inner_width = (content_width as usize).saturating_sub(6).max(1);
            let (cursor_line, cursor_col) = settings.edit_cursor_line_col();
            let prompt_lines = render_wrapped_textarea_buffer(
                settings.edit_buffer(),
                cursor_line,
                cursor_col,
                prompt_inner_width,
            );
            lines.push(Line::from(vec![
                Span::styled(
                    if matches!(
                        editor.field,
                        crate::state::subagents::SubAgentEditorField::SystemPrompt
                    ) {
                        "> "
                    } else {
                        "  "
                    },
                    if matches!(
                        editor.field,
                        crate::state::subagents::SubAgentEditorField::SystemPrompt
                    ) {
                        theme.fg_active
                    } else {
                        theme.fg_dim
                    },
                ),
                Span::styled("System Prompt", theme.fg_dim),
                Span::styled(" [Ctrl+S/Ctrl+Enter: save, Esc: cancel]", theme.fg_dim),
            ]));
            lines.push(Line::from(Span::styled(
                format!("  ╭{}╮", "─".repeat(prompt_inner_width + 2)),
                theme.fg_dim,
            )));
            for rendered in prompt_lines {
                lines.push(Line::from(vec![
                    Span::styled("  │ ", theme.fg_dim),
                    Span::styled(pad_visual_width(&rendered, prompt_inner_width), theme.fg_active),
                    Span::styled(" │", theme.fg_dim),
                ]));
            }
            lines.push(Line::from(Span::styled(
                format!("  ╰{}╯", "─".repeat(prompt_inner_width + 2)),
                theme.fg_dim,
            )));
        } else {
            lines.push(Line::from(vec![
                Span::styled(
                    if matches!(
                        editor.field,
                        crate::state::subagents::SubAgentEditorField::SystemPrompt
                    ) {
                        "> "
                    } else {
                        "  "
                    },
                    if matches!(
                        editor.field,
                        crate::state::subagents::SubAgentEditorField::SystemPrompt
                    ) {
                        theme.fg_active
                    } else {
                        theme.fg_dim
                    },
                ),
                Span::styled("System Prompt", theme.fg_dim),
            ]));
            for line in wrap_text(
                if prompt_value.trim().is_empty() {
                    "Optional override. Use Enter to edit."
                } else {
                    &prompt_value
                },
                (content_width as usize).saturating_sub(4).max(20),
            ) {
                lines.push(Line::from(Span::styled(
                    format!("    {line}"),
                    if prompt_value.trim().is_empty() {
                        theme.fg_dim
                    } else {
                        Style::default().fg(Color::White)
                    },
                )));
            }
        }
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled(
                if matches!(
                    editor.field,
                    crate::state::subagents::SubAgentEditorField::Save
                ) {
                    "> "
                } else {
                    "  "
                },
                if matches!(
                    editor.field,
                    crate::state::subagents::SubAgentEditorField::Save
                ) {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
            Span::styled(
                "[Save]",
                if matches!(
                    editor.field,
                    crate::state::subagents::SubAgentEditorField::Save
                ) {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
            Span::raw("  "),
            Span::styled(
                "[Cancel]",
                if matches!(
                    editor.field,
                    crate::state::subagents::SubAgentEditorField::Cancel
                ) {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
        ]));
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled("  ↑↓", theme.fg_active),
            Span::styled(" move  ", theme.fg_dim),
            Span::styled("Enter", theme.fg_active),
            Span::styled(" edit/open  ", theme.fg_dim),
            Span::styled("←→", theme.fg_active),
            Span::styled(" role preset  ", theme.fg_dim),
            Span::styled("s", theme.fg_active),
            Span::styled(" save  ", theme.fg_dim),
            Span::styled("Esc", theme.fg_active),
            Span::styled(" cancel", theme.fg_dim),
        ]));
        return lines;
    }

    if subagents.entries.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No sub-agents configured.",
            theme.fg_dim,
        )));
    } else {
        // Field 0: subagent_list
        for (i, entry) in subagents.entries.iter().enumerate() {
            let is_selected = subagents.selected == i;
            let marker = if is_selected { "> " } else { "  " };
            let dot = if entry.enabled { "● " } else { "○ " };
            let dot_style = if entry.enabled {
                Style::default().fg(Color::Green)
            } else {
                theme.fg_dim
            };
            let role_str = entry
                .role
                .as_deref()
                .map(|r| format!(" [{}]", r))
                .unwrap_or_default();
            let protection_str = if entry.builtin { " [built-in]" } else { "" };
            let edit_label = "[Edit]";
            let delete_label = if entry.delete_allowed {
                "[Delete]"
            } else {
                "[Protected]"
            };
            let toggle_label = if entry.enabled {
                if entry.disable_allowed {
                    "[Disable]"
                } else {
                    "[Locked]"
                }
            } else {
                "[Enable]"
            };
            let left_width = marker.chars().count()
                + dot.chars().count()
                + entry.name.chars().count()
                + format!(" ({}/{})", entry.provider, entry.model)
                    .chars()
                    .count()
                + role_str.chars().count()
                + protection_str.chars().count();
            let actions_width = edit_label.chars().count()
                + 1
                + delete_label.chars().count()
                + 1
                + toggle_label.chars().count();
            let spacer = " ".repeat(
                (content_width as usize)
                    .saturating_sub(left_width + actions_width)
                    .max(1),
            );

            let line = Line::from(vec![
                Span::styled(
                    marker,
                    if is_selected {
                        theme.fg_active
                    } else {
                        theme.fg_dim
                    },
                ),
                Span::styled(dot, dot_style),
                Span::styled(
                    entry.name.clone(),
                    if is_selected {
                        theme.fg_active
                    } else {
                        Style::default().fg(Color::White)
                    },
                ),
                Span::styled(
                    format!(" ({}/{})", entry.provider, entry.model),
                    theme.fg_dim,
                ),
                Span::styled(role_str, Style::default().fg(Color::Cyan)),
                Span::styled(protection_str.to_string(), theme.accent_secondary),
                Span::raw(spacer),
                Span::styled(
                    edit_label,
                    if is_selected && subagents.actions_focused && subagents.action_cursor == 1 {
                        theme.fg_active
                    } else {
                        theme.fg_dim
                    },
                ),
                Span::raw(" "),
                Span::styled(
                    delete_label,
                    if is_selected && subagents.actions_focused && subagents.action_cursor == 2 {
                        theme.fg_active
                    } else {
                        theme.fg_dim
                    },
                ),
                Span::raw(" "),
                Span::styled(
                    toggle_label,
                    if is_selected && subagents.actions_focused && subagents.action_cursor == 3 {
                        theme.fg_active
                    } else {
                        theme.fg_dim
                    },
                ),
            ]);
            lines.push(line);
        }
    }

    lines.push(Line::raw(""));

    // Field 1: subagent_add
    {
        let is_selected = subagents.actions_focused && subagents.action_cursor == 0;
        let marker = if is_selected { "> " } else { "  " };
        lines.push(Line::from(Span::styled(
            format!("{}[Add Sub-Agent]", marker),
            if is_selected {
                theme.fg_active
            } else {
                theme.fg_dim
            },
        )));
    }
    lines.push(Line::from(vec![
        Span::styled("  ", theme.fg_dim),
        Span::styled("a", theme.fg_active),
        Span::styled(" add  ", theme.fg_dim),
        Span::styled("e", theme.fg_active),
        Span::styled(" edit  ", theme.fg_dim),
        Span::styled("d", theme.fg_active),
        Span::styled(" delete  ", theme.fg_dim),
        Span::styled("Space", theme.fg_active),
        Span::styled(" toggle  ", theme.fg_dim),
        Span::styled("←→", theme.fg_active),
        Span::styled(" row action", theme.fg_dim),
    ]));

    lines
}
