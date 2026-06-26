use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderSecrets {
    pub side: u64,
    pub price: u64,
    pub size: u64,
    pub leverage: u64,
    pub asset: u64,
    pub nonce: u64,
    pub secret: u64,
}

pub struct SecretStore {
    #[allow(dead_code)]
    db: sled::Db,
    tree: sled::Tree,
}

impl SecretStore {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let db = sled::open(path)?;
        let tree = db.open_tree("secrets")?;
        Ok(Self { db, tree })
    }

    pub fn insert(&self, cmt_hex: &str, secrets: &OrderSecrets) -> anyhow::Result<()> {
        let value = serde_json::to_vec(secrets)?;
        self.tree.insert(cmt_hex.as_bytes(), value)?;
        self.tree.flush()?;
        Ok(())
    }

    pub fn get(&self, cmt_hex: &str) -> anyhow::Result<Option<OrderSecrets>> {
        match self.tree.get(cmt_hex.as_bytes())? {
            Some(value) => Ok(Some(serde_json::from_slice(&value)?)),
            None => Ok(None),
        }
    }
}
