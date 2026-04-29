use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::text::{Line, Span};
use zorai_protocol::{
    WorkspaceActor, WorkspacePriority, WorkspaceTaskCreate, WorkspaceTaskType, AGENT_ID_SWAROG,
};

use crate::state::{modal, DaemonCommand};
use crate::theme::ThemeTokens;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkspaceCreateTaskField {
    Title,
    TaskType,
    Description,
    DefinitionOfDone,
    Priority,
    Assignee,
    Reviewer,
    Submit,
    Cancel,
}

impl WorkspaceCreateTaskField {
    const ALL: [Self; 9] = [
        Self::Title,
        Self::TaskType,
        Self::Description,
        Self::DefinitionOfDone,
        Self::Priority,
        Self::Assignee,
        Self::Reviewer,
        Self::Submit,
        Self::Cancel,
    ];

    pub(super) fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|field| *field == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    pub(super) fn previous(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|field| *field == self)
            .unwrap_or(0);
        Self::ALL[if index == 0 {
            Self::ALL.len() - 1
        } else {
            index - 1
        }]
    }

    fn label(self) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::TaskType => "Task type",
            Self::Description => "Description",
            Self::DefinitionOfDone => "Definition of done",
            Self::Priority => "Priority",
            Self::Assignee => "Assignee",
            Self::Reviewer => "Reviewer",
            Self::Submit => "Create",
            Self::Cancel => "Cancel",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RequiredFieldState {
    NotRequired,
    Missing,
    Valid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WorkspaceCreateTaskForm {
    pub(super) task_type: WorkspaceTaskType,
    pub(super) title: String,
    pub(super) description: String,
    pub(super) definition_of_done: String,
    pub(super) priority: WorkspacePriority,
    pub(super) assignee: Option<WorkspaceActor>,
    pub(super) reviewer: Option<WorkspaceActor>,
    pub(super) field: WorkspaceCreateTaskField,
}

impl WorkspaceCreateTaskForm {
    pub(super) fn new(task_type: WorkspaceTaskType) -> Self {
        Self {
            task_type,
            title: String::new(),
            description: String::new(),
            definition_of_done: String::new(),
            priority: WorkspacePriority::Low,
            assignee: Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string())),
            reviewer: Some(WorkspaceActor::User),
            field: WorkspaceCreateTaskField::Title,
        }
    }

    pub(super) fn to_request(&self, workspace_id: &str) -> Result<WorkspaceTaskCreate, String> {
        let title = self.title.trim();
        if title.is_empty() {
            return Err("Title is required".to_string());
        }
        let description = self.description.trim();
        if description.is_empty() {
            return Err("Description is required".to_string());
        }
        if self.reviewer.is_none() {
            return Err("Reviewer is required".to_string());
        }
        Ok(WorkspaceTaskCreate {
            workspace_id: workspace_id.to_string(),
            title: title.to_string(),
            task_type: self.task_type.clone(),
            description: description.to_string(),
            definition_of_done: self
                .definition_of_done
                .trim()
                .is_empty()
                .then_some(None)
                .unwrap_or_else(|| Some(self.definition_of_done.trim().to_string())),
            priority: Some(self.priority.clone()),
            assignee: self.assignee.clone(),
            reviewer: self.reviewer.clone(),
        })
    }

    pub(super) fn required_field_state(
        &self,
        field: WorkspaceCreateTaskField,
    ) -> RequiredFieldState {
        match field {
            WorkspaceCreateTaskField::Title => required_text_state(self.title.trim()),
            WorkspaceCreateTaskField::Description => required_text_state(self.description.trim()),
            WorkspaceCreateTaskField::Reviewer => {
                if self.reviewer.is_some() {
                    RequiredFieldState::Valid
                } else {
                    RequiredFieldState::Missing
                }
            }
            _ => RequiredFieldState::NotRequired,
        }
    }

    pub(super) fn next_field(&mut self) {
        self.field = self.field.next();
    }

    pub(super) fn previous_field(&mut self) {
        self.field = self.field.previous();
    }

    pub(super) fn insert_char(&mut self, ch: char) {
        if let Some(value) = self.active_text_mut() {
            value.push(ch);
        }
    }

    pub(super) fn backspace(&mut self) {
        if let Some(value) = self.active_text_mut() {
            value.pop();
        }
    }

    pub(super) fn activate_current_field(&mut self, subagents: &crate::state::SubAgentsState) {
        match self.field {
            WorkspaceCreateTaskField::TaskType => self.cycle_task_type(),
            WorkspaceCreateTaskField::Priority => self.cycle_priority(),
            WorkspaceCreateTaskField::Assignee => {
                let options = crate::app::workspace_actor_picker::workspace_actor_picker_options(
                    crate::app::workspace_actor_picker::WorkspaceActorPickerMode::Assignee,
                    subagents,
                );
                self.assignee = next_actor_selection(&options, &self.assignee);
            }
            WorkspaceCreateTaskField::Reviewer => {
                let options = crate::app::workspace_actor_picker::workspace_actor_picker_options(
                    crate::app::workspace_actor_picker::WorkspaceActorPickerMode::Reviewer,
                    subagents,
                );
                self.reviewer = next_actor_selection(&options, &self.reviewer);
            }
            _ => self.next_field(),
        }
    }

    fn active_text_mut(&mut self) -> Option<&mut String> {
        match self.field {
            WorkspaceCreateTaskField::Title => Some(&mut self.title),
            WorkspaceCreateTaskField::Description => Some(&mut self.description),
            WorkspaceCreateTaskField::DefinitionOfDone => Some(&mut self.definition_of_done),
            _ => None,
        }
    }

    fn cycle_priority(&mut self) {
        self.priority = match self.priority {
            WorkspacePriority::Low => WorkspacePriority::Normal,
            WorkspacePriority::Normal => WorkspacePriority::High,
            WorkspacePriority::High => WorkspacePriority::Urgent,
            WorkspacePriority::Urgent => WorkspacePriority::Low,
        };
    }

    fn cycle_task_type(&mut self) {
        self.task_type = match self.task_type {
            WorkspaceTaskType::Thread => WorkspaceTaskType::Goal,
            WorkspaceTaskType::Goal => WorkspaceTaskType::Thread,
        };
    }
}

