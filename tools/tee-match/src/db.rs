use crate::{engine, log};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
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

fn book_key(asset: u64) -> [u8; 13] {
    let mut buf = [0u8; 13];
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
        log::info!(
            "Secret store opened",
            "existing_entries",
            count,
            "took",
            log::duration_secs(&start.elapsed())
        );
        Ok(Self {
            _db: db.clone(),
            tree,
        })
    }

    pub fn insert(&self, cmt_hex: &str, secrets: &OrderSecrets) -> anyhow::Result<()> {
        let start = Instant::now();
        let value = serde_json::to_vec(secrets)?;
        let value_size = value.len();
        self.tree.insert(cmt_hex.as_bytes(), value)?;
        self.tree.flush()?;
        let total_entries = self.tree.len();
        log::debug!(
            "Secret inserted into DB",
            "commitment",
            &cmt_hex[..16],
            "value_bytes",
            log::bytes_label(value_size),
            "total_entries",
            total_entries,
            "took",
            log::duration_secs(&start.elapsed())
        );
        Ok(())
    }

    pub fn get(&self, cmt_hex: &str) -> anyhow::Result<Option<OrderSecrets>> {
        let start = Instant::now();
        log::debug!("Looking up secrets in DB", "commitment", &cmt_hex[..16]);
        match self.tree.get(cmt_hex.as_bytes())? {
            Some(value) => {
                let secrets: OrderSecrets = serde_json::from_slice(&value)?;
                log::debug!(
                    "Secrets found in DB",
                    "commitment",
                    &cmt_hex[..16],
                    "value_bytes",
                    log::bytes_label(value.len()),
                    "took",
                    log::duration_secs(&start.elapsed())
                );
                Ok(Some(secrets))
            }
            None => {
                log::warning!(
                    "Secrets NOT found in DB",
                    "commitment",
                    &cmt_hex[..16],
                    "took",
                    log::duration_secs(&start.elapsed())
                );
                Ok(None)
            }
        }
    }

    /// List all commitment hex strings stored in the DB.
    pub fn list(&self) -> anyhow::Result<Vec<String>> {
        let mut cmts = Vec::new();
        for item in self.tree.iter() {
            let (key, _) = item?;
            if let Ok(s) = String::from_utf8(key.to_vec()) {
                cmts.push(s);
            }
        }
        Ok(cmts)
    }
}

// ── Fill Audit Trail ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillEntry {
    pub taker_cmt: String,
    pub maker_cmt: String,
    pub price: u64,
    pub size: u64,
    pub asset: u64,
    pub status: String, // "pending" | "confirmed" | "failed"
    pub timestamp_ns: u128,
}

#[derive(Clone)]
pub struct FillLedger {
    tree: sled::Tree,
    counter: Arc<AtomicU64>,
}

impl FillLedger {
    pub fn open(db: &sled::Db) -> anyhow::Result<Self> {
        let tree = db.open_tree("fills")?;
        let count = tree.len() as u64;
        log::info!("Fill ledger opened", "existing_entries", count);
        Ok(Self {
            tree,
            counter: Arc::new(AtomicU64::new(count)),
        })
    }

    pub fn record(
        &self,
        taker: &str,
        maker: &str,
        price: u64,
        size: u64,
        asset: u64,
        status: &str,
    ) -> anyhow::Result<()> {
        let id = self.counter.fetch_add(1, Ordering::Relaxed);
        let entry = FillEntry {
            taker_cmt: taker.to_string(),
            maker_cmt: maker.to_string(),
            price,
            size,
            asset,
            status: status.to_string(),
            timestamp_ns: engine::now_nanos(),
        };
        let key = format!("{:020}", id);
        self.tree
            .insert(key.as_bytes(), serde_json::to_vec(&entry)?)?;
        self.tree.flush()?;
        log::debug!(
            "Fill recorded",
            "id",
            key,
            "taker",
            engine::short_id(taker),
            "maker",
            engine::short_id(maker),
            "price",
            price,
            "size",
            size,
            "status",
            status
        );
        Ok(())
    }

    pub fn count(&self) -> u64 {
        self.counter.load(Ordering::Relaxed)
    }
}

