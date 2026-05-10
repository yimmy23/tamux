use super::super::from_tasks_to_is_weles_thread::*;
use super::super::is_svarog_agent_name_to_hit_test::*;
use super::format_time_ago_zero_returns_empty_to_filtered_threads_swarog_tab::*;
use crate::state::chat::AgentThread;
use crate::state::modal::{ModalState, ThreadPickerTab};
use crate::state::ModalAction;
use zorai_protocol::AGENT_NAME_RAROG;

#[test]
fn filtered_threads_swarog_tab_includes_unattributed_threads() {
    let chat = make_chat(vec![
        AgentThread {
            id: "thread-svarog".into(),
            agent_name: Some("Svarog".into()),
            title: "Root planning".into(),
            ..Default::default()
        },
        AgentThread {
            id: "thread-subagent-unattributed".into(),
            agent_name: None,
            title: "Execute queued subagent task".into(),
            ..Default::default()
        },
    ]);
    let modal = ModalState::new();

    let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

    assert_eq!(threads.len(), 2);
    assert_eq!(threads[0].id, "thread-svarog");
    assert_eq!(threads[1].id, "thread-subagent-unattributed");
}

#[test]
fn rarog_tab_filters_threads_and_searches_within_tab() {
    let chat = make_chat(vec![
        AgentThread {
            id: "regular-thread".into(),
            agent_name: Some("Svarog".into()),
            title: "Regular work".into(),
            ..Default::default()
        },
        AgentThread {
            id: "concierge".into(),
            title: "Concierge".into(),
            ..Default::default()
        },
        AgentThread {
            id: "heartbeat-1".into(),
            title: "HEARTBEAT SYNTHESIS".into(),
            ..Default::default()
        },
    ]);
    let mut modal = ModalState::new();
    modal.set_thread_picker_tab(ThreadPickerTab::Rarog);
    modal.reduce(ModalAction::SetQuery("heart".into()));

    let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].id, "heartbeat-1");
}

#[test]
fn playgrounds_tab_filters_only_playground_threads() {
    let chat = make_chat(vec![
        AgentThread {
            id: "regular-thread".into(),
            title: "Regular work".into(),
            ..Default::default()
        },
        AgentThread {
            id: "playground:domowoj:thread-user".into(),
            title: "Participant Playground · Domowoj @ thread-user".into(),
            ..Default::default()
        },
    ]);
    let mut modal = ModalState::new();
    modal.set_thread_picker_tab(ThreadPickerTab::Playgrounds);

    let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].id, "playground:domowoj:thread-user");
}

#[test]
fn goals_tab_filters_goal_threads() {
    let chat = make_chat(vec![
        AgentThread {
            id: "regular-thread".into(),
            title: "Regular work".into(),
            ..Default::default()
        },
        AgentThread {
            id: "goal:goal_1".into(),
            agent_name: Some("Domowoj".into()),
            title: "Run concrete moat pass".into(),
            ..Default::default()
        },
    ]);
    let mut modal = ModalState::new();
    modal.set_thread_picker_tab(ThreadPickerTab::Goals);

    let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].id, "goal:goal_1");
}

#[test]
fn goal_run_thread_ids_route_plain_threads_to_goals_tab() {
    let chat = make_chat(vec![AgentThread {
        id: "thread-existing-goal".into(),
        agent_name: Some("Domowoj".into()),
        title: "Run concrete moat pass".into(),
        ..Default::default()
    }]);
    let subagents = make_subagents(Vec::new());
    let tasks = make_tasks_with_goal_thread("thread-existing-goal");
    let mut modal = ModalState::new();

    let labels = tab_specs_for_tasks(&chat, &subagents, &tasks)
        .into_iter()
        .map(|spec| spec.label)
        .collect::<Vec<_>>();
    assert!(
        !labels.iter().any(|label| label == "[Domowoj]"),
        "goal-linked threads should not create normal agent tabs"
    );

    assert!(
        filtered_threads_for_tasks(&chat, &modal, &subagents, &tasks).is_empty(),
        "goal-linked threads should be excluded from the default Swarog tab"
    );

    modal.set_thread_picker_tab(ThreadPickerTab::Agent("domowoj".into()));
    assert!(
        filtered_threads_for_tasks(&chat, &modal, &subagents, &tasks).is_empty(),
        "goal-linked threads should be excluded from normal agent tabs"
    );

    modal.set_thread_picker_tab(ThreadPickerTab::Goals);
    let threads = filtered_threads_for_tasks(&chat, &modal, &subagents, &tasks);
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].id, "thread-existing-goal");
    assert_eq!(
        thread_display_title_for_tasks(threads[0], &tasks),
        "goal: Domowoj · Run concrete moat pass"
    );
}

#[test]
fn goal_thread_display_title_shows_prefix_role_and_title() {
    let thread = AgentThread {
        id: "goal:goal_1".into(),
        agent_name: Some("Domowoj".into()),
        title: "Run concrete moat pass".into(),
        ..Default::default()
    };

    assert_eq!(
        thread_display_title(&thread),
        "goal: Domowoj · Run concrete moat pass"
    );
}