fn required_text_state(value: &str) -> RequiredFieldState {
    if value.is_empty() {
        RequiredFieldState::Missing
    } else {
        RequiredFieldState::Valid
    }
}

fn next_actor_selection(
    options: &[crate::app::workspace_actor_picker::WorkspaceActorPickerOption],
    current: &Option<WorkspaceActor>,
) -> Option<WorkspaceActor> {
    let current_index = options
        .iter()
        .position(|option| option.actor.as_ref() == current.as_ref())
        .unwrap_or(0);
    options
        .get((current_index + 1) % options.len().max(1))
        .and_then(|option| option.actor.clone())
}

#[cfg(test)]
pub(super) fn workspace_create_modal_body(form: &WorkspaceCreateTaskForm) -> String {
    workspace_create_modal_body_with_subagents(form, &crate::state::SubAgentsState::new())
}

#[cfg(test)]
pub(super) fn workspace_create_modal_body_with_subagents(
    form: &WorkspaceCreateTaskForm,
    subagents: &crate::state::SubAgentsState,
) -> String {
    let mut body = String::new();
    for field in WorkspaceCreateTaskField::ALL {
        let marker = if field == form.field { ">" } else { " " };
        let value = match field {
            WorkspaceCreateTaskField::Title => form.title.as_str().to_string(),
            WorkspaceCreateTaskField::TaskType => task_type_label(&form.task_type).to_string(),
            WorkspaceCreateTaskField::Description => form.description.as_str().to_string(),
            WorkspaceCreateTaskField::DefinitionOfDone => {
                form.definition_of_done.as_str().to_string()
            }
            WorkspaceCreateTaskField::Priority => priority_label(&form.priority).to_string(),
            WorkspaceCreateTaskField::Assignee => actor_label(form.assignee.as_ref(), subagents),
            WorkspaceCreateTaskField::Reviewer => actor_label(form.reviewer.as_ref(), subagents),
            WorkspaceCreateTaskField::Submit | WorkspaceCreateTaskField::Cancel => String::new(),
        };
        if value.is_empty() {
            body.push_str(&format!("{marker} {}\n", field.label()));
        } else {
            body.push_str(&format!("{marker} {}: {}\n", field.label(), value));
        }
    }
    body.push_str("\nTab/Shift+Tab navigate - Enter edit/cycle/create - Esc cancel");
    body
}

