pub(super) fn extended_schema_sql() -> &'static str {
    r#"
            CREATE TABLE IF NOT EXISTS causal_traces (
                id                    TEXT PRIMARY KEY,
                thread_id             TEXT,
                goal_run_id           TEXT,
                task_id               TEXT,
                decision_type         TEXT NOT NULL,
                selected_json         TEXT NOT NULL,
                rejected_options_json TEXT,
                context_hash          TEXT,
                causal_factors_json   TEXT,
                outcome_json          TEXT NOT NULL,
                model_used            TEXT,
                created_at            INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_causal_traces_decision_type ON causal_traces(decision_type, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_causal_traces_task_id ON causal_traces(task_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS memory_provenance (
                id            TEXT PRIMARY KEY,
                target        TEXT NOT NULL,
                mode          TEXT NOT NULL,
                source_kind   TEXT NOT NULL,
                content       TEXT NOT NULL,
                fact_keys_json TEXT NOT NULL DEFAULT '[]',
                thread_id     TEXT,
                task_id       TEXT,
                goal_run_id   TEXT,
                created_at    INTEGER NOT NULL,
                confirmed_at  INTEGER,
                retracted_at  INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_memory_provenance_target_ts ON memory_provenance(target, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_memory_provenance_goal_run ON memory_provenance(goal_run_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS memory_provenance_relationships (
                id              TEXT PRIMARY KEY,
                source_entry_id TEXT NOT NULL,
                target_entry_id TEXT NOT NULL,
                relation_type   TEXT NOT NULL,
                fact_key        TEXT,
                created_at      INTEGER NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_provenance_rel_unique ON memory_provenance_relationships(source_entry_id, target_entry_id, relation_type, fact_key);
            CREATE INDEX IF NOT EXISTS idx_memory_provenance_rel_source ON memory_provenance_relationships(source_entry_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS skill_variants (
                variant_id         TEXT PRIMARY KEY,
                skill_name         TEXT NOT NULL,
                variant_name       TEXT NOT NULL,
                relative_path      TEXT NOT NULL UNIQUE,
                parent_variant_id  TEXT,
                version            TEXT NOT NULL,
                context_tags_json  TEXT NOT NULL DEFAULT '[]',
                use_count          INTEGER NOT NULL DEFAULT 0,
                success_count      INTEGER NOT NULL DEFAULT 0,
                failure_count      INTEGER NOT NULL DEFAULT 0,
                status             TEXT NOT NULL DEFAULT 'active',
                last_used_at       INTEGER,
                created_at         INTEGER NOT NULL,
                updated_at         INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_skill_variants_name ON skill_variants(skill_name, status, updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_skill_variants_path ON skill_variants(relative_path);

            CREATE TABLE IF NOT EXISTS skill_variant_usage (
                usage_id           TEXT PRIMARY KEY,
                variant_id         TEXT NOT NULL,
                thread_id          TEXT,
                task_id            TEXT,
                goal_run_id        TEXT,
                context_tags_json  TEXT NOT NULL DEFAULT '[]',
                consulted_at       INTEGER NOT NULL,
                resolved_at        INTEGER,
                outcome            TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_skill_variant_usage_variant ON skill_variant_usage(variant_id, consulted_at DESC);
            CREATE INDEX IF NOT EXISTS idx_skill_variant_usage_resolution ON skill_variant_usage(task_id, goal_run_id, thread_id, resolved_at);

            CREATE TABLE IF NOT EXISTS collaboration_sessions (
                parent_task_id TEXT PRIMARY KEY,
                session_json   TEXT NOT NULL,
                updated_at     INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_collaboration_sessions_updated ON collaboration_sessions(updated_at DESC);

            CREATE TABLE IF NOT EXISTS gateway_threads (
                channel_key TEXT PRIMARY KEY,
                thread_id   TEXT NOT NULL,
                updated_at  INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_gateway_threads_updated ON gateway_threads(updated_at DESC);

            CREATE TABLE IF NOT EXISTS gateway_channel_modes (
                channel_key TEXT PRIMARY KEY,
                route_mode  TEXT NOT NULL,
                updated_at  INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_gateway_channel_modes_updated ON gateway_channel_modes(updated_at DESC);

            CREATE TABLE IF NOT EXISTS gateway_replay_cursors (
                platform    TEXT NOT NULL,
                channel_id  TEXT NOT NULL,
                cursor_value TEXT NOT NULL,
                cursor_type TEXT NOT NULL,
                updated_at  INTEGER NOT NULL,
                PRIMARY KEY (platform, channel_id)
            );
            CREATE INDEX IF NOT EXISTS idx_gateway_replay_cursors_platform ON gateway_replay_cursors(platform, updated_at DESC);

            CREATE TABLE IF NOT EXISTS gateway_health_snapshots (
                platform    TEXT PRIMARY KEY,
                state_json  TEXT NOT NULL,
                updated_at  INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_gateway_health_snapshots_updated ON gateway_health_snapshots(updated_at DESC);

            CREATE TABLE IF NOT EXISTS whatsapp_provider_state (
                provider_id    TEXT PRIMARY KEY,
                linked_phone   TEXT,
                auth_json      TEXT,
                metadata_json  TEXT,
                last_reset_at  INTEGER,
                last_linked_at INTEGER,
                updated_at     INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_whatsapp_provider_state_updated ON whatsapp_provider_state(updated_at DESC);

            CREATE TABLE IF NOT EXISTS operator_profile_sessions (
                session_id   TEXT PRIMARY KEY,
                kind         TEXT NOT NULL,
                session_json TEXT NOT NULL,
                updated_at   INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_operator_profile_sessions_updated ON operator_profile_sessions(updated_at DESC);

            CREATE TABLE IF NOT EXISTS agent_config_items (
                key_path   TEXT PRIMARY KEY,
                value_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_agent_config_items_updated ON agent_config_items(updated_at DESC);

            CREATE TABLE IF NOT EXISTS agent_config_updates (
                id         TEXT PRIMARY KEY,
                key_path   TEXT NOT NULL,
                value_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_agent_config_updates_key_ts ON agent_config_updates(key_path, updated_at DESC);

            CREATE TABLE IF NOT EXISTS provider_auth_state (
                provider_id TEXT NOT NULL,
                auth_mode   TEXT NOT NULL,
                state_json  TEXT NOT NULL,
                updated_at  INTEGER NOT NULL,
                PRIMARY KEY (provider_id, auth_mode)
            );
            CREATE INDEX IF NOT EXISTS idx_provider_auth_state_updated ON provider_auth_state(updated_at DESC);

            CREATE TABLE IF NOT EXISTS heartbeat_history (
                id              TEXT PRIMARY KEY,
                cycle_timestamp INTEGER NOT NULL,
                checks_json     TEXT NOT NULL,
                synthesis_json  TEXT,
                actionable      INTEGER NOT NULL DEFAULT 0,
                digest_text     TEXT,
                llm_tokens_used INTEGER NOT NULL DEFAULT 0,
                duration_ms     INTEGER NOT NULL DEFAULT 0,
                status          TEXT NOT NULL DEFAULT 'completed'
            );
            CREATE INDEX IF NOT EXISTS idx_heartbeat_history_ts ON heartbeat_history(cycle_timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_heartbeat_history_actionable ON heartbeat_history(actionable, cycle_timestamp DESC);

            CREATE TABLE IF NOT EXISTS action_audit (
                id                TEXT PRIMARY KEY,
                timestamp         INTEGER NOT NULL,
                action_type       TEXT NOT NULL,
                summary           TEXT NOT NULL,
                explanation       TEXT,
                confidence        REAL,
                confidence_band   TEXT,
                causal_trace_id   TEXT,
                thread_id         TEXT,
                goal_run_id       TEXT,
                task_id           TEXT,
                raw_data_json     TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_action_audit_ts ON action_audit(timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_action_audit_type_ts ON action_audit(action_type, timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_action_audit_thread ON action_audit(thread_id, timestamp DESC);

            CREATE TABLE IF NOT EXISTS memory_tombstones (
                id TEXT PRIMARY KEY,
                target TEXT NOT NULL,
                original_content TEXT NOT NULL,
                fact_key TEXT,
                replaced_by TEXT,
                replaced_at INTEGER NOT NULL,
                source_kind TEXT NOT NULL,
                provenance_id TEXT,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_tombstones_created ON memory_tombstones(created_at);
            CREATE INDEX IF NOT EXISTS idx_tombstones_target ON memory_tombstones(target, created_at DESC);

            CREATE TABLE IF NOT EXISTS consolidation_state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS plugins (
                name            TEXT PRIMARY KEY,
                version         TEXT NOT NULL,
                description     TEXT,
                author          TEXT,
                manifest_json   TEXT NOT NULL,
                install_source  TEXT NOT NULL DEFAULT 'local',
                enabled         INTEGER NOT NULL DEFAULT 1,
                installed_at    TEXT NOT NULL,
                updated_at      TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS plugin_settings (
                plugin_name     TEXT NOT NULL,
                key             TEXT NOT NULL,
                value           TEXT NOT NULL,
                is_secret       INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (plugin_name, key),
                FOREIGN KEY (plugin_name) REFERENCES plugins(name) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS plugin_credentials (
                plugin_name      TEXT NOT NULL,
                credential_type  TEXT NOT NULL,
                encrypted_value  BLOB,
                expires_at       TEXT,
                created_at       TEXT NOT NULL,
                updated_at       TEXT NOT NULL,
                PRIMARY KEY (plugin_name, credential_type),
                FOREIGN KEY (plugin_name) REFERENCES plugins(name) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS operator_profile_fields (
                field_key        TEXT PRIMARY KEY,
                field_value_json TEXT NOT NULL,
                confidence       REAL NOT NULL DEFAULT 0.0,
                source           TEXT NOT NULL DEFAULT 'unknown',
                updated_at       INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_op_profile_fields_updated ON operator_profile_fields(updated_at DESC);

            CREATE TABLE IF NOT EXISTS operator_profile_consents (
                consent_key TEXT PRIMARY KEY,
                granted     INTEGER NOT NULL DEFAULT 0,
                updated_at  INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS operator_profile_events (
                id            TEXT PRIMARY KEY,
                event_type    TEXT NOT NULL,
                field_key     TEXT,
                value_json    TEXT,
                source        TEXT NOT NULL DEFAULT 'unknown',
                metadata_json TEXT,
                created_at    INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_op_profile_events_created ON operator_profile_events(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_op_profile_events_field ON operator_profile_events(field_key, created_at DESC);

            CREATE TABLE IF NOT EXISTS operator_profile_checkins (
                id            TEXT PRIMARY KEY,
                kind          TEXT NOT NULL,
                scheduled_at  INTEGER NOT NULL,
                shown_at      INTEGER,
                status        TEXT NOT NULL DEFAULT 'pending',
                response_json TEXT,
                created_at    INTEGER NOT NULL,
                updated_at    INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_op_profile_checkins_scheduled ON operator_profile_checkins(scheduled_at ASC);
            CREATE INDEX IF NOT EXISTS idx_op_profile_checkins_status ON operator_profile_checkins(status, scheduled_at ASC);
    "#
}
