use std::time::Duration;

use reqwest::Client;

pub struct HttpClient {
    inner: Client,
}

impl HttpClient {
    pub fn new() -> Result<Self, reqwest::Error> {
        let inner = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self { inner })
    }

    pub fn inner(&self) -> &Client {
        &self.inner
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new().expect("failed to build HTTP client")
    }
}