pub fn open_db(path: &std::path::Path) -> anyhow::Result<sled::Db> {
    let start = Instant::now();
    log::debug!(
        "Opening sled database",
        "path",
        format!("{}", path.display())
    );
    let db = sled::open(path)?;
    log::info!(
        "Database opened",
        "path",
        format!("{}", path.display()),
        "took",
        log::duration_secs(&start.elapsed())
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
        log::debug!(
            "OrderBook saved to DB",
            "asset",
            asset,
            "order_count",
            book.order_count(),
            "size_bytes",
            size,
            "took",
            log::duration_secs(&start.elapsed())
        );
        Ok(())
    }

    pub fn load_book(&self, asset: u64) -> anyhow::Result<Option<engine::OrderBook>> {
        let key = book_key(asset);
        match self.tree.get(&key)? {
            Some(value) => {
                let book: engine::OrderBook = serde_json::from_slice(&value)?;
                log::info!(
                    "OrderBook loaded from DB",
                    "asset",
                    asset,
                    "order_count",
                    book.order_count()
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
            if key.len() == 13 && &key[..5] == b"book_" {
                let asset = u64::from_le_bytes(key[5..].try_into().unwrap());
                match serde_json::from_slice::<engine::OrderBook>(&value) {
                    Ok(book) => {
                        log::debug!("Loaded book", "asset", asset);
                        books.insert(asset, book);
                    }
                    Err(e) => log::error!(
                        "Failed to deserialize book",
                        "asset",
                        asset,
                        "err",
                        e.to_string()
                    ),
                }
            }
        }
        log::info!(
            "Loaded books from DB",
            "count",
            books.len(),
            "took",
            log::duration_secs(&start.elapsed())
        );
        Ok(books)
    }
}

// ── Encrypted Key Store (wraps SecretStore with AEAD) ──

pub struct EncryptedStore {
    inner: SecretStore,
}

impl EncryptedStore {
    pub fn open(db: &sled::Db, _dek: [u8; 32]) -> Self {
        Self {
            inner: SecretStore::open(db).expect("SecretStore open"),
        }
    }

    pub fn insert(&self, cmt: &str, secrets: &OrderSecrets) -> anyhow::Result<()> {
        #[cfg(feature = "secure")]
        {
            let plaintext = serde_json::to_vec(secrets)?;
            let enc = crate::crypto::encrypt(&dek_from_env()?, &plaintext)?;
            let mut buf = enc.nonce.to_vec();
            buf.extend_from_slice(&enc.ciphertext);
            self.inner.tree.insert(cmt.as_bytes(), buf)?;
            self.inner.tree.flush()?;
        }
        #[cfg(not(feature = "secure"))]
        {
            self.inner.insert(cmt, secrets)?;
        }
        Ok(())
    }

    pub fn get(&self, cmt: &str) -> anyhow::Result<Option<OrderSecrets>> {
        #[cfg(feature = "secure")]
        {
            match self.inner.tree.get(cmt.as_bytes())? {
                Some(value) => {
                    if value.len() < 12 {
                        return Ok(None);
                    }
                    let mut nonce = [0u8; 12];
                    nonce.copy_from_slice(&value[..12]);
                    let payload = crate::crypto::EncryptedPayload {
                        nonce,
                        ciphertext: value[12..].to_vec(),
                    };
                    let plaintext = crate::crypto::decrypt(&dek_from_env()?, &payload)?;
                    let secrets: OrderSecrets = serde_json::from_slice(&plaintext)?;
                    Ok(Some(secrets))
                }
                None => Ok(None),
            }
        }
        #[cfg(not(feature = "secure"))]
        {
            self.inner.get(cmt)
        }
    }
}

#[cfg(feature = "secure")]
fn dek_from_env() -> anyhow::Result<[u8; 32]> {
    use std::env;
    let hex_key = env::var("CER_DEK").map_err(|_| anyhow::anyhow!("CER_DEK not set"))?;
    let bytes = hex::decode(&hex_key)?;
    if bytes.len() != 32 {
        anyhow::bail!("CER_DEK must be 64 hex chars");
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    Ok(key)
}
