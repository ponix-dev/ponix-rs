use anyhow::Result;
use clickhouse::Client;

#[derive(Clone)]
pub struct ClickHouseClient {
    client: Client,
}

impl ClickHouseClient {
    pub fn new(url: &str, database: &str, username: &str, password: &str) -> Self {
        let client = Client::default()
            .with_url(url)
            .with_database(database)
            .with_user(username)
            .with_password(password)
            .with_compression(clickhouse::Compression::Lz4)
            // CRITICAL: All three settings needed for JSON type (from official example)
            .with_option("allow_experimental_json_type", "1")
            .with_option("input_format_binary_read_json_as_string", "1")
            .with_option("output_format_binary_write_json_as_string", "1");

        Self { client }
    }

    pub async fn ping(&self) -> Result<()> {
        self.client.query("SELECT 1").fetch_one::<u8>().await?;
        Ok(())
    }

    pub fn get_client(&self) -> &Client {
        &self.client
    }
}
