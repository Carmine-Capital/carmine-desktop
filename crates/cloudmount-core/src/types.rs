use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveItem {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub size: i64,
    #[serde(rename = "lastModifiedDateTime")]
    pub last_modified: Option<DateTime<Utc>>,
    #[serde(rename = "createdDateTime")]
    pub created: Option<DateTime<Utc>>,
    #[serde(rename = "eTag")]
    pub etag: Option<String>,
    #[serde(rename = "parentReference")]
    pub parent_reference: Option<ParentReference>,
    pub folder: Option<FolderFacet>,
    pub file: Option<FileFacet>,
    #[serde(rename = "@microsoft.graph.downloadUrl")]
    pub download_url: Option<String>,
}

impl DriveItem {
    pub fn is_folder(&self) -> bool {
        self.folder.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParentReference {
    #[serde(rename = "driveId")]
    pub drive_id: Option<String>,
    pub id: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderFacet {
    #[serde(rename = "childCount", default)]
    pub child_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileFacet {
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
    pub hashes: Option<FileHashes>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHashes {
    #[serde(rename = "sha256Hash")]
    pub sha256: Option<String>,
    #[serde(rename = "quickXorHash")]
    pub quick_xor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Drive {
    pub id: String,
    pub name: String,
    #[serde(rename = "driveType")]
    pub drive_type: Option<String>,
    pub owner: Option<serde_json::Value>,
    pub quota: Option<DriveQuota>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveQuota {
    pub total: Option<i64>,
    pub used: Option<i64>,
    pub remaining: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(rename = "webUrl")]
    pub web_url: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaResponse {
    pub value: Vec<DriveItem>,
    #[serde(rename = "@odata.deltaLink")]
    pub delta_link: Option<String>,
    #[serde(rename = "@odata.nextLink")]
    pub next_link: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadSession {
    #[serde(rename = "uploadUrl")]
    pub upload_url: String,
    #[serde(rename = "expirationDateTime")]
    pub expiration: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphErrorResponse {
    pub error: GraphErrorBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphErrorBody {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphCollection<T> {
    pub value: Vec<T>,
    #[serde(rename = "@odata.nextLink")]
    pub next_link: Option<String>,
}