pub(super) fn workspace_create_modal_lines_with_subagents(
    form: &WorkspaceCreateTaskForm,
    subagents: &crate::state::SubAgentsState,
    theme: &ThemeTokens,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for field in WorkspaceCreateTaskField::ALL {
        lines.push(workspace_create_modal_line(form, subagents, theme, field));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("Tab/Shift+Tab", theme.fg_active),
        Span::styled(" navigate - ", theme.fg_dim),
        Span::styled("Enter", theme.fg_active),
        Span::styled(" edit/cycle/create - ", theme.fg_dim),
        Span::styled("Esc", theme.fg_active),
        Span::styled(" cancel", theme.fg_dim),
    ]));
    lines
}

fn workspace_create_modal_line(
    form: &WorkspaceCreateTaskForm,
    subagents: &crate::state::SubAgentsState,
    theme: &ThemeTokens,
    field: WorkspaceCreateTaskField,
) -> Line<'static> {
    let missing = form.required_field_state(field) == RequiredFieldState::Missing;
    let row_style = if missing {
        theme.accent_danger
    } else if field == form.field {
        theme.fg_active
    } else {
        ratatui::style::Style::default()
    };
    let marker = if field == form.field { ">" } else { " " };
    let required_marker = if matches!(
        form.required_field_state(field),
        RequiredFieldState::Missing | RequiredFieldState::Valid
    ) {
        " *"
    } else {
        ""
    };
    let value = workspace_create_modal_field_value(form, subagents, field);
    let text = if value.is_empty() {
        format!("{marker} {}{required_marker}", field.label())
    } else {
        format!("{marker} {}{required_marker}: {value}", field.label())
    };
    Line::from(Span::styled(text, row_style))
}

fn workspace_create_modal_field_value(
    form: &WorkspaceCreateTaskForm,
    subagents: &crate::state::SubAgentsState,
    field: WorkspaceCreateTaskField,
) -> String {
    match field {
        WorkspaceCreateTaskField::Title => form.title.as_str().to_string(),
        WorkspaceCreateTaskField::TaskType => task_type_label(&form.task_type).to_string(),
        WorkspaceCreateTaskField::Description => form.description.as_str().to_string(),
        WorkspaceCreateTaskField::DefinitionOfDone => form.definition_of_done.as_str().to_string(),
        WorkspaceCreateTaskField::Priority => priority_label(&form.priority).to_string(),
        WorkspaceCreateTaskField::Assignee => actor_label(form.assignee.as_ref(), subagents),
        WorkspaceCreateTaskField::Reviewer => actor_label(form.reviewer.as_ref(), subagents),
        WorkspaceCreateTaskField::Submit | WorkspaceCreateTaskField::Cancel => String::new(),
    }
}

fn priority_label(priority: &WorkspacePriority) -> &'static str {
    match priority {
        WorkspacePriority::Low => "low",
        WorkspacePriority::Normal => "normal",
        WorkspacePriority::High => "high",
        WorkspacePriority::Urgent => "urgent",
    }
}

fn task_type_label(task_type: &WorkspaceTaskType) -> &'static str {
    match task_type {
        WorkspaceTaskType::Thread => "thread",
        WorkspaceTaskType::Goal => "goal",
    }
}

fn actor_label(actor: Option<&WorkspaceActor>, subagents: &crate::state::SubAgentsState) -> String {
    match actor {
        None => "none".to_string(),
        Some(WorkspaceActor::User) => "user".to_string(),
        Some(WorkspaceActor::Agent(id)) if id == AGENT_ID_SWAROG => "svarog".to_string(),
        Some(WorkspaceActor::Agent(id)) => format!("agent:{id}"),
        Some(WorkspaceActor::Subagent(id)) => subagents
            .entries
            .iter()
            .find(|entry| entry.id.eq_ignore_ascii_case(id))
            .and_then(|entry| {
                let name = entry.name.trim();
                (!name.is_empty()).then(|| name.to_string())
            })
            .or_else(|| {
                crate::state::subagents::BUILTIN_PERSONA_ROLE_CHOICES
                    .iter()
                    .find(|choice| choice.id.eq_ignore_ascii_case(id))
                    .map(|choice| choice.label.to_string())
            })
            .unwrap_or_else(|| format!("subagent:{id}")),
    }
}

