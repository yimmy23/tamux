pub(super) fn base_schema_sql() -> &'static str {
    r#"
            CREATE TABLE IF NOT EXISTS history_entries (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                title TEXT NOT NULL,
                excerpt TEXT NOT NULL,
                content TEXT NOT NULL,
                path TEXT,
                timestamp INTEGER NOT NULL
            );
            CREATE VIRTUAL TABLE IF NOT EXISTS history_fts USING fts5(
                id UNINDEXED,
                title,
                excerpt,
                content
            );
            CREATE TABLE IF NOT EXISTS command_log (
                id           TEXT PRIMARY KEY,
                command      TEXT NOT NULL,
                timestamp    INTEGER NOT NULL,
                path         TEXT,
                cwd          TEXT,
                workspace_id TEXT,
                surface_id   TEXT,
                pane_id      TEXT,
                exit_code    INTEGER,
                duration_ms  INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_command_log_ts ON command_log(timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_command_log_pane ON command_log(pane_id);
            CREATE TABLE IF NOT EXISTS agent_threads (
                id             TEXT PRIMARY KEY,
                workspace_id   TEXT,
                surface_id     TEXT,
                pane_id        TEXT,
                agent_name     TEXT,
                title          TEXT NOT NULL DEFAULT '',
                created_at     INTEGER NOT NULL,
                updated_at     INTEGER NOT NULL,
                message_count  INTEGER NOT NULL DEFAULT 0,
                total_tokens   INTEGER NOT NULL DEFAULT 0,
                last_preview   TEXT NOT NULL DEFAULT '',
                metadata_json  TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_threads_updated ON agent_threads(updated_at DESC);
            CREATE TABLE IF NOT EXISTS agent_messages (
                id              TEXT PRIMARY KEY,
                thread_id       TEXT NOT NULL REFERENCES agent_threads(id) ON DELETE CASCADE,
                created_at      INTEGER NOT NULL,
                role            TEXT NOT NULL,
                content         TEXT NOT NULL DEFAULT '',
                provider        TEXT,
                model           TEXT,
                input_tokens    INTEGER,
                output_tokens   INTEGER,
                total_tokens    INTEGER,
                reasoning       TEXT,
                tool_calls_json TEXT,
                metadata_json   TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_messages_thread ON agent_messages(thread_id, created_at);
            CREATE TABLE IF NOT EXISTS worm_chain_tip (
                kind      TEXT PRIMARY KEY,
                seq       INTEGER NOT NULL,
                hash      TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS agent_events (
                id           TEXT PRIMARY KEY,
                category     TEXT NOT NULL,
                kind         TEXT NOT NULL,
                pane_id      TEXT,
                workspace_id TEXT,
                surface_id   TEXT,
                session_id   TEXT,
                payload_json TEXT NOT NULL,
                timestamp    INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_agent_events_cat ON agent_events(category, timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_agent_events_pane ON agent_events(pane_id, timestamp DESC);
            CREATE TABLE IF NOT EXISTS agent_tasks (
                id                   TEXT PRIMARY KEY,
                title                TEXT NOT NULL,
                description          TEXT NOT NULL,
                status               TEXT NOT NULL,
                priority             TEXT NOT NULL,
                progress             INTEGER NOT NULL DEFAULT 0,
                created_at           INTEGER NOT NULL,
                started_at           INTEGER,
                completed_at         INTEGER,
                error                TEXT,
                result               TEXT,
                thread_id            TEXT,
                source               TEXT NOT NULL DEFAULT 'user',
                notify_on_complete   INTEGER NOT NULL DEFAULT 0,
                notify_channels_json TEXT NOT NULL DEFAULT '[]',
                command              TEXT,
                session_id           TEXT,
                goal_run_id          TEXT,
                goal_run_title       TEXT,
                goal_step_id         TEXT,
                goal_step_title      TEXT,
                parent_task_id       TEXT,
                parent_thread_id     TEXT,
                runtime              TEXT NOT NULL DEFAULT 'daemon',
                retry_count          INTEGER NOT NULL DEFAULT 0,
                max_retries          INTEGER NOT NULL DEFAULT 3,
                next_retry_at        INTEGER,
                scheduled_at         INTEGER,
                blocked_reason       TEXT,
                awaiting_approval_id TEXT,
                policy_fingerprint   TEXT,
                approval_expires_at  INTEGER,
                containment_scope    TEXT,
                compensation_status  TEXT,
                compensation_summary TEXT,
                lane_id              TEXT,
                last_error           TEXT,
                override_provider    TEXT,
                override_model       TEXT,
                override_system_prompt TEXT,
                sub_agent_def_id     TEXT,
                tool_whitelist_json  TEXT,
                tool_blacklist_json  TEXT,
                context_budget_tokens INTEGER,
                context_overflow_action TEXT,
                termination_conditions TEXT,
                success_criteria     TEXT,
                max_duration_secs    INTEGER,
                supervisor_config_json TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_agent_tasks_status ON agent_tasks(status, priority, created_at DESC);
            CREATE TABLE IF NOT EXISTS agent_task_dependencies (
                task_id             TEXT NOT NULL REFERENCES agent_tasks(id) ON DELETE CASCADE,
                depends_on_task_id  TEXT NOT NULL,
                ordinal             INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (task_id, depends_on_task_id)
            );
            CREATE INDEX IF NOT EXISTS idx_agent_task_deps_parent ON agent_task_dependencies(depends_on_task_id);
            CREATE TABLE IF NOT EXISTS agent_task_logs (
                id         TEXT PRIMARY KEY,
                task_id    TEXT NOT NULL REFERENCES agent_tasks(id) ON DELETE CASCADE,
                timestamp  INTEGER NOT NULL,
                level      TEXT NOT NULL,
                phase      TEXT NOT NULL,
                message    TEXT NOT NULL,
                details    TEXT,
                attempt    INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_agent_task_logs_task_ts ON agent_task_logs(task_id, timestamp ASC);
            CREATE TABLE IF NOT EXISTS transcript_index (
                id           TEXT PRIMARY KEY,
                pane_id      TEXT,
                workspace_id TEXT,
                surface_id   TEXT,
                filename     TEXT NOT NULL,
                reason       TEXT,
                captured_at  INTEGER NOT NULL,
                size_bytes   INTEGER,
                preview      TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_transcript_ts ON transcript_index(captured_at DESC);
            CREATE TABLE IF NOT EXISTS snapshot_index (
                snapshot_id  TEXT PRIMARY KEY,
                workspace_id TEXT,
                session_id   TEXT,
                kind         TEXT NOT NULL,
                label        TEXT,
                path         TEXT NOT NULL,
                created_at   INTEGER NOT NULL,
                details_json TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_snapshot_ts ON snapshot_index(created_at DESC);
            CREATE TABLE IF NOT EXISTS goal_runs (
                id                  TEXT PRIMARY KEY,
                title               TEXT NOT NULL,
                goal                TEXT NOT NULL,
                client_request_id   TEXT,
                status              TEXT NOT NULL,
                priority            TEXT NOT NULL,
                created_at          INTEGER NOT NULL,
                updated_at          INTEGER NOT NULL,
                started_at          INTEGER,
                completed_at        INTEGER,
                thread_id           TEXT,
                session_id          TEXT,
                current_step_index  INTEGER NOT NULL DEFAULT 0,
                replan_count        INTEGER NOT NULL DEFAULT 0,
                max_replans         INTEGER NOT NULL DEFAULT 2,
                plan_summary        TEXT,
                reflection_summary  TEXT,
                memory_updates_json TEXT NOT NULL DEFAULT '[]',
                generated_skill_path TEXT,
                last_error          TEXT,
                failure_cause       TEXT,
                child_task_ids_json TEXT NOT NULL DEFAULT '[]',
                child_task_count    INTEGER NOT NULL DEFAULT 0,
                approval_count      INTEGER NOT NULL DEFAULT 0,
                awaiting_approval_id TEXT,
                policy_fingerprint  TEXT,
                approval_expires_at INTEGER,
                containment_scope   TEXT,
                compensation_status TEXT,
                compensation_summary TEXT,
                active_task_id      TEXT,
                duration_ms         INTEGER,
                total_prompt_tokens INTEGER NOT NULL DEFAULT 0,
                total_completion_tokens INTEGER NOT NULL DEFAULT 0,
                estimated_cost_usd  REAL,
                autonomy_level      TEXT NOT NULL DEFAULT 'aware',
                authorship_tag      TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_goal_runs_status ON goal_runs(status, updated_at DESC);
            CREATE TABLE IF NOT EXISTS goal_run_steps (
                id                TEXT PRIMARY KEY,
                goal_run_id       TEXT NOT NULL REFERENCES goal_runs(id) ON DELETE CASCADE,
                ordinal           INTEGER NOT NULL,
                title             TEXT NOT NULL,
                instructions      TEXT NOT NULL,
                kind              TEXT NOT NULL,
                success_criteria  TEXT NOT NULL,
                session_id        TEXT,
                status            TEXT NOT NULL,
                task_id           TEXT,
                summary           TEXT,
                error             TEXT,
                started_at        INTEGER,
                completed_at      INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_goal_run_steps_goal_run ON goal_run_steps(goal_run_id, ordinal ASC);
            CREATE TABLE IF NOT EXISTS goal_run_events (
                id          TEXT PRIMARY KEY,
                goal_run_id TEXT NOT NULL REFERENCES goal_runs(id) ON DELETE CASCADE,
                timestamp   INTEGER NOT NULL,
                phase       TEXT NOT NULL,
                message     TEXT NOT NULL,
                details     TEXT,
                step_index  INTEGER,
                todo_snapshot_json TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_goal_run_events_goal_run_ts ON goal_run_events(goal_run_id, timestamp ASC);
            CREATE TABLE IF NOT EXISTS subagent_metrics (
                task_id              TEXT PRIMARY KEY,
                parent_task_id       TEXT,
                thread_id            TEXT,
                tool_calls_total     INTEGER DEFAULT 0,
                tool_calls_succeeded INTEGER DEFAULT 0,
                tool_calls_failed    INTEGER DEFAULT 0,
                tokens_consumed      INTEGER DEFAULT 0,
                context_budget_tokens INTEGER,
                progress_rate        REAL DEFAULT 0.0,
                last_progress_at     INTEGER,
                stuck_score          REAL DEFAULT 0.0,
                health_state         TEXT DEFAULT 'healthy',
                created_at           INTEGER NOT NULL,
                updated_at           INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS agent_checkpoints (
                id                TEXT PRIMARY KEY,
                goal_run_id       TEXT NOT NULL,
                thread_id         TEXT,
                task_id           TEXT,
                checkpoint_type   TEXT NOT NULL,
                state_json        TEXT NOT NULL,
                context_summary   TEXT,
                created_at        INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_agent_checkpoints_goal_run ON agent_checkpoints(goal_run_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS agent_health_log (
                id            TEXT PRIMARY KEY,
                entity_type   TEXT NOT NULL,
                entity_id     TEXT NOT NULL,
                health_state  TEXT NOT NULL,
                indicators_json TEXT,
                intervention  TEXT,
                created_at    INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_agent_health_log_entity ON agent_health_log(entity_type, entity_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS context_archive (
                id                     TEXT PRIMARY KEY,
                thread_id              TEXT NOT NULL,
                original_role          TEXT,
                compressed_content     TEXT NOT NULL,
                summary                TEXT,
                relevance_score        REAL DEFAULT 0.0,
                token_count_original   INTEGER,
                token_count_compressed INTEGER,
                metadata_json          TEXT,
                archived_at            INTEGER NOT NULL,
                last_accessed_at       INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_context_archive_thread ON context_archive(thread_id, archived_at DESC);

            CREATE TABLE IF NOT EXISTS execution_traces (
                id               TEXT PRIMARY KEY,
                goal_run_id      TEXT,
                task_id          TEXT,
                task_type        TEXT,
                outcome          TEXT,
                quality_score    REAL,
                tool_sequence_json TEXT,
                metrics_json     TEXT,
                duration_ms      INTEGER,
                tokens_used      INTEGER,
                created_at       INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_execution_traces_task_type ON execution_traces(task_type, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_execution_traces_goal_run ON execution_traces(goal_run_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS approval_records (
                approval_id         TEXT PRIMARY KEY,
                run_id              TEXT,
                task_id             TEXT,
                goal_run_id         TEXT,
                thread_id           TEXT,
                transition_kind     TEXT NOT NULL,
                stage_id            TEXT,
                scope_summary       TEXT,
                target_scope_json   TEXT NOT NULL DEFAULT '[]',
                constraints_json    TEXT NOT NULL DEFAULT '[]',
                risk_class          TEXT NOT NULL,
                rationale_json      TEXT NOT NULL DEFAULT '[]',
                policy_fingerprint  TEXT NOT NULL,
                requested_at        INTEGER NOT NULL,
                resolved_at         INTEGER,
                expires_at          INTEGER,
                resolution          TEXT,
                invalidated_at      INTEGER,
                invalidation_reason TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_approval_records_requested_at ON approval_records(requested_at DESC);
            CREATE INDEX IF NOT EXISTS idx_approval_records_policy ON approval_records(policy_fingerprint, requested_at DESC);

            CREATE TABLE IF NOT EXISTS governance_evaluations (
                id                 TEXT PRIMARY KEY,
                run_id             TEXT,
                task_id            TEXT,
                goal_run_id        TEXT,
                thread_id          TEXT,
                transition_kind    TEXT NOT NULL,
                input_json         TEXT NOT NULL,
                verdict_json       TEXT NOT NULL,
                policy_fingerprint TEXT NOT NULL,
                created_at         INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_governance_evaluations_created_at ON governance_evaluations(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_governance_evaluations_policy ON governance_evaluations(policy_fingerprint, created_at DESC);

    "#
}
