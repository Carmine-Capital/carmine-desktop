use std::path::PathBuf;

use tokio::fs;

pub struct WriteBackBuffer {
    pending_dir: PathBuf,
}

impl WriteBackBuffer {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            pending_dir: cache_dir.join("pending"),
        }
    }

    fn pending_path(&self, drive_id: &str, item_id: &str) -> PathBuf {
        self.pending_dir.join(drive_id).join(item_id)
    }

    pub async fn write(
        &self,
        drive_id: &str,
        item_id: &str,
        content: &[u8],
    ) -> cloudmount_core::Result<()> {
        let path = self.pending_path(drive_id, item_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| cloudmount_core::Error::Cache(format!("mkdir pending failed: {e}")))?;
        }
        fs::write(&path, content)
            .await
            .map_err(|e| cloudmount_core::Error::Cache(format!("write pending failed: {e}")))?;
        Ok(())
    }

    pub async fn read(&self, drive_id: &str, item_id: &str) -> Option<Vec<u8>> {
        let path = self.pending_path(drive_id, item_id);
        fs::read(&path).await.ok()
    }

    pub async fn remove(&self, drive_id: &str, item_id: &str) -> cloudmount_core::Result<()> {
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
