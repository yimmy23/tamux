# Phase 4 Moat — Spec

**Phase:** 4  
**Status:** Speculative  
**Prerequisites:** Phase 2 (M4, M5)

---

## M10: Runtime Tool Synthesis

### Vision

No agent framework generates its own tool surface. Skills are passive. This makes tamux **active**: the agent grows its own capability frontier based on what's needed in the operator's environment.

When the agent encounters an API, CLI, or system it doesn't have a tool for, it can **generate, sandbox, deploy, and persist** a new tool — usable in the current session and available for future sessions.

### The Pipeline

```
┌─────────────────────────────────────────────────────────────────┐
│ 1. RECOGNITION                                                  │
│ Agent encounters: "I need to query the Kubernetes cluster"      │
│ No built-in tool exists for K8s.                                │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 2. SPEC GENERATION                                               │
│ Generate tool definition from API introspect or CLI --help:       │
│                                                                 │
│ ToolSpec {                                                       │
│   name: "k8s_get_pods",                                         │
│   description: "List pods in a Kubernetes namespace",            │
│   parameters: [                                                 │
│     { name: "namespace", type: "string", required: true },    │
│     { name: "label_selector", type: "string" },                │
│   ],                                                             │
│   returns: "PodList",                                           │
│   source: "kubectl get pods --help",                            │
│ }                                                                │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 3. SANDBOXED DEPLOYMENT                                          │
│ Deploy to generated tool registry (initially read-only or       │
│ limited execution):                                              │
│                                                                 │
│ ~/.tamux/tools/generated/k8s_get_pods/                          │
│ ├── tool.json          # ToolSpec                               │
│ ├── implementation.sh  # Generated wrapper                      │
│ └── test.sh            # Validation suite                       │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 4. EFFECTIVENESS TRACKING                                        │
│ Track success across sessions:                                   │
│                                                                 │
│ GeneratedTool {                                                  │
│   tool_id: "k8s_get_pods",                                      │
│   sessions_used: 5,                                             │
│   success_rate: 0.8,                                           │
│   status: Active | NeedsReview | Archived,                      │
│ }                                                                │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ 5. PROMOTION                                                     │
│ Useful tools (success_rate > 0.8, used 3+ sessions) →           │
│ Skills library or built-in tool candidate                       │
└─────────────────────────────────────────────────────────────────┘
```

### Tool Spec Generation

#### 10.1 CLI Introspection

```rust
async fn generate_from_cli(command: &str) -> Result<ToolSpec, ToolSynthesisError> {
    // Run command with --help or --usage
    let help_output = run_command(format!("{} --help", command)).await?;
    
    // Parse help text (heuristic for common CLI patterns)
    let spec = parse_cli_help(command, help_output)?;
    
    // Validate: can we at least get a dry run?
    let dry_run = run_command(format!("{} --dry-run 2>&1 || true", command)).await?;
    spec.dry_run_output = Some(dry_run);
    
    Ok(spec)
}

fn parse_cli_help(command: &str, help_text: &str) -> Result<ToolSpec, ToolSynthesisError> {
    // Heuristic parsing for common CLI conventions:
    // - "Usage: cmd [OPTIONS]"
    // - "-n, --name <TEXT>  Description"
    // - "Options:"
    
    let mut params = Vec::new();
    for line in help_text.lines() {
        if let Some(param) = parse_flag_or_option(line) {
            params.push(param);
        }
    }
    
    Ok(ToolSpec {
        name: sanitize_name(command),
        description: extract_description(help_text),
        parameters: params,
        // ...
    })
}
```

#### 10.2 API Introspection

```rust
async fn generate_from_api(base_url: &str) -> Result<ToolSpec, ToolSynthesisError> {
    // Try common introspect endpoints
    let spec = if let Ok(spec) = fetch_openapi(base_url).await {
        spec
    } else if let Ok(spec) = fetch_swagger(base_url).await {
        spec
    } else {
        return Err(ToolSynthesisError::NoIntrospection(base_url));
    };
    
    Ok(spec)
}
```

