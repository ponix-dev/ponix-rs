use crate::nats::traits::{DocumentContentStore, ObjectInfo};
use anyhow::{Context, Result};
use async_nats::jetstream;
use async_nats::jetstream::object_store::InfoErrorKind;
use async_trait::async_trait;
use tokio::io::AsyncReadExt;
use tracing::debug;

pub struct NatsObjectStoreClient {
    store: jetstream::object_store::ObjectStore,
}

impl NatsObjectStoreClient {
    pub async fn new(jetstream: &jetstream::Context, bucket_name: &str) -> Result<Self> {
        debug!(bucket = %bucket_name, "initializing object store client");

        let store = match jetstream.get_object_store(bucket_name).await {
            Ok(store) => {
                debug!(bucket = %bucket_name, "object store bucket already exists");
                store
            }
            Err(_) => {
                debug!(bucket = %bucket_name, "creating object store bucket");
                jetstream
                    .create_object_store(jetstream::object_store::Config {
                        bucket: bucket_name.to_string(),
                        ..Default::default()
                    })
                    .await
                    .context("failed to create object store bucket")?
            }
        };

        Ok(Self { store })
    }
}

#[async_trait]
impl DocumentContentStore for NatsObjectStoreClient {
    async fn upload(&self, key: &str, content: bytes::Bytes) -> Result<()> {
        let mut reader = &content[..];
        self.store
            .put(key, &mut reader)
            .await
            .context("failed to upload object")?;
        Ok(())
    }

    async fn download(&self, key: &str) -> Result<bytes::Bytes> {
        let mut object = self.store.get(key).await.context("failed to get object")?;

        let mut buf = Vec::new();
        object
            .read_to_end(&mut buf)
            .await
            .context("failed to read object content")?;

        Ok(bytes::Bytes::from(buf))
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.store
            .delete(key)
            .await
            .context("failed to delete object")?;
        Ok(())
    }

    async fn info(&self, key: &str) -> Result<ObjectInfo> {
        self.store
            .info(key)
            .await
            .context("failed to get object info")
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        match self.store.info(key).await {
            Ok(_) => Ok(true),
            Err(err) if err.kind() == InfoErrorKind::NotFound => Ok(false),
            Err(err) => Err(err).context("failed to check object existence"),
        }
    }
}
