#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatisticsTab {
    #[default]
    Overview,
    Providers,
    Models,
    Rankings,
}

impl StatisticsTab {
    pub const ALL: [StatisticsTab; 4] = [
        StatisticsTab::Overview,
        StatisticsTab::Providers,
        StatisticsTab::Models,
        StatisticsTab::Rankings,
    ];

    pub fn prev(self) -> Self {
        let index = Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0);
        if index == 0 {
            Self::ALL[Self::ALL.len() - 1]
        } else {
            Self::ALL[index - 1]
        }
    }

    pub fn next(self) -> Self {
        let index = Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Providers => "Providers",
            Self::Models => "Models",
            Self::Rankings => "Rankings",
        }
    }
}