#### 10.3 Shell Command Wrapping

```rust
fn generate_implementation(spec: &ToolSpec) -> String {
    let mut sh = String::new();
    
    sh.push_str("#!/bin/bash\n");
    sh.push_str(&format!("# Generated tool: {}\n", spec.name));
    sh.push_str(&format!("# Description: {}\n\n", spec.description));
    
    // Generate argument parsing
    sh.push_str("set -e\n\n");
    sh.push_str("NAMESPACE=\"${NAMESPACE:-default}\"\n");
    sh.push_str("LABEL_SELECTOR=\"${LABEL_SELECTOR:-}\"\n\n");
    
    // Generate command
    sh.push_str(&format!(
        "kubectl get pods -n \"$NAMESPACE\" \\\n  ${{LABEL_SELECTOR:+-l \"$LABEL_SELECTOR\"}}\n"
    ));
    
    sh
}
```

### Sandboxed Registry

```rust
struct GeneratedToolRegistry {
    tools: HashMap<ToolId, GeneratedTool>,
    sandbox_policy: SandboxPolicy,
}

struct SandboxPolicy {
    max_execution_time: Duration,
    allowed_network: NetworkPolicy,
    allowed_filesystem: FsPolicy,
    max_output_size: usize,
}

enum NetworkPolicy {
    None,
    AllowPrefix(Vec<String>),  // Allow connections to these hosts/prefixes
    All,
}

impl GeneratedToolRegistry {
    fn register(&mut self, spec: ToolSpec, impl_script: String) -> ToolId {
        let id = ToolId::new(&spec.name);
        
        // Write to sandboxed directory
        let tool_dir = format!("~/.tamux/tools/generated/{}/", id);
        fs::write(tool_dir.join("tool.json"), &spec)?;
        fs::write(tool_dir.join("implementation.sh"), &impl_script)?;
        
        // Mark as initially read-only
        self.tools.insert(id.clone(), GeneratedTool {
            spec,
            status: GeneratedToolStatus::New,
            sandbox_policy: self.sandbox_policy.clone(),
        });
        
        id
    }
    
    async fn execute(&self, id: &ToolId, params: Value) -> Result<Value, ToolError> {
        let tool = self.tools.get(id).ok_or(ToolError::NotFound)?;
        
        // Enforce sandbox policy
        self.check_policy(&tool.sandbox_policy)?;
        
        // Execute with timeout
        let output = tokio::time::timeout(
            tool.sandbox_policy.max_execution_time,
            run_tool_script(id, params)
        ).await?;
        
        Ok(output)
    }
}
```

### Effectiveness Tracking

```rust
struct GeneratedTool {
    id: ToolId,
    spec: ToolSpec,
    
    // Usage metrics
    sessions_used: u32,
    calls_total: u32,
    calls_success: u32,
    calls_failure: u32,
    calls_timeout: u32,
    
    // Quality signals
    operator_rating: Option<f64>,      // If operator explicitly rated
    implicit_success: f64,             // Derived from call patterns
    error_patterns: Vec<ErrorPattern>,
    
    // Lifecycle
    status: GeneratedToolStatus,
    created_at: DateTime<Utc>,
    last_used: DateTime<Utc>,
}

enum GeneratedToolStatus {
    New,           // Never used, needs validation
    Active,        // Working, available for use
    NeedsReview,   // Has some failures, needs operator review
    Archived,      // Poor performance, disabled
    Promoted,      // Graduated to skills or built-in
}

impl GeneratedTool {
    fn compute_effectiveness(&self) -> f64 {
        if self.calls_total == 0 {
            return 0.5;  // Neutral for new tools
        }
        
        let success_rate = self.calls_success as f64 / self.calls_total as f64;
        let recency = self.compute_recency_score();
        let quality = self.compute_quality_score();
        
        // Weighted combination
        success_rate * 0.6 + recency * 0.2 + quality * 0.2
    }
    
    fn should_promote(&self) -> bool {
        self.compute_effectiveness() > 0.85
            && self.sessions_used >= 3
            && self.error_patterns.len() < 2
    }
}
```

