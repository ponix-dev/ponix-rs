pub struct AwarenessRelay {
    nats_client: async_nats::Client,
    instance_id: String,
}

impl AwarenessRelay {
    pub fn new(nats_client: async_nats::Client, instance_id: String) -> Self {
        Self {
            nats_client,
            instance_id,
        }
    }

    pub async fn publish(&self, document_id: &str, update: &[u8]) -> anyhow::Result<()> {
        let subject = format!("awareness.{}", document_id);
        let mut headers = async_nats::HeaderMap::new();
        headers.insert("Instance-Id", self.instance_id.as_str());
        self.nats_client
            .publish_with_headers(subject, headers, bytes::Bytes::from(update.to_vec()))
            .await?;
        Ok(())
    }

    pub async fn subscribe(&self, document_id: &str) -> anyhow::Result<async_nats::Subscriber> {
        let subject = format!("awareness.{}", document_id);
        Ok(self.nats_client.subscribe(subject).await?)
    }

    pub fn is_from_self(&self, message: &async_nats::Message) -> bool {
        message
            .headers
            .as_ref()
            .and_then(|h| h.get("Instance-Id"))
            .map(|v| v.as_str() == self.instance_id)
            .unwrap_or(false)
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }
}
