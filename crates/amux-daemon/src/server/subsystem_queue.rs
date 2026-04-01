#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum BackgroundSubsystem {
    ConciergeWork,
    AgentWork,
    ProviderIo,
    PluginIo,
    ConfigReconcile,
}

impl BackgroundSubsystem {
    pub(super) const ALL: [Self; 5] = [
        Self::ConciergeWork,
        Self::AgentWork,
        Self::ProviderIo,
        Self::PluginIo,
        Self::ConfigReconcile,
    ];
}

pub(super) enum BackgroundSignal {
    Deliver(DaemonMessage),
    Finished,
}

pub(super) struct BackgroundSubsystemQueues {
    concierge_work_tx: tokio::sync::mpsc::UnboundedSender<BackgroundSignal>,
    concierge_work_rx: tokio::sync::mpsc::UnboundedReceiver<BackgroundSignal>,
    agent_work_tx: tokio::sync::mpsc::UnboundedSender<BackgroundSignal>,
    agent_work_rx: tokio::sync::mpsc::UnboundedReceiver<BackgroundSignal>,
    provider_io_tx: tokio::sync::mpsc::UnboundedSender<BackgroundSignal>,
    provider_io_rx: tokio::sync::mpsc::UnboundedReceiver<BackgroundSignal>,
    plugin_io_tx: tokio::sync::mpsc::UnboundedSender<BackgroundSignal>,
    plugin_io_rx: tokio::sync::mpsc::UnboundedReceiver<BackgroundSignal>,
    config_reconcile_tx: tokio::sync::mpsc::UnboundedSender<BackgroundSignal>,
    config_reconcile_rx: tokio::sync::mpsc::UnboundedReceiver<BackgroundSignal>,
}

impl BackgroundSubsystemQueues {
    pub(super) fn new() -> Self {
        let (concierge_work_tx, concierge_work_rx) =
            tokio::sync::mpsc::unbounded_channel::<BackgroundSignal>();
        let (agent_work_tx, agent_work_rx) =
            tokio::sync::mpsc::unbounded_channel::<BackgroundSignal>();
        let (provider_io_tx, provider_io_rx) =
            tokio::sync::mpsc::unbounded_channel::<BackgroundSignal>();
        let (plugin_io_tx, plugin_io_rx) =
            tokio::sync::mpsc::unbounded_channel::<BackgroundSignal>();
        let (config_reconcile_tx, config_reconcile_rx) =
            tokio::sync::mpsc::unbounded_channel::<BackgroundSignal>();

        Self {
            concierge_work_tx,
            concierge_work_rx,
            agent_work_tx,
            agent_work_rx,
            provider_io_tx,
            provider_io_rx,
            plugin_io_tx,
            plugin_io_rx,
            config_reconcile_tx,
            config_reconcile_rx,
        }
    }

    pub(super) fn sender(
        &self,
        subsystem: BackgroundSubsystem,
    ) -> tokio::sync::mpsc::UnboundedSender<BackgroundSignal> {
        match subsystem {
            BackgroundSubsystem::ConciergeWork => self.concierge_work_tx.clone(),
            BackgroundSubsystem::AgentWork => self.agent_work_tx.clone(),
            BackgroundSubsystem::ProviderIo => self.provider_io_tx.clone(),
            BackgroundSubsystem::PluginIo => self.plugin_io_tx.clone(),
            BackgroundSubsystem::ConfigReconcile => self.config_reconcile_tx.clone(),
        }
    }

