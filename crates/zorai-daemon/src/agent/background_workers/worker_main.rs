use super::protocol::{BackgroundWorkerCommand, BackgroundWorkerKind, BackgroundWorkerResult};
use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub(crate) async fn run_background_worker_from_stdio(kind: BackgroundWorkerKind) -> Result<()> {
    let mut request_bytes = Vec::new();
    tokio::io::stdin()
        .read_to_end(&mut request_bytes)
        .await
        .context("read background worker request")?;
    let request = serde_json::from_slice::<BackgroundWorkerCommand>(&request_bytes)
        .context("parse background worker request")?;
    let response = handle_background_worker_command(kind, request);
    let response_bytes =
        serde_json::to_vec(&response).context("serialize background worker response")?;
    tokio::io::stdout()
        .write_all(&response_bytes)
        .await
        .context("write background worker response")?;
    Ok(())
}

pub(crate) fn handle_background_worker_command(
    kind: BackgroundWorkerKind,
    command: BackgroundWorkerCommand,
) -> BackgroundWorkerResult {
    match command {
        BackgroundWorkerCommand::Ping => BackgroundWorkerResult::Pong { kind },
        BackgroundWorkerCommand::TickSafety {
            observations,
            candidates,
            now_ms,
        } => {
            if kind != BackgroundWorkerKind::Safety {
                return BackgroundWorkerResult::Error {
                    message: format!("tick_safety is not handled by {kind:?}"),
                };
            }
            BackgroundWorkerResult::SafetyTick {
                decisions: super::domain_safety::evaluate_tick(observations, candidates, now_ms),
            }
        }
        BackgroundWorkerCommand::TickRouting {
            profiles,
            required_tags,
            score_rows,
            morphogenesis,
            routing,
            now_ms,
        } => {
            if kind != BackgroundWorkerKind::Routing {
                return BackgroundWorkerResult::Error {
                    message: format!("tick_routing is not handled by {kind:?}"),
                };
            }
            BackgroundWorkerResult::RoutingTick {
                snapshot: super::domain_routing::build_routing_snapshot(
                    &profiles,
                    &required_tags,
                    &score_rows,
                    &morphogenesis,
                    &routing,
                    now_ms,
                ),
            }
        }
        BackgroundWorkerCommand::TickMemory {
            thread_id,
            task_id,
            structural_memory,
            semantic_packages,
            now_ms,
        } => {
            if kind != BackgroundWorkerKind::Memory {
                return BackgroundWorkerResult::Error {
                    message: format!("tick_memory is not handled by {kind:?}"),
                };
            }
            BackgroundWorkerResult::MemoryTick {
                snapshot: super::domain_memory::build_memory_snapshot(
                    thread_id.as_deref(),
                    task_id.as_deref(),
                    structural_memory.as_ref(),
                    &semantic_packages,
                    now_ms,
                ),
            }
        }
        BackgroundWorkerCommand::TickLearning {
            successful_traces,
            variants,
            now_ms,
        } => {
            if kind != BackgroundWorkerKind::Learning {
                return BackgroundWorkerResult::Error {
                    message: format!("tick_learning is not handled by {kind:?}"),
                };
            }
            BackgroundWorkerResult::LearningTick {
                snapshot: super::domain_learning::build_learning_snapshot(
                    &successful_traces,
                    &variants,
                    now_ms,
                ),
            }
        }
    }
}
