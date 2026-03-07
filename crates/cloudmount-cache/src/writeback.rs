use std::path::PathBuf;

use dashmap::DashMap;
use tokio::fs;

pub struct WriteBackBuffer {
    pending_dir: PathBuf,
    /// In-memory write buffers keyed by "{drive_id}\0{item_id}".
    /// Avoids disk round-trips on every FUSE write call.
    buffers: DashMap<String, Vec<u8>>,
}

impl WriteBackBuffer {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            pending_dir: cache_dir.join("pending"),
            buffers: DashMap::new(),
        }
    }

    fn buffer_key(drive_id: &str, item_id: &str) -> String {
        format!("{drive_id}\0{item_id}")
    }

    fn pending_path(&self, drive_id: &str, item_id: &str) -> PathBuf {
        self.pending_dir.join(drive_id).join(item_id)
    }

    /// Store content in the in-memory buffer (no disk I/O).
    pub async fn write(
        &self,
        drive_id: &str,
        item_id: &str,
        content: &[u8],
    ) -> cloudmount_core::Result<()> {
        let key = Self::buffer_key(drive_id, item_id);
        self.buffers.insert(key, content.to_vec());
        Ok(())
    }

    /// Read content from in-memory buffer first, falling back to disk.
    pub async fn read(&self, drive_id: &str, item_id: &str) -> Option<Vec<u8>> {
        let key = Self::buffer_key(drive_id, item_id);
        if let Some(buf) = self.buffers.get(&key) {
            return Some(buf.clone());
        }
        let path = self.pending_path(drive_id, item_id);
        fs::read(&path).await.ok()
    }

    /// Persist in-memory buffer to disk for crash safety. Call before uploading.
    pub async fn persist(&self, drive_id: &str, item_id: &str) -> cloudmount_core::Result<()> {
        let key = Self::buffer_key(drive_id, item_id);
        let content = match self.buffers.get(&key) {
            Some(buf) => buf.clone(),
            None => return Ok(()),
        };
        let path = self.pending_path(drive_id, item_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| cloudmount_core::Error::Cache(format!("mkdir pending failed: {e}")))?;
        }
        fs::write(&path, &content)
            .await
            .map_err(|e| cloudmount_core::Error::Cache(format!("persist pending failed: {e}")))?;
        Ok(())
    }

    pub async fn remove(&self, drive_id: &str, item_id: &str) -> cloudmount_core::Result<()> {
        let key = Self::buffer_key(drive_id, item_id);
        self.buffers.remove(&key);
        let path = self.pending_path(drive_id, item_id);
        if path.exists() {
            fs::remove_file(&path).await.map_err(|e| {
                cloudmount_core::Error::Cache(format!("remove pending failed: {e}"))
            })?;
        }
        Ok(())
    }

    pub async fn list_pending(&self) -> cloudmount_core::Result<Vec<(String, String)>> {
        let mut pending = Vec::new();
        if !self.pending_dir.exists() {
            return Ok(pending);
        }

        let mut drive_dirs = fs::read_dir(&self.pending_dir)
            .await
            .map_err(|e| cloudmount_core::Error::Cache(format!("read pending dir failed: {e}")))?;

        while let Some(drive_entry) = drive_dirs
            .next_entry()
            .await
            .map_err(|e| cloudmount_core::Error::Cache(format!("read drive entry failed: {e}")))?
        {
            let drive_id = drive_entry.file_name().to_string_lossy().to_string();
            let mut item_files = fs::read_dir(drive_entry.path())
                .await
                .map_err(|e| cloudmount_core::Error::Cache(format!("read item dir failed: {e}")))?;

            while let Some(item_entry) = item_files.next_entry().await.map_err(|e| {
                cloudmount_core::Error::Cache(format!("read item entry failed: {e}"))
            })? {
                let item_id = item_entry.file_name().to_string_lossy().to_string();
                pending.push((drive_id.clone(), item_id));
            }
        }

        Ok(pending)
    }
}
