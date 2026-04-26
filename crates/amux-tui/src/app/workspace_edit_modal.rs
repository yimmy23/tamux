use amux_protocol::{WorkspaceActor, WorkspacePriority, WorkspaceTask, WorkspaceTaskUpdate};
use crossterm::event::{KeyCode, KeyModifiers};

use crate::state::{modal, DaemonCommand};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkspaceEditField {
    Title,
    Description,
    DefinitionOfDone,
    Priority,
    Assignee,
    Reviewer,
    Submit,
    Cancel,
}

impl WorkspaceEditField {
    const ALL: [Self; 8] = [
        Self::Title,
        Self::Description,
        Self::DefinitionOfDone,
        Self::Priority,
        Self::Assignee,
        Self::Reviewer,
        Self::Submit,
        Self::Cancel,
    ];

    fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|field| *field == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    fn previous(self) -> Self {
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
            Self::Description => "Description",
            Self::DefinitionOfDone => "Definition of done",
            Self::Priority => "Priority",
            Self::Assignee => "Assignee",
            Self::Reviewer => "Reviewer",
            Self::Submit => "Save",
            Self::Cancel => "Cancel",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WorkspaceEditForm {
    pub(super) task_id: String,
    pub(super) title: String,
    pub(super) description: String,
    pub(super) definition_of_done: String,
    pub(super) priority: WorkspacePriority,
    pub(super) assignee: Option<WorkspaceActor>,
    pub(super) reviewer: Option<WorkspaceActor>,
    pub(super) field: WorkspaceEditField,
}

impl WorkspaceEditForm {
    pub(super) fn from_task(task: &WorkspaceTask) -> Self {
        Self {
            task_id: task.id.clone(),
            title: task.title.clone(),
            description: task.description.clone(),
            definition_of_done: task.definition_of_done.clone().unwrap_or_default(),
            priority: task.priority.clone(),
            assignee: task.assignee.clone(),
            reviewer: task.reviewer.clone(),
            field: WorkspaceEditField::Title,
        }
    }

    pub(super) fn to_update(&self) -> Result<WorkspaceTaskUpdate, String> {
        let title = self.title.trim();
        if title.is_empty() {
            return Err("Title is required".to_string());
        }
        let description = self.description.trim();
        if description.is_empty() {
            return Err("Description is required".to_string());
        }
        Ok(WorkspaceTaskUpdate {
            title: Some(title.to_string()),
            description: Some(description.to_string()),
            definition_of_done: Some(
                self.definition_of_done
                    .trim()
                    .is_empty()
                    .then_some(None)
                    .unwrap_or_else(|| Some(self.definition_of_done.trim().to_string())),
            ),
            priority: Some(self.priority.clone()),
            assignee: Some(self.assignee.clone()),
            reviewer: Some(self.reviewer.clone()),
        })
    }

    fn next_field(&mut self) {
        self.field = self.field.next();
    }

    fn previous_field(&mut self) {
        self.field = self.field.previous();
    }

    fn insert_char(&mut self, ch: char) {
        if let Some(value) = self.active_text_mut() {
            value.push(ch);
        }
    }

    fn backspace(&mut self) {
        if let Some(value) = self.active_text_mut() {
            value.pop();
        }
    }

    fn activate_current_field(&mut self) {
        match self.field {
            WorkspaceEditField::Priority => self.cycle_priority(),
            WorkspaceEditField::Assignee | WorkspaceEditField::Reviewer => {}
            _ => self.next_field(),
        }
    }

    fn active_text_mut(&mut self) -> Option<&mut String> {
        match self.field {
            WorkspaceEditField::Title => Some(&mut self.title),
            WorkspaceEditField::Description => Some(&mut self.description),
            WorkspaceEditField::DefinitionOfDone => Some(&mut self.definition_of_done),
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
}

pub(super) fn workspace_edit_modal_body(form: &WorkspaceEditForm) -> String {
    let mut body = format!(
        "Task: {}\n\n",
        form.task_id.chars().take(12).collect::<String>()
    );
    for field in WorkspaceEditField::ALL {
        let marker = if field == form.field { ">" } else { " " };
        let value = match field {
            WorkspaceEditField::Title => form.title.as_str().to_string(),
            WorkspaceEditField::Description => form.description.as_str().to_string(),
            WorkspaceEditField::DefinitionOfDone => form.definition_of_done.as_str().to_string(),
            WorkspaceEditField::Priority => priority_label(&form.priority).to_string(),
            WorkspaceEditField::Assignee => actor_label(form.assignee.as_ref()),
            WorkspaceEditField::Reviewer => actor_label(form.reviewer.as_ref()),
            WorkspaceEditField::Submit | WorkspaceEditField::Cancel => String::new(),
        };
        if value.is_empty() {
            body.push_str(&format!("{marker} {}\n", field.label()));
        } else {
            body.push_str(&format!("{marker} {}: {}\n", field.label(), value));
        }
    }
    body.push_str("\nTab/Shift+Tab navigate - Enter edit/cycle/save - Esc cancel");
    body
}

fn priority_label(priority: &WorkspacePriority) -> &'static str {
    match priority {
        WorkspacePriority::Low => "low",
        WorkspacePriority::Normal => "normal",
        WorkspacePriority::High => "high",
        WorkspacePriority::Urgent => "urgent",
    }
}

fn actor_label(actor: Option<&WorkspaceActor>) -> String {
    match actor {
        Some(WorkspaceActor::User) => "user".to_string(),
        Some(WorkspaceActor::Agent(id)) => format!("agent:{id}"),
        Some(WorkspaceActor::Subagent(id)) => format!("subagent:{id}"),
        None => "none".to_string(),
    }
}

impl super::TuiModel {
    pub(super) fn open_workspace_edit_modal_for_task(&mut self, task: WorkspaceTask) {
        self.pending_workspace_edit_form = Some(WorkspaceEditForm::from_task(&task));
        self.modal.reduce(modal::ModalAction::Push(
            modal::ModalKind::WorkspaceEditTask,
        ));
        self.status_line = "Edit workspace task".to_string();
    }

    pub(super) fn open_workspace_edit_modal_by_id(&mut self, task_id: String) {
        let task = self
            .workspace
            .projection()
            .columns
            .iter()
            .flat_map(|column| column.tasks.iter())
            .find(|task| task.id == task_id)
            .cloned();
        if let Some(task) = task {
            self.open_workspace_edit_modal_for_task(task);
        } else {
            self.status_line = "Workspace task not found".to_string();
        }
    }

    pub(super) fn workspace_edit_modal_body(&self) -> String {
        self.pending_workspace_edit_form
            .as_ref()
            .map(workspace_edit_modal_body)
            .unwrap_or_else(|| "No workspace task selected".to_string())
    }

    pub(super) fn open_workspace_edit_actor_picker(
        &mut self,
        mode: crate::app::workspace_actor_picker::WorkspaceActorPickerMode,
    ) {
        let Some(form) = self.pending_workspace_edit_form.as_ref() else {
            self.status_line = "No workspace task selected".to_string();
            return;
        };
        let count = crate::app::workspace_actor_picker::workspace_actor_picker_options(
            mode,
            &self.subagents,
        )
        .len();
        self.pending_workspace_actor_picker = Some(super::PendingWorkspaceActorPicker {
            target: super::PendingWorkspaceActorPickerTarget::EditForm,
            task_id: form.task_id.clone(),
            mode,
        });
        self.modal.reduce(modal::ModalAction::Push(
            modal::ModalKind::WorkspaceActorPicker,
        ));
        self.modal.set_picker_item_count(count);
        self.status_line = format!("Select {}", mode.title().to_ascii_lowercase());
    }

    pub(super) fn submit_workspace_edit_modal(&mut self) {
        let Some(form) = self.pending_workspace_edit_form.clone() else {
            self.close_top_modal();
            return;
        };
        let update = match form.to_update() {
            Ok(update) => update,
            Err(message) => {
                self.status_line = message;
                return;
            }
        };
        let task_id = form.task_id;
        self.close_top_modal();
        self.send_daemon_command(DaemonCommand::UpdateWorkspaceTask { task_id, update });
        self.main_pane_view = super::MainPaneView::Workspace;
        self.status_line = "Updating workspace task...".to_string();
    }

    pub(super) fn handle_workspace_edit_modal_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        match code {
            KeyCode::Esc => self.close_top_modal(),
            KeyCode::Tab | KeyCode::Down => {
                if let Some(form) = self.pending_workspace_edit_form.as_mut() {
                    form.next_field();
                }
            }
            KeyCode::BackTab | KeyCode::Up => {
                if let Some(form) = self.pending_workspace_edit_form.as_mut() {
                    form.previous_field();
                }
            }
            KeyCode::Backspace => {
                if let Some(form) = self.pending_workspace_edit_form.as_mut() {
                    form.backspace();
                }
            }
            KeyCode::Enter => {
                let field = self
                    .pending_workspace_edit_form
                    .as_ref()
                    .map(|form| form.field);
                match field {
                    Some(WorkspaceEditField::Submit) => self.submit_workspace_edit_modal(),
                    Some(WorkspaceEditField::Cancel) => self.close_top_modal(),
                    Some(WorkspaceEditField::Assignee) => self.open_workspace_edit_actor_picker(
                        crate::app::workspace_actor_picker::WorkspaceActorPickerMode::Assignee,
                    ),
                    Some(WorkspaceEditField::Reviewer) => self.open_workspace_edit_actor_picker(
                        crate::app::workspace_actor_picker::WorkspaceActorPickerMode::Reviewer,
                    ),
                    Some(_) => {
                        if let Some(form) = self.pending_workspace_edit_form.as_mut() {
                            form.activate_current_field();
                        }
                    }
                    None => self.close_top_modal(),
                }
            }
            KeyCode::Char(ch)
                if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                if let Some(form) = self.pending_workspace_edit_form.as_mut() {
                    form.insert_char(ch);
                }
            }
            _ => {}
        }
        false
    }

    pub(super) fn paste_into_workspace_edit_modal(&mut self, text: &str) {
        if let Some(form) = self.pending_workspace_edit_form.as_mut() {
            for ch in text.chars() {
                if !matches!(ch, '\r' | '\n') {
                    form.insert_char(ch);
                }
            }
        }
    }
}
