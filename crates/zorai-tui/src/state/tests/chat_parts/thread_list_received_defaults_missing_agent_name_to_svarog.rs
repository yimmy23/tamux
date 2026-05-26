use super::super::*;
use zorai_protocol::AGENT_NAME_SWAROG;

#[test]
fn thread_list_received_defaults_missing_agent_name_to_svarog() {
    let mut state = ChatState::new();

    state.reduce(ChatAction::ThreadListReceived(vec![AgentThread {
        id: "thread-unowned".to_string(),
        agent_name: None,
        title: "Recovered thread".to_string(),
        ..Default::default()
    }]));

    let thread = state
        .threads()
        .iter()
        .find(|thread| thread.id == "thread-unowned")
        .expect("thread should be present");
    assert_eq!(thread.agent_name.as_deref(), Some(AGENT_NAME_SWAROG));
}