#[test]
fn search_matches_thread_responder_name() {
    let chat = make_chat(vec![AgentThread {
        id: "regular-thread".into(),
        agent_name: Some("Svarog".into()),
        title: "Needs review".into(),
        ..Default::default()
    }]);
    let mut modal = ModalState::new();
    modal.reduce(ModalAction::SetQuery("svarog".into()));

    let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].id, "regular-thread");
}

#[test]
fn weles_tab_filters_weles_threads_without_internal_dms() {
    let chat = make_chat(vec![
        AgentThread {
            id: "regular-thread".into(),
            agent_name: Some("Svarog".into()),
            title: "Regular work".into(),
            ..Default::default()
        },
        AgentThread {
            id: "weles-thread".into(),
            title: "WELES governance review".into(),
            ..Default::default()
        },
        AgentThread {
            id: "dm:svarog:weles".into(),
            title: "Internal DM · Svarog ↔ Weles".into(),
            ..Default::default()
        },
    ]);
    let mut modal = ModalState::new();
    modal.set_thread_picker_tab(ThreadPickerTab::Weles);

    let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].id, "weles-thread");
}

#[test]
fn weles_tab_uses_agent_name_for_new_targeted_threads() {
    let chat = make_chat(vec![
        AgentThread {
            id: "thread-weles".into(),
            agent_name: Some("Weles".into()),
            title: "Review pending changes".into(),
            ..Default::default()
        },
        AgentThread {
            id: "thread-svarog".into(),
            agent_name: Some("Svarog".into()),
            title: "Review pending changes".into(),
            ..Default::default()
        },
    ]);
    let mut modal = ModalState::new();
    modal.set_thread_picker_tab(ThreadPickerTab::Weles);

    let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].id, "thread-weles");
}

#[test]
fn rarog_tab_uses_agent_name_for_new_targeted_threads() {
    let chat = make_chat(vec![
        AgentThread {
            id: "thread-rarog".into(),
            agent_name: Some("Rarog".into()),
            title: "Operator triage".into(),
            ..Default::default()
        },
        AgentThread {
            id: "thread-svarog".into(),
            agent_name: Some("Svarog".into()),
            title: "Operator triage".into(),
            ..Default::default()
        },
    ]);
    let mut modal = ModalState::new();
    modal.set_thread_picker_tab(ThreadPickerTab::Rarog);

    let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].id, "thread-rarog");
}

#[test]
fn internal_tab_filters_internal_dm_threads() {
    let chat = make_chat(vec![
        AgentThread {
            id: "regular-thread".into(),
            title: "Regular work".into(),
            ..Default::default()
        },
        AgentThread {
            id: "dm:svarog:weles".into(),
            title: "Internal DM · Svarog ↔ Weles".into(),
            ..Default::default()
        },
    ]);
    let mut modal = ModalState::new();
    modal.set_thread_picker_tab(ThreadPickerTab::Internal);

    let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].id, "dm:svarog:weles");
}

#[test]
fn gateway_tab_filters_gateway_threads() {
    let chat = make_chat(vec![
        AgentThread {
            id: "regular-thread".into(),
            title: "Regular work".into(),
            ..Default::default()
        },
        AgentThread {
            id: "thread-slack-alice".into(),
            title: "slack Alice".into(),
            ..Default::default()
        },
        AgentThread {
            id: "dm:svarog:weles".into(),
            title: "Internal DM · Svarog ↔ Weles".into(),
            ..Default::default()
        },
    ]);
    let mut modal = ModalState::new();
    modal.set_thread_picker_tab(ThreadPickerTab::Gateway);

    let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].id, "thread-slack-alice");
}

#[test]
fn filtered_threads_exclude_hidden_handoff_threads() {
    let chat = make_chat(vec![
        AgentThread {
            id: "regular-thread".into(),
            agent_name: Some("Svarog".into()),
            title: "Regular work".into(),
            ..Default::default()
        },
        AgentThread {
            id: "handoff:regular-thread:handoff-1".into(),
            title: "Handoff · Svarog -> Weles".into(),
            ..Default::default()
        },
    ]);
    let modal = ModalState::new();

    let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].id, "regular-thread");
}

#[test]
fn dynamic_agent_tab_filters_matching_threads() {
    let chat = make_chat(vec![
        AgentThread {
            id: "thread-domowoj".into(),
            agent_name: Some("Domowoj".into()),
            title: "Workspace cleanup".into(),
            ..Default::default()
        },
        AgentThread {
            id: "thread-svarog".into(),
            agent_name: Some("Svarog".into()),
            title: "Workspace cleanup".into(),
            ..Default::default()
        },
    ]);
    let subagents = make_subagents(vec![sample_subagent("domowoj", "Domowoj", false)]);
    let mut modal = ModalState::new();
    modal.set_thread_picker_tab(ThreadPickerTab::Agent("domowoj".to_string()));

    let threads = filtered_threads(&chat, &modal, &subagents);

    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].id, "thread-domowoj");
}

#[test]
fn thread_display_title_renames_concierge_to_rarog() {
    let thread = AgentThread {
        id: "concierge".into(),
        title: "Concierge".into(),
        ..Default::default()
    };

    assert_eq!(thread_display_title(&thread), AGENT_NAME_RAROG);
}
