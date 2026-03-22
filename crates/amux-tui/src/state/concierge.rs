#![allow(dead_code)]

/// TUI-side concierge state.
pub struct ConciergeState {
    pub enabled: bool,
    pub detail_level: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub auto_cleanup_on_navigate: bool,
    pub loading: bool,
    pub welcome_content: Option<String>,
    pub welcome_actions: Vec<ConciergeActionVm>,
    pub welcome_visible: bool,
    pub selected_action: usize,
}

#[derive(Debug, Clone)]
pub struct ConciergeActionVm {
    pub label: String,
    pub action_type: String,
    pub thread_id: Option<String>,
}

impl ConciergeState {
    pub fn new() -> Self {
        Self {
            enabled: true,
            detail_level: "proactive_triage".into(),
            provider: None,
            model: None,
            auto_cleanup_on_navigate: true,
            loading: false,
            welcome_content: None,
            welcome_actions: Vec::new(),
            welcome_visible: false,
            selected_action: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ConciergeAction {
    ConfigReceived {
        enabled: bool,
        detail_level: String,
        provider: Option<String>,
        model: Option<String>,
        auto_cleanup_on_navigate: bool,
    },
    WelcomeLoading(bool),
    WelcomeReceived {
        content: String,
        actions: Vec<ConciergeActionVm>,
    },
    SelectAction(usize),
    NavigateAction(i32),
    WelcomeDismissed,
}

impl ConciergeState {
    pub fn reduce(&mut self, action: ConciergeAction) {
        match action {
            ConciergeAction::ConfigReceived {
                enabled,
                detail_level,
                provider,
                model,
                auto_cleanup_on_navigate,
            } => {
                self.enabled = enabled;
                self.detail_level = detail_level;
                self.provider = provider;
                self.model = model;
                self.auto_cleanup_on_navigate = auto_cleanup_on_navigate;
                if !enabled {
                    self.loading = false;
                }
            }
            ConciergeAction::WelcomeLoading(loading) => {
                self.loading = loading && self.enabled;
            }
            ConciergeAction::WelcomeReceived { content, actions } => {
                self.loading = false;
                self.welcome_content = Some(content);
                self.welcome_actions = actions;
                self.welcome_visible = true;
                self.selected_action = 0;
            }
            ConciergeAction::SelectAction(index) => {
                if index < self.welcome_actions.len() {
                    self.selected_action = index;
                }
            }
            ConciergeAction::NavigateAction(delta) => {
                if self.welcome_actions.is_empty() {
                    self.selected_action = 0;
                } else if delta > 0 {
                    self.selected_action =
                        (self.selected_action + delta as usize).min(self.welcome_actions.len() - 1);
                } else {
                    self.selected_action = self.selected_action.saturating_sub((-delta) as usize);
                }
            }
            ConciergeAction::WelcomeDismissed => {
                self.loading = false;
                self.welcome_content = None;
                self.welcome_actions.clear();
                self.welcome_visible = false;
                self.selected_action = 0;
            }
        }
    }
}
