#[test]
fn manual_compaction_workflow_notice_forwards_only_to_subscribed_target_thread() {
    let event = AgentEvent::WorkflowNotice {
        thread_id: "thread-compaction".to_string(),
        kind: "manual-compaction".to_string(),
        message: "Manual compaction applied.".to_string(),
        details: Some("{\"split_at\":5,\"total_message_count\":12}".to_string()),
    };

    let subscribed = std::collections::HashSet::from(["thread-compaction".to_string()]);
    let other_thread = std::collections::HashSet::from(["thread-other".to_string()]);
    let no_threads = std::collections::HashSet::new();

    assert!(
        super::should_forward_agent_event(&event, &subscribed),
        "compaction notice should forward to the subscribed target thread"
    );
    assert!(
        !super::should_forward_agent_event(&event, &other_thread),
        "compaction notice should not forward to a different subscribed thread"
    );
    assert!(
        !super::should_forward_agent_event(&event, &no_threads),
        "compaction notice should not behave like a global event"
    );
}

#[test]
fn thread_reload_required_for_compaction_forwards_only_to_subscribed_target_thread() {
    let event = AgentEvent::ThreadReloadRequired {
        thread_id: "thread-compaction".to_string(),
    };

    let subscribed = std::collections::HashSet::from(["thread-compaction".to_string()]);
    let other_thread = std::collections::HashSet::from(["thread-other".to_string()]);

    assert!(
        super::should_forward_agent_event(&event, &subscribed),
        "compaction reload should forward to the subscribed target thread"
    );
    assert!(
        !super::should_forward_agent_event(&event, &other_thread),
        "compaction reload should not forward to a different subscribed thread"
    );
}