impl super::TuiModel {
    pub(super) fn open_workspace_create_modal(&mut self, task_type: WorkspaceTaskType) {
        self.pending_workspace_create_form = Some(WorkspaceCreateTaskForm::new(task_type));
        self.modal.reduce(modal::ModalAction::Push(
            modal::ModalKind::WorkspaceCreateTask,
        ));
        self.status_line = "Create workspace task".to_string();
    }

    pub(super) fn submit_workspace_create_modal(&mut self) {
        let Some(form) = self.pending_workspace_create_form.clone() else {
            self.close_top_modal();
            return;
        };
        let request = match form.to_request(self.workspace.workspace_id()) {
            Ok(request) => request,
            Err(message) => {
                self.status_line = message;
                return;
            }
        };
        self.close_top_modal();
        self.send_daemon_command(DaemonCommand::CreateWorkspaceTask(request));
        self.main_pane_view = super::MainPaneView::Workspace;
        self.status_line = "Creating workspace task...".to_string();
    }

    pub(super) fn handle_workspace_create_modal_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        match code {
            KeyCode::Esc => self.close_top_modal(),
            KeyCode::Tab | KeyCode::Down => {
                if let Some(form) = self.pending_workspace_create_form.as_mut() {
                    form.next_field();
                }
            }
            KeyCode::BackTab | KeyCode::Up => {
                if let Some(form) = self.pending_workspace_create_form.as_mut() {
                    form.previous_field();
                }
            }
            KeyCode::Backspace => {
                if let Some(form) = self.pending_workspace_create_form.as_mut() {
                    form.backspace();
                }
            }
            KeyCode::Enter => {
                let field = self
                    .pending_workspace_create_form
                    .as_ref()
                    .map(|form| form.field);
                match field {
                    Some(WorkspaceCreateTaskField::Submit) => self.submit_workspace_create_modal(),
                    Some(WorkspaceCreateTaskField::Cancel) => self.close_top_modal(),
                    Some(WorkspaceCreateTaskField::Assignee) => self
                        .open_workspace_create_actor_picker(
                            crate::app::workspace_actor_picker::WorkspaceActorPickerMode::Assignee,
                        ),
                    Some(WorkspaceCreateTaskField::Reviewer) => self
                        .open_workspace_create_actor_picker(
                            crate::app::workspace_actor_picker::WorkspaceActorPickerMode::Reviewer,
                        ),
                    Some(_) => {
                        if let Some(form) = self.pending_workspace_create_form.as_mut() {
                            form.activate_current_field(&self.subagents);
                        }
                    }
                    None => self.close_top_modal(),
                }
            }
            KeyCode::Char(ch)
                if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                if let Some(form) = self.pending_workspace_create_form.as_mut() {
                    form.insert_char(ch);
                }
            }
            _ => {}
        }
        false
    }

    pub(super) fn paste_into_workspace_create_modal(&mut self, text: &str) {
        if let Some(form) = self.pending_workspace_create_form.as_mut() {
            for ch in text.chars() {
                if !matches!(ch, '\r' | '\n') {
                    form.insert_char(ch);
                }
            }
        }
    }

    fn open_workspace_create_actor_picker(
        &mut self,
        mode: crate::app::workspace_actor_picker::WorkspaceActorPickerMode,
    ) {
        let count = crate::app::workspace_actor_picker::workspace_actor_picker_options(
            mode,
            &self.subagents,
        )
        .len();
        self.pending_workspace_actor_picker = Some(super::PendingWorkspaceActorPicker {
            target: super::PendingWorkspaceActorPickerTarget::CreateForm,
            task_id: "new workspace".to_string(),
            mode,
        });
        self.modal.reduce(modal::ModalAction::Push(
            modal::ModalKind::WorkspaceActorPicker,
        ));
        self.modal.set_picker_item_count(count);
        self.status_line = format!("Select {}", mode.title().to_ascii_lowercase());
    }
}
