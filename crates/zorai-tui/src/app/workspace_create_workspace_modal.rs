use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::text::{Line, Span};
use zorai_protocol::WorkspaceOperator;

use crate::state::{modal, DaemonCommand};
use crate::theme::ThemeTokens;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkspaceCreateField {
    WorkspaceId,
    Operator,
    Submit,
    Cancel,
}

impl WorkspaceCreateField {
    const ALL: [Self; 4] = [Self::WorkspaceId, Self::Operator, Self::Submit, Self::Cancel];

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
            Self::WorkspaceId => "Workspace",
            Self::Operator => "Operator",
            Self::Submit => "Create",
            Self::Cancel => "Cancel",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WorkspaceCreateForm {
    pub(super) workspace_id: String,
    pub(super) operator: WorkspaceOperator,
    pub(super) field: WorkspaceCreateField,
}

impl WorkspaceCreateForm {
    pub(super) fn new() -> Self {
        Self {
            workspace_id: String::new(),
            operator: WorkspaceOperator::User,
            field: WorkspaceCreateField::WorkspaceId,
        }
    }

    fn to_request(&self) -> Result<(String, WorkspaceOperator), String> {
        let workspace_id = self.workspace_id.trim();
        if workspace_id.is_empty() {
            return Err("Workspace name is required".to_string());
        }
        Ok((workspace_id.to_string(), self.operator.clone()))
    }

    fn next_field(&mut self) {
        self.field = self.field.next();
    }

    fn previous_field(&mut self) {
        self.field = self.field.previous();
    }

    fn insert_char(&mut self, ch: char) {
        if self.field == WorkspaceCreateField::WorkspaceId {
            self.workspace_id.push(ch);
        }
    }

    fn backspace(&mut self) {
        if self.field == WorkspaceCreateField::WorkspaceId {
            self.workspace_id.pop();
        }
    }

    fn activate_current_field(&mut self) {
        match self.field {
            WorkspaceCreateField::Operator => self.operator = next_operator(&self.operator),
            _ => self.next_field(),
        }
    }
}

fn next_operator(operator: &WorkspaceOperator) -> WorkspaceOperator {
    match operator {
        WorkspaceOperator::User => WorkspaceOperator::Svarog,
        WorkspaceOperator::Svarog => WorkspaceOperator::User,
    }
}

pub(super) fn workspace_create_modal_lines(
    form: &WorkspaceCreateForm,
    theme: &ThemeTokens,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for field in WorkspaceCreateField::ALL {
        let marker = if field == form.field { ">" } else { " " };
        let value = match field {
            WorkspaceCreateField::WorkspaceId => form.workspace_id.clone(),
            WorkspaceCreateField::Operator => operator_label(&form.operator).to_string(),
            WorkspaceCreateField::Submit | WorkspaceCreateField::Cancel => String::new(),
        };
        let row_style = if field == form.field {
            theme.fg_active
        } else if field == WorkspaceCreateField::WorkspaceId && form.workspace_id.trim().is_empty()
        {
            theme.accent_danger
        } else {
            ratatui::style::Style::default()
        };
        let required_marker = if field == WorkspaceCreateField::WorkspaceId {
            " *"
        } else {
            ""
        };
        let text = if value.is_empty() {
            format!("{marker} {}{required_marker}", field.label())
        } else {
            format!("{marker} {}{required_marker}: {value}", field.label())
        };
        lines.push(Line::from(Span::styled(text, row_style)));
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

fn operator_label(operator: &WorkspaceOperator) -> &'static str {
    match operator {
        WorkspaceOperator::User => "user",
        WorkspaceOperator::Svarog => "svarog",
    }
}

impl super::TuiModel {
    pub(super) fn open_workspace_create_workspace_modal(&mut self) {
        self.pending_workspace_create_workspace_form = Some(WorkspaceCreateForm::new());
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::WorkspaceCreate));
        self.status_line = "Create workspace".to_string();
    }

    pub(super) fn open_workspace_create_workspace_modal_with_values(
        &mut self,
        workspace_id: String,
        operator: WorkspaceOperator,
    ) {
        let mut form = WorkspaceCreateForm::new();
        form.workspace_id = workspace_id;
        form.operator = operator;
        self.pending_workspace_create_workspace_form = Some(form);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::WorkspaceCreate));
        self.status_line = "Review workspace details".to_string();
    }

    pub(super) fn submit_workspace_create_workspace_modal(&mut self) {
        let Some(form) = self.pending_workspace_create_workspace_form.clone() else {
            self.close_top_modal();
            return;
        };
        let (workspace_id, operator) = match form.to_request() {
            Ok(request) => request,
            Err(message) => {
                self.status_line = message;
                return;
            }
        };

        self.close_top_modal();
        self.workspace.switch_workspace(&workspace_id);
        self.workspace.set_operator(operator.clone());
        self.main_pane_view = super::MainPaneView::Workspace;
        self.focus = super::FocusArea::Chat;
        self.send_daemon_command(DaemonCommand::SetWorkspaceOperator {
            workspace_id: workspace_id.clone(),
            operator,
        });
        self.send_daemon_command(DaemonCommand::ListWorkspaceTasks {
            workspace_id: workspace_id.clone(),
            include_deleted: self.workspace.filter().include_deleted,
        });
        self.send_daemon_command(DaemonCommand::ListWorkspaceNotices {
            workspace_id: workspace_id.clone(),
            task_id: None,
        });
        self.status_line = format!("Creating workspace {workspace_id}...");
    }

    pub(super) fn handle_workspace_create_workspace_modal_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        match code {
            KeyCode::Esc => self.close_top_modal(),
            KeyCode::Tab | KeyCode::Down => {
                if let Some(form) = self.pending_workspace_create_workspace_form.as_mut() {
                    form.next_field();
                }
            }
            KeyCode::BackTab | KeyCode::Up => {
                if let Some(form) = self.pending_workspace_create_workspace_form.as_mut() {
                    form.previous_field();
                }
            }
            KeyCode::Backspace => {
                if let Some(form) = self.pending_workspace_create_workspace_form.as_mut() {
                    form.backspace();
                }
            }
            KeyCode::Enter => {
                let field = self
                    .pending_workspace_create_workspace_form
                    .as_ref()
                    .map(|form| form.field);
                match field {
                    Some(WorkspaceCreateField::Submit) => {
                        self.submit_workspace_create_workspace_modal();
                    }
                    Some(WorkspaceCreateField::Cancel) => self.close_top_modal(),
                    Some(_) => {
                        if let Some(form) = self.pending_workspace_create_workspace_form.as_mut() {
                            form.activate_current_field();
                        }
                    }
                    None => self.close_top_modal(),
                }
            }
            KeyCode::Char(ch)
                if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                if let Some(form) = self.pending_workspace_create_workspace_form.as_mut() {
                    form.insert_char(ch);
                }
            }
            _ => {}
        }
        false
    }

    pub(super) fn paste_into_workspace_create_workspace_modal(&mut self, text: &str) {
        if let Some(form) = self.pending_workspace_create_workspace_form.as_mut() {
            for ch in text.chars() {
                if !matches!(ch, '\r' | '\n') {
                    form.insert_char(ch);
                }
            }
        }
    }
}
