use crate::log;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderSecrets {
    pub side: u64,
    pub price: u64,
    pub size: u64,
    pub leverage: u64,
    pub asset: u64,
    pub nonce: u64,
    pub secret: u64,
    #[serde(default)]
    pub is_market: bool,
}

pub struct SecretStore {
    db: sled::Db,
    tree: sled::Tree,
}

impl SecretStore {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let start = Instant::now();
        log::debug!("Opening sled database", "path", format!("{}", path.display()));
        let db = sled::open(path)?;
        let tree = db.open_tree("secrets")?;
        let count = tree.len();
        log::info!("Database opened",
            "path", format!("{}", path.display()),
            "existing_entries", count,
            "took", log::duration_secs(&start.elapsed())
        );
        Ok(Self { db, tree })
    }

    pub fn insert(&self, cmt_hex: &str, secrets: &OrderSecrets) -> anyhow::Result<()> {
        let start = Instant::now();
        let value = serde_json::to_vec(secrets)?;
        let value_size = value.len();
        self.tree.insert(cmt_hex.as_bytes(), value)?;
        self.tree.flush()?;
        let total_entries = self.tree.len();
        log::debug!("Secret inserted into DB",
            "commitment", &cmt_hex[..16],
            "value_bytes", log::bytes_label(value_size),
            "total_entries", total_entries,
            "took", log::duration_secs(&start.elapsed())
        );
        Ok(())
    }

    pub fn get(&self, cmt_hex: &str) -> anyhow::Result<Option<OrderSecrets>> {
        let start = Instant::now();
        log::debug!("Looking up secrets in DB",
            "commitment", &cmt_hex[..16]
        );
        match self.tree.get(cmt_hex.as_bytes())? {
            Some(value) => {
                let secrets: OrderSecrets = serde_json::from_slice(&value)?;
                log::debug!("Secrets found in DB",
                    "commitment", &cmt_hex[..16],
                    "value_bytes", log::bytes_label(value.len()),
                    "took", log::duration_secs(&start.elapsed())
                );
                Ok(Some(secrets))
            }
            None => {
                log::warning!("Secrets NOT found in DB",
                    "commitment", &cmt_hex[..16],
                    "took", log::duration_secs(&start.elapsed())
                );
                Ok(None)
            }
        }
    }
}
