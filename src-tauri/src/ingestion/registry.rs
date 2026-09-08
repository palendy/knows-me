//! Plugin registry: `SourceKind -> Connector` (Pattern P8, NFR-7).
//!
//! Adding a new source means registering another `Connector` here — no other
//! layer changes.

use std::collections::HashMap;
use std::sync::Arc;

use crate::core::traits::Connector;
use crate::core::types::SourceKind;

/// Holds one connector per registered source.
#[derive(Default)]
pub struct ConnectorRegistry {
    connectors: HashMap<SourceKind, Arc<dyn Connector>>,
}

impl ConnectorRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or replace) the connector for its source.
    pub fn register(&mut self, connector: Arc<dyn Connector>) {
        self.connectors.insert(connector.id(), connector);
    }

    pub fn get(&self, source: SourceKind) -> Option<Arc<dyn Connector>> {
        self.connectors.get(&source).cloned()
    }

    /// All registered sources (used to run "all sources" ingestion).
    pub fn sources(&self) -> Vec<SourceKind> {
        self.connectors.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingestion::connectors::FileConnector;

    #[test]
    fn register_and_get() {
        let mut reg = ConnectorRegistry::new();
        reg.register(Arc::new(FileConnector::new()));
        assert!(reg.get(SourceKind::File).is_some());
        assert!(reg.get(SourceKind::Notion).is_none());
        assert_eq!(reg.sources(), vec![SourceKind::File]);
    }
}
