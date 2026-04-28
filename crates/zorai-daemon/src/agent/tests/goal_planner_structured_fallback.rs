use super::*;
use crate::session_manager::SessionManager;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn sample_goal_run_for_structured_fallback(goal_run_id: &str) -> GoalRun {
    GoalRun {
        id: goal_run_id.to_string(),
        title: "Fallback goal".to_string(),
        goal: "Recover when structured output schema is rejected".to_string(),
        client_request_id: None,
        status: GoalRunStatus::Queued,
        priority: TaskPriority::Normal,
        created_at: now_millis(),
        updated_at: now_millis(),
        started_at: None,
        completed_at: None,
        thread_id: None,
        session_id: None,
        current_step_index: 0,
        current_step_title: None,
        current_step_kind: None,
        planner_owner_profile: None,
        current_step_owner_profile: None,
        replan_count: 0,
        max_replans: 1,
        plan_summary: None,
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
        stopped_reason: None,
        child_task_ids: Vec::new(),
        child_task_count: 0,
        approval_count: 0,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        active_task_id: None,
        duration_ms: None,
        steps: Vec::new(),
        events: Vec::new(),
        dossier: None,
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        model_usage: Vec::new(),
        autonomy_level: AutonomyLevel::Supervised,
        authorship_tag: None,
        launch_assignment_snapshot: Vec::new(),
        runtime_assignment_list: Vec::new(),
        root_thread_id: None,
        active_thread_id: None,
        execution_thread_ids: Vec::new(),
    }
}

async fn spawn_schema_rejection_then_success_server(
    recorded_bodies: Arc<StdMutex<VecDeque<String>>>,
    assistant_content: String,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind schema fallback server");
    let addr = listener
        .local_addr()
        .expect("schema fallback server local addr");
    let request_count = Arc::new(AtomicUsize::new(0));
    let success_json =
        serde_json::to_string(&assistant_content).expect("assistant content should serialize");

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let recorded_bodies = recorded_bodies.clone();
            let request_count = request_count.clone();
            let success_json = success_json.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 65536];
                let read = socket
                    .read(&mut buffer)
                    .await
                    .expect("read schema fallback request");
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                let body = request
                    .split("\r\n\r\n")
                    .nth(1)
                    .unwrap_or_default()
                    .to_string();
                recorded_bodies
                    .lock()
                    .expect("lock schema fallback request log")
                    .push_back(body);

                let response = if request_count.fetch_add(1, Ordering::SeqCst) == 0 {
                    let error_body = serde_json::json!({
                        "error": {
                            "message": "Invalid schema for response_format 'structured_output': In context=('properties', 'steps', 'items', 'properties', 'proof_checks', 'items'), 'required' is required to be supplied and to be an array including every key in properties. Missing 'id'.",
                            "type": "invalid_request_error",
                            "param": "text.format.schema",
                            "code": "invalid_json_schema"
                        }
                    })
                    .to_string();
                    format!(
                        "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        error_body.len(),
                        error_body
                    )
                } else {
                    format!(
                        concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {{\"choices\":[{{\"delta\":{{\"content\":{}}}}}]}}\n\n",
                            "data: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":7,\"completion_tokens\":3}}}}\n\n",
                            "data: [DONE]\n\n"
                        ),
                        success_json
                    )
                };

                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write schema fallback response");
            });
        }
    });

    format!("http://{addr}/v1")
}

#[tokio::test]
async fn request_goal_plan_falls_back_when_provider_rejects_structured_schema() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = spawn_schema_rejection_then_success_server(
        recorded_bodies.clone(),
        serde_json::json!({
            "title": "Recovered plan",
            "summary": "Fallback to plain JSON request after schema rejection.",
            "steps": [
                {
                    "title": "Retry without provider-enforced schema",
                    "instructions": "Ask for JSON without response_format so the plan can still be parsed.",
                    "kind": "command",
                    "success_criteria": "plan JSON is returned",
                    "session_id": null,
                    "llm_confidence": "likely",
                    "llm_confidence_rationale": "same prompt, less brittle transport"
                }
            ],
            "rejected_alternatives": []
        })
        .to_string(),
    )
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let plan = engine
        .request_goal_plan(&sample_goal_run_for_structured_fallback(
            "goal-schema-fallback",
        ))
        .await
        .expect("goal plan should fall back after schema rejection");

    assert_eq!(
        plan.summary,
        "Fallback to plain JSON request after schema rejection."
    );
    assert_eq!(plan.steps.len(), 1);
    assert!(plan.steps[0]
        .title
        .contains("Retry without provider-enforced schema"));

    let recorded = recorded_bodies
        .lock()
        .expect("lock schema fallback request log");
    assert_eq!(
        recorded.len(),
        2,
        "expected structured request plus raw fallback"
    );
    assert!(
        recorded[0].contains("\"response_format\""),
        "first request should attempt structured output: {}",
        recorded[0]
    );
    assert!(
        !recorded[1].contains("\"response_format\""),
        "fallback request should not force response_format: {}",
        recorded[1]
    );
}