### Skill Promotion Path

```rust
fn promote_to_skill(tool: &GeneratedTool) -> Result<Skill, ToolSynthesisError> {
    let skill_content = format!(
        r#"# Skill: Use {name}

## When to Use
- {when_to_use}

## How
```bash
{implementation}
```

## Parameters
{parameters}

## Notes
- Generated on {date} from {source}
- Effectiveness: {effectiveness:.0%}
- Used in {sessions} sessions

## Lessons Learned
{lessons}
"#
    );
    
    let skill = Skill {
        name: format!("use-{}", tool.spec.name),
        content: skill_content,
        source: SkillSource::GeneratedTool(tool.id.clone()),
        // ...
    };
    
    // Save to skills library
    save_skill(&skill)?;
    
    // Update tool status
    self.update_status(&tool.id, GeneratedToolStatus::Promoted);
    
    Ok(skill)
}
```

### Integration with Tool Selection

```rust
fn get_all_available_tools(context: &ExecutionContext) -> Vec<Tool> {
    let mut tools = Vec::new();
    
    // Built-in tools
    tools.extend(get_builtin_tools());
    
    // Loaded skills
    tools.extend(get_skill_tools());
    
    // Generated tools (filtered by effectiveness)
    let generated = generated_tool_registry.get_effective_tools();
    tools.extend(generated);
    
    // Filter by tool filter if set (M1/M3 context)
    if let Some(filter) = &context.tool_filter {
        tools.retain(|t| filter.allows(t));
    }
    
    tools
}
```

### Operator Controls

```yaml
# ~/.tamux/config.toml
[tool_synthesis]
enabled = true                    # Allow runtime tool generation
require_approval = true           # Ask before deploying new tool
auto_promote_threshold = 0.85     # Effectiveness to auto-promote
max_sandboxed_tools = 20          # Cap on generated tools

[tool_synthesis.sandbox]
max_execution_time_secs = 30
network = "allow_prefix:k8s-api.internal,metrics.internal"
allow_filesystem = false          # No file access by default
max_output_kb = 512
```

### Wire Into Existing Modules

- `agent/context/` → tool need detection (encountered API/CLI without tool)
- `agent/learning/effectiveness.rs` → generated tool tracking
- `~/.tamux/skills/generated/` → promotion target
- `agent/agent_loop.rs` → inject generated tools into schema
- Plugin system → runtime registration

---

## Milestones

- [ ] M10.1: Tool spec generation from CLI --help
- [ ] M10.2: Tool spec generation from OpenAPI/Swagger
- [ ] M10.3: Shell command wrapper generation
- [ ] M10.4: Sandboxed tool registry
- [ ] M10.5: Effectiveness tracking per tool
- [ ] M10.6: Approval-gated deployment
- [ ] M10.7: Promotion to skills library
- [ ] M10.8: MCP tool export from generated tools

---

## Dependencies

```
M5 (Semantic Environment) ──→ M10 (tool need detection, spec generation)
M4 (Genetic Skills)        ──→ M10 (promotion path)
M3 (Causal Traces)         ──→ M10 (effectiveness attribution)
```

---

## Risk Assessment

| Risk | Level | Mitigation |
|------|-------|------------|
| Code injection | 🔴 High | Sandboxed execution, no arbitrary code |
| API abuse | 🔴 High | Network allowlist, rate limits |
| Hallucinated tools | 🟡 Medium | Require dry-run validation before deploy |
| Context pollution | 🟡 Medium | Cap total generated tools, prune old ones |

---

## Test Plan

1. **CLI tool**: Generate tool from `kubectl --help`, use to list pods
2. **API tool**: Generate from OpenAPI spec, use against test API
3. **Sandbox test**: Try to break out of sandbox, verify containment
4. **Effectiveness test**: Run 10 sessions with generated tool, verify tracking
5. **Promotion test**: Achieve 85% effectiveness, verify promotion to skill