    pub(super) fn try_recv(
        &mut self,
        subsystem: BackgroundSubsystem,
    ) -> Result<BackgroundSignal, tokio::sync::mpsc::error::TryRecvError> {
        match subsystem {
            BackgroundSubsystem::ConciergeWork => self.concierge_work_rx.try_recv(),
            BackgroundSubsystem::AgentWork => self.agent_work_rx.try_recv(),
            BackgroundSubsystem::ProviderIo => self.provider_io_rx.try_recv(),
            BackgroundSubsystem::PluginIo => self.plugin_io_rx.try_recv(),
            BackgroundSubsystem::ConfigReconcile => self.config_reconcile_rx.try_recv(),
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct BackgroundPendingCounts {
    concierge_work: usize,
    agent_work: usize,
    provider_io: usize,
    plugin_io: usize,
    config_reconcile: usize,
}

impl BackgroundPendingCounts {
    pub(super) fn capacity(subsystem: BackgroundSubsystem) -> usize {
        match subsystem {
            BackgroundSubsystem::ConciergeWork => 4,
            BackgroundSubsystem::AgentWork => 32,
            BackgroundSubsystem::ProviderIo => 16,
            BackgroundSubsystem::PluginIo => 16,
            BackgroundSubsystem::ConfigReconcile => 8,
        }
    }

    pub(super) fn try_increment(&mut self, subsystem: BackgroundSubsystem) -> bool {
        let current = match subsystem {
            BackgroundSubsystem::ConciergeWork => self.concierge_work,
            BackgroundSubsystem::AgentWork => self.agent_work,
            BackgroundSubsystem::ProviderIo => self.provider_io,
            BackgroundSubsystem::PluginIo => self.plugin_io,
            BackgroundSubsystem::ConfigReconcile => self.config_reconcile,
        };

        if current >= Self::capacity(subsystem) {
            return false;
        }

        self.increment(subsystem);
        true
    }

    pub(super) fn has_capacity(&self, subsystem: BackgroundSubsystem) -> bool {
        let current = match subsystem {
            BackgroundSubsystem::ConciergeWork => self.concierge_work,
            BackgroundSubsystem::AgentWork => self.agent_work,
            BackgroundSubsystem::ProviderIo => self.provider_io,
            BackgroundSubsystem::PluginIo => self.plugin_io,
            BackgroundSubsystem::ConfigReconcile => self.config_reconcile,
        };

        current < Self::capacity(subsystem)
    }

    pub(super) fn increment(&mut self, subsystem: BackgroundSubsystem) {
        match subsystem {
            BackgroundSubsystem::ConciergeWork => {
                self.concierge_work = self.concierge_work.saturating_add(1);
                subsystem_metrics().record_depth(subsystem, self.concierge_work);
            }
            BackgroundSubsystem::AgentWork => {
                self.agent_work = self.agent_work.saturating_add(1);
                subsystem_metrics().record_depth(subsystem, self.agent_work);
            }
            BackgroundSubsystem::ProviderIo => {
                self.provider_io = self.provider_io.saturating_add(1);
                subsystem_metrics().record_depth(subsystem, self.provider_io);
            }
            BackgroundSubsystem::PluginIo => {
                self.plugin_io = self.plugin_io.saturating_add(1);
                subsystem_metrics().record_depth(subsystem, self.plugin_io);
            }
            BackgroundSubsystem::ConfigReconcile => {
                self.config_reconcile = self.config_reconcile.saturating_add(1);
                subsystem_metrics().record_depth(subsystem, self.config_reconcile);
            }
        }
    }

    pub(super) fn decrement(&mut self, subsystem: BackgroundSubsystem) {
        match subsystem {
            BackgroundSubsystem::ConciergeWork => {
                self.concierge_work = self.concierge_work.saturating_sub(1);
                subsystem_metrics().record_depth(subsystem, self.concierge_work);
            }
            BackgroundSubsystem::AgentWork => {
                self.agent_work = self.agent_work.saturating_sub(1);
                subsystem_metrics().record_depth(subsystem, self.agent_work);
            }
            BackgroundSubsystem::ProviderIo => {
                self.provider_io = self.provider_io.saturating_sub(1);
                subsystem_metrics().record_depth(subsystem, self.provider_io);
            }
            BackgroundSubsystem::PluginIo => {
                self.plugin_io = self.plugin_io.saturating_sub(1);
                subsystem_metrics().record_depth(subsystem, self.plugin_io);
            }
            BackgroundSubsystem::ConfigReconcile => {
                self.config_reconcile = self.config_reconcile.saturating_sub(1);
                subsystem_metrics().record_depth(subsystem, self.config_reconcile);
            }
        }
    }

    pub(super) fn note_rejection(&self, subsystem: BackgroundSubsystem) {
        subsystem_metrics().record_rejection(subsystem);
    }

    pub(super) fn any(&self) -> bool {
        self.concierge_work > 0
            || self.agent_work > 0
            || self.provider_io > 0
            || self.plugin_io > 0
            || self.config_reconcile > 0
    }
}

#[cfg(test)]
mod subsystem_queue_tests {
    use super::*;

    #[tokio::test]
    async fn subsystem_queues_isolate_pending_messages_by_domain() {
        let mut queues = BackgroundSubsystemQueues::new();

        queues
            .sender(BackgroundSubsystem::PluginIo)
            .send(BackgroundSignal::Deliver(DaemonMessage::Pong))
            .expect("send plugin delivery");
        queues
            .sender(BackgroundSubsystem::AgentWork)
            .send(BackgroundSignal::Deliver(DaemonMessage::Pong))
            .expect("send agent delivery");

        assert!(matches!(
            queues.try_recv(BackgroundSubsystem::AgentWork),
            Ok(BackgroundSignal::Deliver(DaemonMessage::Pong))
        ));
        assert!(matches!(
            queues.try_recv(BackgroundSubsystem::AgentWork),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));
        assert!(matches!(
            queues.try_recv(BackgroundSubsystem::PluginIo),
            Ok(BackgroundSignal::Deliver(DaemonMessage::Pong))
        ));
    }

    #[test]
    fn pending_capacity_is_enforced_per_domain() {
        let mut counts = BackgroundPendingCounts::default();

        for _ in 0..BackgroundPendingCounts::capacity(BackgroundSubsystem::PluginIo) {
            assert!(counts.try_increment(BackgroundSubsystem::PluginIo));
        }

        assert!(!counts.try_increment(BackgroundSubsystem::PluginIo));
        assert!(counts.try_increment(BackgroundSubsystem::AgentWork));
        assert!(counts.any());
    }

    #[test]
    fn subsystem_metrics_track_depth_and_rejections() {
        let before = subsystem_metrics().snapshot_for(BackgroundSubsystem::PluginIo);

        let mut counts = BackgroundPendingCounts::default();
        counts.increment(BackgroundSubsystem::PluginIo);
        counts.increment(BackgroundSubsystem::PluginIo);
        counts.decrement(BackgroundSubsystem::PluginIo);
        counts.note_rejection(BackgroundSubsystem::PluginIo);

        let snapshot = subsystem_metrics().snapshot_for(BackgroundSubsystem::PluginIo);
        assert_eq!(snapshot.current_depth, 1);
        assert!(snapshot.max_depth >= 2);
        assert!(snapshot.rejection_count >= before.rejection_count.saturating_add(1));
    }

    #[tokio::test]
    async fn subsystem_queues_support_completion_signals_without_messages() {
        let mut queues = BackgroundSubsystemQueues::new();

        queues
            .sender(BackgroundSubsystem::AgentWork)
            .send(BackgroundSignal::Finished)
            .expect("send completion signal");

        assert!(matches!(
            queues.try_recv(BackgroundSubsystem::AgentWork),
            Ok(BackgroundSignal::Finished)
        ));
    }
}
