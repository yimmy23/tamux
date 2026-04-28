use super::*;
use chrono::{Datelike, Duration, Local, TimeZone};
use rusqlite::params;

#[derive(Debug)]
struct StatisticsTotalsRow {
    input_tokens: i64,
    output_tokens: i64,
    total_tokens: i64,
    cost_usd: f64,
    provider_count: i64,
    model_count: i64,
    missing_cost_rows: i64,
}

impl HistoryStore {
    pub async fn get_agent_statistics(
        &self,
        window: AgentStatisticsWindow,
    ) -> Result<AgentStatisticsSnapshot> {
        let cutoff_ms = window_cutoff_ms(window);
        let totals_row = self
            .read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT
                        COALESCE(SUM(COALESCE(input_tokens, 0)), 0) AS input_tokens,
                        COALESCE(SUM(COALESCE(output_tokens, 0)), 0) AS output_tokens,
                        COALESCE(SUM(COALESCE(total_tokens, COALESCE(input_tokens, 0) + COALESCE(output_tokens, 0))), 0) AS total_tokens,
                        COALESCE(SUM(COALESCE(cost_usd, 0)), 0.0) AS cost_usd,
                        COUNT(DISTINCT CASE WHEN provider IS NOT NULL AND TRIM(provider) <> '' THEN provider END) AS provider_count,
                        COUNT(DISTINCT CASE WHEN model IS NOT NULL AND TRIM(model) <> '' THEN model END) AS model_count,
                        COALESCE(SUM(CASE WHEN cost_usd IS NULL THEN 1 ELSE 0 END), 0) AS missing_cost_rows
                     FROM agent_messages
                     WHERE role = 'assistant'
                       AND (?1 IS NULL OR created_at >= ?1)",
                )?;
                let row = stmt.query_row(params![cutoff_ms], |row| {
                    Ok(StatisticsTotalsRow {
                        input_tokens: row.get(0)?,
                        output_tokens: row.get(1)?,
                        total_tokens: row.get(2)?,
                        cost_usd: row.get(3)?,
                        provider_count: row.get(4)?,
                        model_count: row.get(5)?,
                        missing_cost_rows: row.get(6)?,
                    })
                })?;
                Ok(row)
            })
            .await?;

        let provider_cutoff_ms = cutoff_ms;
        let mut providers = self
            .read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT
                        CASE WHEN provider IS NULL OR TRIM(provider) = '' THEN 'unknown' ELSE provider END AS provider_key,
                        COALESCE(SUM(COALESCE(input_tokens, 0)), 0) AS input_tokens,
                        COALESCE(SUM(COALESCE(output_tokens, 0)), 0) AS output_tokens,
                        COALESCE(SUM(COALESCE(total_tokens, COALESCE(input_tokens, 0) + COALESCE(output_tokens, 0))), 0) AS total_tokens,
                        COALESCE(SUM(COALESCE(cost_usd, 0)), 0.0) AS cost_usd
                     FROM agent_messages
                     WHERE role = 'assistant'
                       AND (?1 IS NULL OR created_at >= ?1)
                     GROUP BY provider_key",
                )?;
                let rows = stmt.query_map(params![provider_cutoff_ms], |row| {
                    Ok(ProviderStatisticsRow {
                        provider: row.get(0)?,
                        input_tokens: row.get::<_, i64>(1)?.max(0) as u64,
                        output_tokens: row.get::<_, i64>(2)?.max(0) as u64,
                        total_tokens: row.get::<_, i64>(3)?.max(0) as u64,
                        cost_usd: row.get(4)?,
                    })
                })?;
                Ok(rows.filter_map(|row| row.ok()).collect::<Vec<_>>())
            })
            .await?;

        providers.sort_by(|left, right| {
            right
                .total_tokens
                .cmp(&left.total_tokens)
                .then_with(|| right.cost_usd.total_cmp(&left.cost_usd))
                .then_with(|| left.provider.cmp(&right.provider))
        });

        let model_cutoff_ms = cutoff_ms;
        let models = self
            .read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT
                        CASE WHEN provider IS NULL OR TRIM(provider) = '' THEN 'unknown' ELSE provider END AS provider_key,
                        CASE WHEN model IS NULL OR TRIM(model) = '' THEN 'unknown' ELSE model END AS model_key,
                        COALESCE(SUM(COALESCE(input_tokens, 0)), 0) AS input_tokens,
                        COALESCE(SUM(COALESCE(output_tokens, 0)), 0) AS output_tokens,
                        COALESCE(SUM(COALESCE(total_tokens, COALESCE(input_tokens, 0) + COALESCE(output_tokens, 0))), 0) AS total_tokens,
                        COALESCE(SUM(COALESCE(cost_usd, 0)), 0.0) AS cost_usd
                     FROM agent_messages
                     WHERE role = 'assistant'
                       AND (?1 IS NULL OR created_at >= ?1)
                       AND NOT (
                           (provider IS NULL OR TRIM(provider) = '')
                           AND (model IS NULL OR TRIM(model) = '')
                       )
                     GROUP BY provider_key, model_key",
                )?;
                let rows = stmt.query_map(params![model_cutoff_ms], |row| {
                    Ok(ModelStatisticsRow {
                        provider: row.get(0)?,
                        model: row.get(1)?,
                        input_tokens: row.get::<_, i64>(2)?.max(0) as u64,
                        output_tokens: row.get::<_, i64>(3)?.max(0) as u64,
                        total_tokens: row.get::<_, i64>(4)?.max(0) as u64,
                        cost_usd: row.get(5)?,
                    })
                })?;
                Ok(rows.filter_map(|row| row.ok()).collect::<Vec<_>>())
            })
            .await?;

        let mut sorted_models = models.clone();
        sorted_models.sort_by(|left, right| {
            right
                .total_tokens
                .cmp(&left.total_tokens)
                .then_with(|| right.cost_usd.total_cmp(&left.cost_usd))
                .then_with(|| left.provider.cmp(&right.provider))
                .then_with(|| left.model.cmp(&right.model))
        });

        let mut top_models_by_cost = models.clone();
        top_models_by_cost.sort_by(|left, right| {
            right
                .cost_usd
                .total_cmp(&left.cost_usd)
                .then_with(|| right.total_tokens.cmp(&left.total_tokens))
                .then_with(|| left.provider.cmp(&right.provider))
                .then_with(|| left.model.cmp(&right.model))
        });
        top_models_by_cost.truncate(5);

        let mut top_models_by_tokens = sorted_models.clone();
        top_models_by_tokens.truncate(5);

        Ok(AgentStatisticsSnapshot {
            window,
            generated_at: current_time_ms(),
            has_incomplete_cost_history: totals_row.missing_cost_rows > 0,
            totals: AgentStatisticsTotals {
                input_tokens: totals_row.input_tokens.max(0) as u64,
                output_tokens: totals_row.output_tokens.max(0) as u64,
                total_tokens: totals_row.total_tokens.max(0) as u64,
                cost_usd: totals_row.cost_usd,
                provider_count: totals_row.provider_count.max(0) as u64,
                model_count: totals_row.model_count.max(0) as u64,
            },
            providers,
            models: sorted_models,
            top_models_by_tokens,
            top_models_by_cost,
        })
    }
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn window_cutoff_ms(window: AgentStatisticsWindow) -> Option<i64> {
    match window {
        AgentStatisticsWindow::Today => {
            let now = Local::now();
            let start_of_day = Local
                .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
                .single()
                .unwrap_or(now);
            Some(start_of_day.timestamp_millis())
        }
        AgentStatisticsWindow::Last7Days => {
            Some((Local::now() - Duration::days(7)).timestamp_millis())
        }
        AgentStatisticsWindow::Last30Days => {
            Some((Local::now() - Duration::days(30)).timestamp_millis())
        }
        AgentStatisticsWindow::All => None,
    }
}
