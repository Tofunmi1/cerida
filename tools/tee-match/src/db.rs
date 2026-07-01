use crate::{engine, log};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

fn book_key(asset: u64) -> [u8; 10] {
    let mut buf = [0u8; 10];
    buf[..5].copy_from_slice(b"book_");
    buf[5..].copy_from_slice(&asset.to_le_bytes());
    buf
}

pub struct SecretStore {
    _db: sled::Db,
    tree: sled::Tree,
}

impl SecretStore {
    pub fn open(db: &sled::Db) -> anyhow::Result<Self> {
        let start = Instant::now();
        let tree = db.open_tree("secrets")?;
        let count = tree.len();
        log::info!("Secret store opened",
            "existing_entries", count,
            "took", log::duration_secs(&start.elapsed())
        );
        Ok(Self { _db: db.clone(), tree })
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

pub fn open_db(path: &std::path::Path) -> anyhow::Result<sled::Db> {
    let start = Instant::now();
    log::debug!("Opening sled database", "path", format!("{}", path.display()));
    let db = sled::open(path)?;
    log::info!("Database opened",
        "path", format!("{}", path.display()),
        "took", log::duration_secs(&start.elapsed())
    );
    Ok(db)
}

pub struct BookStore {
    tree: sled::Tree,
}

impl BookStore {
    pub fn open(db: &sled::Db) -> anyhow::Result<Self> {
        let tree = db.open_tree("book")?;
        Ok(Self { tree })
    }

    pub fn save_book(&self, asset: u64, book: &engine::OrderBook) -> anyhow::Result<()> {
        let start = Instant::now();
        let value = serde_json::to_vec(book)?;
        let size = value.len();
        let key = book_key(asset);
        self.tree.insert(&key, value)?;
        self.tree.flush()?;
        log::debug!("OrderBook saved to DB",
            "asset", asset,
            "order_count", book.order_count(),
            "size_bytes", size,
            "took", log::duration_secs(&start.elapsed())
        );
        Ok(())
    }

    pub fn load_book(&self, asset: u64) -> anyhow::Result<Option<engine::OrderBook>> {
        let key = book_key(asset);
        match self.tree.get(&key)? {
            Some(value) => {
                let book: engine::OrderBook = serde_json::from_slice(&value)?;
                log::info!("OrderBook loaded from DB",
                    "asset", asset,
                    "order_count", book.order_count()
                );
                Ok(Some(book))
            }
            None => Ok(None),
        }
    }

    pub fn load_all(&self) -> anyhow::Result<HashMap<u64, engine::OrderBook>> {
        let start = Instant::now();
        let mut books = HashMap::new();
        for result in self.tree.iter() {
            let (key, value) = result?;
            if key.len() == 10 && &key[..5] == b"book_" {
                let asset = u64::from_le_bytes(key[5..].try_into().unwrap());
                match serde_json::from_slice::<engine::OrderBook>(&value) {
                    Ok(book) => {
                        log::debug!("Loaded book", "asset", asset);
                        books.insert(asset, book);
                    }
                    Err(e) => log::error!("Failed to deserialize book", "asset", asset, "err", e.to_string()),
                }
            }
        }
        log::info!("Loaded books from DB", "count", books.len(), "took", log::duration_secs(&start.elapsed()));
        Ok(books)
    }
}
