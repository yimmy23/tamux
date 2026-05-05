---
name: lint
description: Lint and format code. Auto-detects ESLint, Biome, Prettier, or language-native formatters and runs them with auto-fix. Reports remaining issues with actionable suggestions.

tags: [gsd-2, skills, lint]
---|------|------|---------|
| ... | ... | ... | ... |

### Warnings (X issues)
| File | Line | Rule | Message |
|------|------|------|---------|
| ... | ... | ... | ... |

### Formatting
- X files would be reformatted
- [list files]

### Summary
- Total issues: X errors, Y warnings, Z formatting
- Auto-fixable: N issues (run `/lint --fix` to apply)
```

**Step 4: Suggest fixes for common issues**

For the most frequent issues, provide brief actionable guidance:

- If the same rule appears 5+ times, suggest a bulk fix or config change.
- For unused imports/variables, list them for quick removal.
- For formatting-only issues, note that `--fix` will resolve them safely.
- For issues that cannot be auto-fixed, provide a one-line explanation of how to resolve each unique rule violation.

</execution>

<critical_rules>

1. **Never modify files without `--fix`**: Default mode is report-only. Respect the user's working tree.
2. **Use the project's own config**: Do not invent lint rules. Use whatever config files exist in the project.
3. **Use the project's installed version**: Always prefer `npx`, `cargo`, or the project-local binary. Do not use globally installed tools unless no local version exists.
4. **Handle missing tools gracefully**: If a config file exists but the tool is not installed, inform the user and provide the install command (e.g., `npm install --save-dev eslint`).
5. **Respect `.gitignore` and ignore patterns**: Do not lint `node_modules`, `dist`, `build`, `target`, `.git`, or other commonly ignored directories. Most tools handle this automatically; verify they do.
6. **Limit output**: If there are more than 50 issues, show the first 30 grouped by severity, then summarize the rest with counts per file. Do not flood the user with hundreds of lines.
7. **Exit cleanly**: After presenting results, do not take further action. Let the user decide next steps.

</critical_rules>
