//! `RemoteSource` — a `Source` that lives on the server.
//!
//! Holds only the descriptor the server returned from `open_folder`; every
//! call is a server function. Errors from the transport become
//! `SourceError::Io`; errors from the remote source come through as
//! themselves (the server functions return `Result<Result<_, SourceError>,
//! ServerFnError>` on purpose, so a `Conflict` on the server is a `Conflict`
//! on the client).

use dioxus::prelude::ServerFnError;
use moonkale_core::{
    async_trait, Applied, NodeId, Query, QueryResult, Source, SourceDescriptor, SourceError,
    SourceId, Transaction, Version,
};

#[derive(Clone, Debug)]
pub struct RemoteSource {
    descriptor: SourceDescriptor,
}

impl RemoteSource {
    /// Open a folder on the server; returns the folder and its index.
    pub async fn open_folder(
        path: &str,
        embed: Option<moonkale_llm::LlmSettings>,
    ) -> Result<Vec<Self>, SourceError> {
        let descriptors = crate::open_folder(path.to_string(), embed)
            .await
            .map_err(transport)?;
        Ok(descriptors
            .into_iter()
            .map(|descriptor| Self { descriptor })
            .collect())
    }

    pub fn from_descriptor(descriptor: SourceDescriptor) -> Self {
        Self { descriptor }
    }
}

fn transport(e: ServerFnError) -> SourceError {
    SourceError::Io(e.to_string())
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Source for RemoteSource {
    fn id(&self) -> SourceId {
        self.descriptor.id.clone()
    }

    fn descriptor(&self) -> SourceDescriptor {
        self.descriptor.clone()
    }

    async fn query(&self, query: Query) -> Result<QueryResult, SourceError> {
        crate::query_source(self.id(), query)
            .await
            .map_err(transport)?
    }

    async fn fetch_text(&self, node: NodeId) -> Result<(String, Version), SourceError> {
        crate::fetch_text_from(self.id(), node)
            .await
            .map_err(transport)?
    }

    async fn apply(&self, tx: Transaction) -> Result<Applied, SourceError> {
        crate::apply_to(self.id(), tx).await.map_err(transport)?
    }

    async fn refresh(&self, node: NodeId) -> Result<(), SourceError> {
        crate::refresh_source(self.id(), node)
            .await
            .map_err(transport)?
    }
}
