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
        self.pending_dir
            .join(drive_id)
            .join(Self::sanitize_filename(item_id))
    }

    /// Encode colons in item_id for use as a filename (colons are invalid on Windows).
    fn sanitize_filename(item_id: &str) -> String {
        item_id.replace(':', "%3A")
    }

    /// Reverse filename sanitization to recover the original item_id.
    fn unsanitize_filename(name: &str) -> String {
        name.replace("%3A", ":")
    }

    /// Store content in the in-memory buffer and persist to disk for crash safety.
    pub async fn write(
        &self,
        drive_id: &str,
        item_id: &str,
        content: &[u8],
    ) -> cloudmount_core::Result<()> {
        let key = Self::buffer_key(drive_id, item_id);
        self.buffers.insert(key, content.to_vec());
        // Persist to disk immediately for crash safety
        self.persist(drive_id, item_id).await?;
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
        let tmp_path = path.with_extension("tmp");
        fs::write(&tmp_path, &content)
            .await
            .map_err(|e| cloudmount_core::Error::Cache(format!("persist pending failed: {e}")))?;
        fs::rename(&tmp_path, &path)
            .await
            .map_err(|e| cloudmount_core::Error::Cache(format!("rename pending failed: {e}")))?;
        Ok(())
    }

    pub async fn remove(&self, drive_id: &str, item_id: &str) -> cloudmount_core::Result<()> {
        let key = Self::buffer_key(drive_id, item_id);
        self.buffers.remove(&key);
        let path = self.pending_path(drive_id, item_id);
        match fs::remove_file(&path).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(cloudmount_core::Error::Cache(format!(
                    "remove pending failed: {e}"
                )));
            }
        }
        Ok(())
    }

    pub async fn list_pending(&self) -> cloudmount_core::Result<Vec<(String, String)>> {
        let mut pending = Vec::new();

        let mut drive_dirs = match fs::read_dir(&self.pending_dir).await {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(pending),
            Err(e) => {
                return Err(cloudmount_core::Error::Cache(format!(
                    "read pending dir failed: {e}"
                )));
            }
        };

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
                let name = item_entry.file_name().to_string_lossy().to_string();
                // Skip leftover .tmp files from interrupted atomic writes
                if name.ends_with(".tmp") {
                    continue;
                }
                pending.push((drive_id.clone(), Self::unsanitize_filename(&name)));
            }
        }

        Ok(pending)
    }
}
