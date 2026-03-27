use anyhow::Result;

use crate::client::attach::AttachedClient;

pub struct TuiApp {
    client: AttachedClient,
}

impl TuiApp {
    pub fn new(client: AttachedClient) -> Self {
        Self { client }
    }

    pub fn run(self) -> Result<()> {
        let _ = self.client.state();
        Ok(())
    }
}
