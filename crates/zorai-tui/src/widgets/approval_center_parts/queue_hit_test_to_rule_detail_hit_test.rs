use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders};

use crate::state::approval::{ApprovalFilter, ApprovalState};

use super::render_to_header_hit_test::ApprovalCenterHitTarget;

pub(crate) fn queue_hit_test(
    area: Rect,
    approval: &ApprovalState,
    current_thread_id: Option<&str>,
    current_workspace_id: Option<&str>,
    position: Position,
) -> Option<ApprovalCenterHitTarget> {
    if approval.filter() == ApprovalFilter::SavedRules {
        return rule_queue_hit_test(area, approval, position);
    }
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(area);
    if position.x < inner.x
        || position.x >= inner.x.saturating_add(inner.width)
        || position.y < inner.y
        || position.y >= inner.y.saturating_add(inner.height)
    {
        return None;
    }
    let row = position.y.saturating_sub(inner.y) as usize;
    let item_index = row / 3;
    approval
        .visible_approvals(current_thread_id, current_workspace_id)
        .get(item_index)
        .map(|_| ApprovalCenterHitTarget::Row(item_index))
}

fn rule_queue_hit_test(
    area: Rect,
    approval: &ApprovalState,
    position: Position,
) -> Option<ApprovalCenterHitTarget> {
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(area);
    if position.x < inner.x
        || position.x >= inner.x.saturating_add(inner.width)
        || position.y < inner.y
        || position.y >= inner.y.saturating_add(inner.height)
    {
        return None;
    }
    let row = position.y.saturating_sub(inner.y) as usize;
    let item_index = row / 3;
    approval
        .saved_rules()
        .get(item_index)
        .map(|_| ApprovalCenterHitTarget::RuleRow(item_index))
}

pub(crate) fn detail_hit_test(
    area: Rect,
    approval: &ApprovalState,
    current_thread_id: Option<&str>,
    current_workspace_id: Option<&str>,
    position: Position,
) -> Option<ApprovalCenterHitTarget> {
    if approval.filter() == ApprovalFilter::SavedRules {
        return rule_detail_hit_test(area, approval, position);
    }
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(area);
    if position.x < inner.x
        || position.x >= inner.x.saturating_add(inner.width)
        || position.y < inner.y
        || position.y >= inner.y.saturating_add(inner.height)
    {
        return None;
    }

    let selected = approval.selected_visible_approval(current_thread_id, current_workspace_id)?;

    if position.y == inner.y.saturating_add(1)
        && position.x >= inner.x.saturating_add(8)
        && position.x < inner.x.saturating_add(32)
    {
        return selected
            .thread_id
            .clone()
            .map(ApprovalCenterHitTarget::ThreadJump);
    }

    let action_y = inner.y.saturating_add(inner.height.saturating_sub(2));
    if position.y != action_y {
        return None;
    }
    let approve_once_x = inner.x.saturating_add(20);
    let approve_once_w = 14;
    let approve_session_x = approve_once_x.saturating_add(16);
    let approve_session_w = 17;
    let always_approve_x = approve_session_x.saturating_add(19);
    let always_approve_w = 16;
    let deny_x = always_approve_x.saturating_add(18);
    let deny_w = 6;

    if position.x >= approve_once_x && position.x < approve_once_x.saturating_add(approve_once_w) {
        return Some(ApprovalCenterHitTarget::ApproveOnce(
            selected.approval_id.clone(),
        ));
    }
    if position.x >= approve_session_x
        && position.x < approve_session_x.saturating_add(approve_session_w)
    {
        return Some(ApprovalCenterHitTarget::ApproveSession(
            selected.approval_id.clone(),
        ));
    }
    if position.x >= always_approve_x
        && position.x < always_approve_x.saturating_add(always_approve_w)
    {
        return Some(ApprovalCenterHitTarget::AlwaysApprove(
            selected.approval_id.clone(),
        ));
    }
    if position.x >= deny_x && position.x < deny_x.saturating_add(deny_w) {
        return Some(ApprovalCenterHitTarget::Deny(selected.approval_id.clone()));
    }

    None
}

fn rule_detail_hit_test(
    area: Rect,
    approval: &ApprovalState,
    position: Position,
) -> Option<ApprovalCenterHitTarget> {
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(area);
    if position.x < inner.x
        || position.x >= inner.x.saturating_add(inner.width)
        || position.y < inner.y
        || position.y >= inner.y.saturating_add(inner.height)
    {
        return None;
    }
    let selected = approval.selected_rule()?;
    let action_y = inner.y.saturating_add(inner.height.saturating_sub(2));
    let revoke_x = inner.x.saturating_add(20);
    let revoke_w = 13;
    if position.y == action_y
        && position.x >= revoke_x
        && position.x < revoke_x.saturating_add(revoke_w)
    {
        return Some(ApprovalCenterHitTarget::RevokeRule(selected.id.clone()));
    }
    None
}
