use super::schema_helpers::{ensure_column, table_has_column};
use rusqlite::{Connection, OptionalExtension};
use std::path::Path;

const OFFLOADED_PAYLOADS_TABLE_SQL: &str = "CREATE TABLE offloaded_payloads (
    payload_id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    tool_call_id TEXT,
    storage_path TEXT NOT NULL,
    content_type TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    summary TEXT NOT NULL,
    created_at INTEGER NOT NULL
)";

const OFFLOADED_PAYLOADS_TABLE_IF_MISSING_SQL: &str =
    "CREATE TABLE IF NOT EXISTS offloaded_payloads (
    payload_id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    tool_call_id TEXT,
    storage_path TEXT NOT NULL,
    content_type TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    summary TEXT NOT NULL,
    created_at INTEGER NOT NULL
)";

const OFFLOADED_PAYLOADS_INDEX_SQL: &str = "CREATE INDEX IF NOT EXISTS idx_offloaded_payloads_thread_created ON offloaded_payloads(thread_id, created_at DESC)";

fn offloaded_payloads_summary_is_required(connection: &Connection) -> rusqlite::Result<bool> {
    let summary_notnull = connection
        .query_row(
            "SELECT \"notnull\" FROM pragma_table_info('offloaded_payloads') WHERE name = 'summary'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(0);
    Ok(summary_notnull == 1)
}

fn canonical_offloaded_payload_storage_path(
    offloaded_payloads_dir: &Path,
    thread_id: &str,
    payload_id: &str,
) -> String {
    offloaded_payloads_dir
        .join(thread_id)
        .join(format!("{payload_id}.txt"))
        .to_string_lossy()
        .into_owned()
}

fn rebuild_offloaded_payloads_table(
    connection: &Connection,
    offloaded_payloads_dir: &Path,
) -> rusqlite::Result<()> {
    let transaction = connection.unchecked_transaction()?;

    transaction.execute_batch(&format!(
        "ALTER TABLE offloaded_payloads RENAME TO offloaded_payloads_legacy;
         {OFFLOADED_PAYLOADS_TABLE_SQL};"
    ))?;

    let legacy_rows = {
        let mut stmt = transaction.prepare(
            "SELECT payload_id, thread_id, tool_name, tool_call_id, content_type, byte_size, summary, created_at
             FROM offloaded_payloads_legacy",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };

    let mut insert_stmt = transaction.prepare(
        "INSERT INTO offloaded_payloads (
             payload_id,
             thread_id,
             tool_name,
             tool_call_id,
             storage_path,
             content_type,
             byte_size,
             summary,
             created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;

    for (
        payload_id,
        thread_id,
        tool_name,
        tool_call_id,
        content_type,
        byte_size,
        summary,
        created_at,
    ) in legacy_rows
    {
        let storage_path = canonical_offloaded_payload_storage_path(
            offloaded_payloads_dir,
            &thread_id,
            &payload_id,
        );
        insert_stmt.execute(rusqlite::params![
            payload_id,
            thread_id,
            tool_name,
            tool_call_id,
            storage_path,
            content_type,
            byte_size,
            summary.unwrap_or_default(),
            created_at,
        ])?;
    }

    drop(insert_stmt);
    transaction.execute_batch(OFFLOADED_PAYLOADS_INDEX_SQL)?;
    transaction.commit()
}

fn ensure_offloaded_payloads_schema(
    connection: &Connection,
    offloaded_payloads_dir: &Path,
) -> rusqlite::Result<()> {
    connection.execute_batch(&format!("{OFFLOADED_PAYLOADS_TABLE_IF_MISSING_SQL};"))?;
    if table_has_column(connection, "offloaded_payloads", "summary")?
        && !offloaded_payloads_summary_is_required(connection)?
    {
        rebuild_offloaded_payloads_table(connection, offloaded_payloads_dir)?;
    }
    connection.execute_batch(&format!("{OFFLOADED_PAYLOADS_INDEX_SQL};"))?;
    Ok(())
}

pub(super) fn ensure_context_archive_fts(connection: &Connection) {
    connection
        .execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS context_archive_fts USING fts5(summary, compressed_content, content=context_archive, content_rowid=rowid);",
        )
        .ok();
}

pub(super) fn prepare_extended_schema_migrations(connection: &Connection) -> rusqlite::Result<()> {
    if !table_has_column(connection, "external_runtime_profiles", "runtime")? {
        return Ok(());
    }

    ensure_column(
        connection,
        "external_runtime_profiles",
        "session_id",
        "TEXT",
    )?;
    ensure_column(
        connection,
        "external_runtime_profiles",
        "source_config_path",
        "TEXT",
    )?;
    ensure_column(
        connection,
        "external_runtime_profiles",
        "source_fingerprint",
        "TEXT",
    )?;
    Ok(())
}

pub(super) fn apply_schema_migrations(
    connection: &Connection,
    offloaded_payloads_dir: &Path,
) -> rusqlite::Result<()> {
    ensure_offloaded_payloads_schema(connection, offloaded_payloads_dir)?;
    ensure_column(connection, "approval_inbox", "gateway_surface", "TEXT")?;
    ensure_column(connection, "approval_inbox", "gateway_channel", "TEXT")?;
    ensure_column(connection, "approval_inbox", "gateway_thread", "TEXT")?;
    ensure_column(connection, "approval_inbox", "rendered_prompt", "TEXT")?;
    ensure_column(
        connection,
        "external_runtime_profiles",
        "session_id",
        "TEXT",
    )?;
    ensure_column(
        connection,
        "external_runtime_profiles",
        "source_config_path",
        "TEXT",
    )?;
    ensure_column(
        connection,
        "external_runtime_profiles",
        "source_fingerprint",
        "TEXT",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS external_runtime_import_sessions (
            session_id TEXT PRIMARY KEY,
            runtime TEXT NOT NULL,
            source_config_path TEXT NOT NULL,
            source_fingerprint TEXT NOT NULL,
            dry_run INTEGER NOT NULL DEFAULT 0,
            conflict_policy TEXT NOT NULL,
            source_surface TEXT NOT NULL,
            session_json TEXT NOT NULL,
            imported_at_ms INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_external_runtime_import_sessions_runtime ON external_runtime_import_sessions(runtime, updated_at DESC);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_external_runtime_import_sessions_fingerprint ON external_runtime_import_sessions(runtime, source_config_path, source_fingerprint, dry_run);
        CREATE TABLE IF NOT EXISTS imported_runtime_assets (
            asset_id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            runtime TEXT NOT NULL,
            asset_kind TEXT NOT NULL,
            bucket TEXT NOT NULL,
            severity TEXT NOT NULL,
            recommended_action TEXT,
            source_path TEXT,
            source_fingerprint TEXT,
            conflict_policy TEXT NOT NULL,
            asset_json TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_imported_runtime_assets_session ON imported_runtime_assets(session_id, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_imported_runtime_assets_runtime_kind ON imported_runtime_assets(runtime, asset_kind, updated_at DESC);
        CREATE TABLE IF NOT EXISTS external_runtime_shadow_runs (
            run_id TEXT PRIMARY KEY,
            runtime TEXT NOT NULL,
            session_id TEXT NOT NULL,
            workflow TEXT NOT NULL,
            readiness_score INTEGER NOT NULL,
            blocker_count INTEGER NOT NULL,
            summary TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_external_runtime_shadow_runs_runtime ON external_runtime_shadow_runs(runtime, created_at_ms DESC);
        CREATE INDEX IF NOT EXISTS idx_external_runtime_shadow_runs_session ON external_runtime_shadow_runs(session_id, created_at_ms DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS embedding_jobs (
            source_kind TEXT NOT NULL,
            source_id TEXT NOT NULL,
            chunk_id TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            title TEXT NOT NULL,
            body TEXT NOT NULL,
            workspace_id TEXT,
            thread_id TEXT,
            agent_id TEXT,
            source_timestamp INTEGER NOT NULL,
            queued_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            claimed_at INTEGER,
            attempts INTEGER NOT NULL DEFAULT 0,
            last_error TEXT,
            PRIMARY KEY (source_kind, source_id, chunk_id)
        );
        CREATE TABLE IF NOT EXISTS embedding_job_completions (
            source_kind TEXT NOT NULL,
            source_id TEXT NOT NULL,
            chunk_id TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            embedding_model TEXT NOT NULL,
            dimensions INTEGER NOT NULL,
            completed_at INTEGER NOT NULL,
            PRIMARY KEY (source_kind, source_id, chunk_id, embedding_model, dimensions)
        );
        CREATE INDEX IF NOT EXISTS idx_embedding_jobs_updated ON embedding_jobs(updated_at ASC);
        CREATE INDEX IF NOT EXISTS idx_embedding_jobs_claimed ON embedding_jobs(claimed_at, updated_at);
        CREATE TABLE IF NOT EXISTS embedding_deletions (
            source_kind TEXT NOT NULL,
            source_id TEXT NOT NULL,
            queued_at INTEGER NOT NULL,
            claimed_at INTEGER,
            attempts INTEGER NOT NULL DEFAULT 0,
            last_error TEXT,
            PRIMARY KEY (source_kind, source_id)
        );
        CREATE INDEX IF NOT EXISTS idx_embedding_deletions_claimed ON embedding_deletions(claimed_at, queued_at);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS thread_structural_memory (
            thread_id TEXT PRIMARY KEY,
            state_json TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_thread_structural_memory_updated ON thread_structural_memory(updated_at DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory_nodes (
            id TEXT PRIMARY KEY,
            label TEXT NOT NULL,
            node_type TEXT NOT NULL,
            embedding_blob BLOB,
            created_at_ms INTEGER NOT NULL,
            last_accessed_ms INTEGER NOT NULL,
            access_count INTEGER NOT NULL DEFAULT 0,
            summary_text TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_memory_nodes_type_accessed ON memory_nodes(node_type, last_accessed_ms DESC);
        CREATE TABLE IF NOT EXISTS memory_edges (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_node_id TEXT NOT NULL,
            target_node_id TEXT NOT NULL,
            relation_type TEXT NOT NULL,
            weight REAL NOT NULL DEFAULT 1.0,
            last_updated_ms INTEGER NOT NULL,
            FOREIGN KEY (source_node_id) REFERENCES memory_nodes(id),
            FOREIGN KEY (target_node_id) REFERENCES memory_nodes(id)
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_edges_unique ON memory_edges(source_node_id, target_node_id, relation_type);
        CREATE INDEX IF NOT EXISTS idx_memory_edges_source_updated ON memory_edges(source_node_id, last_updated_ms DESC);
        CREATE INDEX IF NOT EXISTS idx_memory_edges_target_updated ON memory_edges(target_node_id, last_updated_ms DESC);"
    )?;
    ensure_column(connection, "agent_tasks", "session_id", "TEXT")?;
    ensure_column(connection, "agent_threads", "metadata_json", "TEXT")?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS skill_variant_history (
            id TEXT PRIMARY KEY,
            variant_id TEXT NOT NULL,
            recorded_at INTEGER NOT NULL,
            outcome TEXT NOT NULL,
            fitness_score REAL NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_skill_variant_history_variant_ts ON skill_variant_history(variant_id, recorded_at DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS gene_pool (
            parent_a TEXT NOT NULL,
            parent_b TEXT NOT NULL,
            offspring_id TEXT NOT NULL,
            lifecycle_state TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            PRIMARY KEY (parent_a, parent_b)
        );
        CREATE INDEX IF NOT EXISTS idx_gene_pool_offspring ON gene_pool(offspring_id, created_at DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS gene_fitness_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            variant_id TEXT NOT NULL,
            recorded_at_ms INTEGER NOT NULL,
            fitness_score REAL NOT NULL,
            use_count INTEGER NOT NULL,
            success_rate REAL NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_gene_fitness_variant_ts ON gene_fitness_history(variant_id, recorded_at_ms DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS gene_crossbreeds (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            left_parent_variant_id TEXT NOT NULL,
            right_parent_variant_id TEXT NOT NULL,
            skill_name TEXT NOT NULL,
            co_usage_rate REAL NOT NULL,
            proposed_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_gene_crossbreeds_skill_ts ON gene_crossbreeds(skill_name, proposed_at_ms DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS morphogenesis_affinities (
            agent_id TEXT NOT NULL,
            domain TEXT NOT NULL,
            affinity_score REAL NOT NULL DEFAULT 0.0,
            task_count INTEGER NOT NULL DEFAULT 0,
            success_count INTEGER NOT NULL DEFAULT 0,
            failure_count INTEGER NOT NULL DEFAULT 0,
            last_updated_ms INTEGER NOT NULL,
            PRIMARY KEY (agent_id, domain)
        );
        CREATE INDEX IF NOT EXISTS idx_morphogenesis_domain_updated ON morphogenesis_affinities(domain, last_updated_ms DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS affinity_updates_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            agent_id TEXT NOT NULL,
            domain TEXT NOT NULL,
            old_affinity REAL NOT NULL,
            new_affinity REAL NOT NULL,
            trigger_type TEXT NOT NULL,
            task_id TEXT,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_affinity_updates_agent_domain_ts ON affinity_updates_log(agent_id, domain, updated_at_ms DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS soul_adaptations_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            agent_id TEXT NOT NULL,
            domain TEXT NOT NULL,
            adaptation_type TEXT NOT NULL,
            soul_snippet TEXT NOT NULL,
            old_soul_hash TEXT,
            new_soul_hash TEXT,
            created_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_soul_adaptations_agent_ts ON soul_adaptations_log(agent_id, created_at_ms DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS consensus_bid_priors (
            role TEXT PRIMARY KEY,
            success_count INTEGER NOT NULL DEFAULT 0,
            failure_count INTEGER NOT NULL DEFAULT 0,
            prior_score REAL NOT NULL DEFAULT 0.5,
            last_updated_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_consensus_bid_priors_updated ON consensus_bid_priors(last_updated_ms DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS consensus_bids (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id TEXT NOT NULL,
            round_id INTEGER NOT NULL,
            agent_id TEXT NOT NULL,
            confidence REAL NOT NULL,
            reasoning TEXT,
            availability TEXT NOT NULL,
            domain_affinity REAL NOT NULL DEFAULT 0.0,
            submitted_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_consensus_bids_task_round ON consensus_bids(task_id, round_id, submitted_at_ms DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS role_assignments (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id TEXT NOT NULL,
            round_id INTEGER NOT NULL,
            primary_agent_id TEXT NOT NULL,
            reviewer_agent_id TEXT,
            observers TEXT NOT NULL DEFAULT '[]',
            assigned_at_ms INTEGER NOT NULL,
            outcome TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_role_assignments_task_round ON role_assignments(task_id, round_id, assigned_at_ms DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS consensus_quality_metrics (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id TEXT NOT NULL,
            predicted_confidence REAL NOT NULL,
            actual_outcome_score REAL NOT NULL,
            prediction_error REAL NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_consensus_quality_task_ts ON consensus_quality_metrics(task_id, updated_at_ms DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS cognitive_resonance_samples (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            sampled_at_ms INTEGER NOT NULL,
            revision_velocity_ms INTEGER,
            session_entropy REAL,
            approval_latency_ms INTEGER,
            tool_hesitation_count INTEGER NOT NULL DEFAULT 0,
            cognitive_state TEXT NOT NULL,
            state_confidence REAL NOT NULL,
            resonance_score REAL NOT NULL,
            verbosity_adjustment REAL NOT NULL,
            risk_adjustment REAL NOT NULL,
            proactiveness_adjustment REAL NOT NULL,
            memory_urgency_adjustment REAL NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_cognitive_resonance_samples_sampled ON cognitive_resonance_samples(sampled_at_ms DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS behavior_adjustments_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            adjusted_at_ms INTEGER NOT NULL,
            parameter TEXT NOT NULL,
            old_value REAL NOT NULL,
            new_value REAL NOT NULL,
            trigger_reason TEXT NOT NULL,
            resonance_score REAL NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_behavior_adjustments_log_adjusted ON behavior_adjustments_log(adjusted_at_ms DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS intent_models (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            agent_id TEXT NOT NULL UNIQUE,
            model_blob BLOB,
            created_at_ms INTEGER NOT NULL,
            accuracy_score REAL
        );
        CREATE INDEX IF NOT EXISTS idx_intent_models_agent_created ON intent_models(agent_id, created_at_ms DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS temporal_patterns (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            pattern_type TEXT NOT NULL,
            timescale TEXT NOT NULL,
            pattern_description TEXT NOT NULL,
            context_filter TEXT,
            frequency INTEGER NOT NULL DEFAULT 1,
            last_observed_ms INTEGER NOT NULL,
            first_observed_ms INTEGER NOT NULL,
            confidence REAL NOT NULL,
            decay_rate REAL NOT NULL DEFAULT 0.01,
            created_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_temporal_patterns_type_scale ON temporal_patterns(pattern_type, timescale, created_at_ms DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS temporal_predictions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            pattern_id INTEGER NOT NULL,
            predicted_action TEXT NOT NULL,
            predicted_at_ms INTEGER NOT NULL,
            confidence REAL NOT NULL,
            actual_action TEXT,
            was_accepted INTEGER,
            accuracy_score REAL,
            FOREIGN KEY (pattern_id) REFERENCES temporal_patterns(id)
        );
        CREATE INDEX IF NOT EXISTS idx_temporal_predictions_pattern_predicted ON temporal_predictions(pattern_id, predicted_at_ms DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS precomputation_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            prediction_id INTEGER NOT NULL,
            precomputation_type TEXT NOT NULL,
            precomputation_details TEXT NOT NULL,
            started_at_ms INTEGER NOT NULL,
            completed_at_ms INTEGER,
            was_used INTEGER,
            FOREIGN KEY (prediction_id) REFERENCES temporal_predictions(id)
        );
        CREATE INDEX IF NOT EXISTS idx_precomputation_log_prediction_started ON precomputation_log(prediction_id, started_at_ms DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS dream_cycles (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            started_at_ms INTEGER NOT NULL,
            completed_at_ms INTEGER,
            idle_duration_ms INTEGER NOT NULL,
            tasks_analyzed INTEGER NOT NULL,
            counterfactuals_generated INTEGER NOT NULL,
            counterfactuals_successful INTEGER NOT NULL,
            status TEXT NOT NULL DEFAULT 'running'
        );
        CREATE INDEX IF NOT EXISTS idx_dream_cycles_started ON dream_cycles(started_at_ms DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS counterfactual_evaluations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            dream_cycle_id INTEGER NOT NULL,
            source_task_id TEXT NOT NULL,
            variation_type TEXT NOT NULL,
            counterfactual_description TEXT NOT NULL,
            estimated_token_saving REAL,
            estimated_time_saving_ms INTEGER,
            estimated_revision_reduction INTEGER,
            score REAL NOT NULL,
            threshold_met INTEGER NOT NULL,
            created_at_ms INTEGER NOT NULL,
            FOREIGN KEY (dream_cycle_id) REFERENCES dream_cycles(id)
        );
        CREATE INDEX IF NOT EXISTS idx_counterfactual_evaluations_cycle ON counterfactual_evaluations(dream_cycle_id, created_at_ms DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS event_triggers (
            id TEXT PRIMARY KEY,
            event_family TEXT NOT NULL,
            event_kind TEXT NOT NULL,
            agent_id TEXT,
            target_state TEXT,
            thread_id TEXT,
            enabled INTEGER NOT NULL DEFAULT 1,
            cooldown_secs INTEGER NOT NULL DEFAULT 0,
            risk_label TEXT NOT NULL DEFAULT 'low',
            notification_kind TEXT NOT NULL,
            prompt_template TEXT,
            title_template TEXT NOT NULL,
            body_template TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            last_fired_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_event_triggers_family_kind_enabled ON event_triggers(event_family, event_kind, enabled, updated_at DESC);",
    )?;
    ensure_column(connection, "event_triggers", "agent_id", "TEXT")?;
    ensure_column(connection, "event_triggers", "prompt_template", "TEXT")?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS routine_definitions (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            paused_at INTEGER,
            schedule_expression TEXT NOT NULL,
            target_kind TEXT NOT NULL,
            target_payload_json TEXT NOT NULL,
            schema_version INTEGER NOT NULL DEFAULT 1,
            next_run_at INTEGER,
            last_run_at INTEGER,
            last_result TEXT,
            last_error TEXT,
            last_success_summary TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_routine_definitions_enabled_next_run ON routine_definitions(enabled, next_run_at, updated_at DESC);",
    )?;
    ensure_column(
        connection,
        "routine_definitions",
        "schema_version",
        "INTEGER NOT NULL DEFAULT 1",
    )?;
    ensure_column(connection, "routine_definitions", "last_result", "TEXT")?;
    ensure_column(connection, "routine_definitions", "last_error", "TEXT")?;
    ensure_column(
        connection,
        "routine_definitions",
        "last_success_summary",
        "TEXT",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS routine_runs (
            id TEXT PRIMARY KEY,
            routine_id TEXT NOT NULL,
            trigger_kind TEXT NOT NULL,
            status TEXT NOT NULL,
            started_at INTEGER NOT NULL,
            finished_at INTEGER,
            created_task_id TEXT,
            created_goal_run_id TEXT,
            payload_json TEXT NOT NULL,
            result_summary TEXT,
            error TEXT,
            rerun_of_run_id TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_routine_runs_routine_started ON routine_runs(routine_id, started_at DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS event_log (
            id TEXT PRIMARY KEY,
            event_family TEXT NOT NULL,
            event_kind TEXT NOT NULL,
            state TEXT,
            thread_id TEXT,
            payload_json TEXT NOT NULL,
            risk_label TEXT NOT NULL,
            handled_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_event_log_family_kind_ts ON event_log(event_family, event_kind, handled_at_ms DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory_graph_clusters (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            summary_text TEXT,
            center_node_id TEXT,
            created_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_memory_graph_clusters_center ON memory_graph_clusters(center_node_id, created_at_ms DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory_cluster_members (
            cluster_id INTEGER NOT NULL,
            node_id TEXT NOT NULL,
            PRIMARY KEY (cluster_id, node_id)
        );
        CREATE INDEX IF NOT EXISTS idx_memory_cluster_members_node ON memory_cluster_members(node_id, cluster_id);",
    )?;
    ensure_column(
        connection,
        "skill_variants",
        "fitness_score",
        "REAL NOT NULL DEFAULT 0",
    )?;
    connection.execute(
        "UPDATE skill_variants SET fitness_score = CAST(success_count AS REAL) - CAST(failure_count AS REAL) WHERE fitness_score = 0",
        [],
    )?;
    ensure_column(connection, "agent_threads", "deleted_at", "INTEGER")?;
    ensure_column(connection, "agent_messages", "cost_usd", "REAL")?;
    ensure_column(connection, "agent_messages", "deleted_at", "INTEGER")?;
    connection.execute(
        "CREATE INDEX IF NOT EXISTS idx_messages_thread_deleted_created ON agent_messages(thread_id, deleted_at, created_at, id)",
        [],
    )?;
    ensure_column(connection, "agent_tasks", "scheduled_at", "INTEGER")?;
    ensure_column(connection, "agent_tasks", "goal_run_id", "TEXT")?;
    ensure_column(connection, "agent_tasks", "goal_run_title", "TEXT")?;
    ensure_column(connection, "agent_tasks", "goal_step_id", "TEXT")?;
    ensure_column(connection, "agent_tasks", "goal_step_title", "TEXT")?;
    ensure_column(connection, "agent_tasks", "parent_task_id", "TEXT")?;
    ensure_column(connection, "agent_tasks", "parent_thread_id", "TEXT")?;
    ensure_column(
        connection,
        "agent_tasks",
        "runtime",
        "TEXT NOT NULL DEFAULT 'daemon'",
    )?;
    ensure_column(connection, "agent_tasks", "override_provider", "TEXT")?;
    ensure_column(connection, "agent_tasks", "override_model", "TEXT")?;
    ensure_column(connection, "agent_tasks", "override_system_prompt", "TEXT")?;
    ensure_column(connection, "agent_tasks", "sub_agent_def_id", "TEXT")?;
    ensure_column(connection, "agent_tasks", "tool_whitelist_json", "TEXT")?;
    ensure_column(connection, "agent_tasks", "tool_blacklist_json", "TEXT")?;
    ensure_column(
        connection,
        "agent_tasks",
        "context_budget_tokens",
        "INTEGER",
    )?;
    ensure_column(connection, "agent_tasks", "context_overflow_action", "TEXT")?;
    ensure_column(connection, "agent_tasks", "termination_conditions", "TEXT")?;
    ensure_column(connection, "agent_tasks", "success_criteria", "TEXT")?;
    ensure_column(connection, "agent_tasks", "max_duration_secs", "INTEGER")?;
    ensure_column(connection, "agent_tasks", "supervisor_config_json", "TEXT")?;
    ensure_column(connection, "agent_tasks", "deleted_at", "INTEGER")?;
    ensure_column(
        connection,
        "agent_task_dependencies",
        "deleted_at",
        "INTEGER",
    )?;
    ensure_column(connection, "agent_task_logs", "deleted_at", "INTEGER")?;
    ensure_column(connection, "agent_config_items", "deleted_at", "INTEGER")?;
    ensure_column(connection, "provider_auth_state", "deleted_at", "INTEGER")?;
    ensure_column(connection, "plugins", "deleted_at", "INTEGER")?;
    ensure_column(connection, "plugin_settings", "deleted_at", "INTEGER")?;
    ensure_column(connection, "plugin_credentials", "deleted_at", "INTEGER")?;
    for table in [
        "command_log",
        "snapshot_index",
        "agent_checkpoints",
        "gateway_threads",
        "gateway_channel_modes",
        "whatsapp_provider_state",
        "operator_profile_sessions",
        "action_audit",
        "memory_tombstones",
        "consolidation_state",
        "offloaded_payloads",
        "thread_structural_memory",
        "routine_definitions",
        "memory_cluster_members",
        "cognitive_biases",
        "workflow_profiles",
        "protocol_steps",
    ] {
        ensure_column(connection, table, "deleted_at", "INTEGER")?;
    }
    ensure_column(
        connection,
        "memory_distillation_log",
        "source_message_span_json",
        "TEXT",
    )?;
    ensure_column(connection, "agent_tasks", "policy_fingerprint", "TEXT")?;
    ensure_column(connection, "agent_tasks", "approval_expires_at", "INTEGER")?;
    ensure_column(connection, "agent_tasks", "containment_scope", "TEXT")?;
    ensure_column(connection, "agent_tasks", "compensation_status", "TEXT")?;
    ensure_column(connection, "agent_tasks", "compensation_summary", "TEXT")?;
    ensure_column(connection, "goal_runs", "client_request_id", "TEXT")?;
    ensure_column(connection, "goal_runs", "failure_cause", "TEXT")?;
    ensure_column(connection, "goal_runs", "stopped_reason", "TEXT")?;
    ensure_column(
        connection,
        "goal_runs",
        "child_task_count",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        connection,
        "goal_runs",
        "approval_count",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(connection, "goal_runs", "awaiting_approval_id", "TEXT")?;
    ensure_column(connection, "goal_runs", "policy_fingerprint", "TEXT")?;
    ensure_column(connection, "goal_runs", "approval_expires_at", "INTEGER")?;
    ensure_column(connection, "goal_runs", "containment_scope", "TEXT")?;
    ensure_column(connection, "goal_runs", "compensation_status", "TEXT")?;
    ensure_column(connection, "goal_runs", "compensation_summary", "TEXT")?;
    ensure_column(connection, "goal_runs", "active_task_id", "TEXT")?;
    ensure_column(connection, "goal_runs", "duration_ms", "INTEGER")?;
    ensure_column(connection, "goal_runs", "dossier_json", "TEXT")?;
    ensure_column(
        connection,
        "goal_runs",
        "total_prompt_tokens",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        connection,
        "goal_runs",
        "total_completion_tokens",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(connection, "goal_runs", "estimated_cost_usd", "REAL")?;
    ensure_column(
        connection,
        "goal_runs",
        "model_usage_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    ensure_column(
        connection,
        "goal_runs",
        "autonomy_level",
        "TEXT NOT NULL DEFAULT 'aware'",
    )?;
    ensure_column(connection, "goal_runs", "authorship_tag", "TEXT")?;
    ensure_column(
        connection,
        "goal_runs",
        "planner_owner_profile_json",
        "TEXT",
    )?;
    ensure_column(
        connection,
        "goal_runs",
        "current_step_owner_profile_json",
        "TEXT",
    )?;
    ensure_column(
        connection,
        "goal_runs",
        "launch_assignment_snapshot_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    ensure_column(
        connection,
        "goal_runs",
        "runtime_assignment_list_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    ensure_column(connection, "goal_runs", "root_thread_id", "TEXT")?;
    ensure_column(connection, "goal_runs", "active_thread_id", "TEXT")?;
    ensure_column(
        connection,
        "goal_runs",
        "execution_thread_ids_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    ensure_column(connection, "goal_runs", "deleted_at", "INTEGER")?;
    ensure_column(connection, "goal_run_steps", "deleted_at", "INTEGER")?;
    ensure_column(connection, "goal_run_events", "step_index", "INTEGER")?;
    ensure_column(connection, "goal_run_events", "todo_snapshot_json", "TEXT")?;
    ensure_column(connection, "goal_run_events", "deleted_at", "INTEGER")?;
    // BEAT-09: user_action column for dismissal tracking in action_audit.
    ensure_column(connection, "action_audit", "user_action", "TEXT")?;
    ensure_column(
        connection,
        "causal_traces",
        "trace_family",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    connection.execute(
        "CREATE INDEX IF NOT EXISTS idx_causal_traces_family ON causal_traces(trace_family, created_at DESC)",
        [],
    )?;
    ensure_column(
        connection,
        "memory_provenance",
        "entry_hash",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(connection, "memory_provenance", "signature", "TEXT")?;
    ensure_column(connection, "memory_provenance", "signature_scheme", "TEXT")?;
    ensure_column(connection, "memory_provenance", "confirmed_at", "INTEGER")?;
    ensure_column(connection, "memory_provenance", "retracted_at", "INTEGER")?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory_provenance_relationships (
            id TEXT PRIMARY KEY,
            source_entry_id TEXT NOT NULL,
            target_entry_id TEXT NOT NULL,
            relation_type TEXT NOT NULL,
            fact_key TEXT,
            created_at INTEGER NOT NULL
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_provenance_rel_unique ON memory_provenance_relationships(source_entry_id, target_entry_id, relation_type, fact_key);
        CREATE INDEX IF NOT EXISTS idx_memory_provenance_rel_source ON memory_provenance_relationships(source_entry_id, created_at DESC);",
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS collaboration_agent_outcomes (
            parent_task_id TEXT NOT NULL,
            task_id TEXT NOT NULL,
            success_count INTEGER NOT NULL DEFAULT 0,
            failure_count INTEGER NOT NULL DEFAULT 0,
            learned_score REAL NOT NULL DEFAULT 0.5,
            last_outcome TEXT,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY (parent_task_id, task_id)
        );
        CREATE INDEX IF NOT EXISTS idx_collaboration_agent_outcomes_parent_updated ON collaboration_agent_outcomes(parent_task_id, updated_at_ms DESC);",
    )?;
    connection.execute(
            "CREATE INDEX IF NOT EXISTS idx_agent_tasks_goal_run ON agent_tasks(goal_run_id, created_at DESC)",
            [],
        )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspace_settings (
            workspace_id TEXT PRIMARY KEY,
            workspace_root TEXT,
            operator TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS workspace_tasks (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            title TEXT NOT NULL,
            task_type TEXT NOT NULL,
            description TEXT NOT NULL,
            definition_of_done TEXT,
            priority TEXT NOT NULL,
            status TEXT NOT NULL,
            sort_order INTEGER NOT NULL,
            reporter_json TEXT NOT NULL,
            assignee_json TEXT,
            reviewer_json TEXT,
            thread_id TEXT,
            goal_run_id TEXT,
            runtime_history_json TEXT NOT NULL DEFAULT '[]',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            started_at INTEGER,
            completed_at INTEGER,
            deleted_at INTEGER,
            last_notice_id TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_workspace_tasks_visible ON workspace_tasks(workspace_id, deleted_at, status, sort_order);
        CREATE TABLE IF NOT EXISTS workspace_notices (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            task_id TEXT NOT NULL,
            notice_type TEXT NOT NULL,
            message TEXT NOT NULL,
            actor_json TEXT,
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_workspace_notices_task ON workspace_notices(workspace_id, task_id, created_at DESC);",
    )?;
    ensure_column(
        connection,
        "workspace_tasks",
        "runtime_history_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    // Episodic memory schema (Phase v3.0).
    crate::agent::episodic::schema::init_episodic_schema(connection)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))?;
    // Handoff broker schema (Phase v3.0: HAND-09).
    crate::agent::handoff::schema::init_handoff_schema(connection)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))?;
    Ok(())
}
