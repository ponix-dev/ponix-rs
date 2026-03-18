use futures_util::future::BoxFuture;

pub type RemoteUpdateHandler = Box<dyn Fn(Vec<u8>) -> BoxFuture<'static, ()> + Send + Sync>;

#[async_trait::async_trait]
pub trait DocumentRelay: Send + Sync {
    async fn publish_update(&self, document_id: &str, update: &[u8]) -> Result<(), anyhow::Error>;
    async fn subscribe_updates(
        &self,
        document_id: &str,
        updated_at: Option<chrono::DateTime<chrono::Utc>>,
        stream_name: &str,
        handler: RemoteUpdateHandler,
    ) -> Result<(), anyhow::Error>;
    async fn unsubscribe_updates(&self, document_id: &str);
    async fn publish_awareness(
        &self,
        document_id: &str,
        update: &[u8],
    ) -> Result<(), anyhow::Error>;
    async fn subscribe_awareness(
        &self,
        document_id: &str,
        handler: RemoteUpdateHandler,
    ) -> Result<(), anyhow::Error>;
    async fn unsubscribe_awareness(&self, document_id: &str);
}
