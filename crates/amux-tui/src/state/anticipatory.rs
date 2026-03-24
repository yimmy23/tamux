use crate::wire::AnticipatoryItem;

#[derive(Debug, Clone)]
pub enum AnticipatoryAction {
    Replace(Vec<AnticipatoryItem>),
}

pub struct AnticipatoryState {
    items: Vec<AnticipatoryItem>,
}

impl AnticipatoryState {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn reduce(&mut self, action: AnticipatoryAction) {
        match action {
            AnticipatoryAction::Replace(items) => self.items = items,
        }
    }

    pub fn items(&self) -> &[AnticipatoryItem] {
        &self.items
    }

    pub fn has_items(&self) -> bool {
        !self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_swaps_items() {
        let mut state = AnticipatoryState::new();
        state.reduce(AnticipatoryAction::Replace(vec![AnticipatoryItem {
            id: "a".into(),
            ..AnticipatoryItem::default()
        }]));
        assert_eq!(state.items().len(), 1);
    }
}
