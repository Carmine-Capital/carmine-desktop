use bytes::Bytes;
use cloudmount_core::types::*;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, RANGE};
use reqwest::{Client, StatusCode};

use crate::retry::with_retry;

const GRAPH_BASE: &str = "https://graph.microsoft.com/v1.0";
const UPLOAD_CHUNK_SIZE: usize = 10 * 1024 * 1024;
const SMALL_FILE_LIMIT: usize = 4 * 1024 * 1024;

pub struct GraphClient {
    http: Client,
    base_url: String,
    token_fn: Box<
        dyn Fn() -> std::pin::Pin<
                Box<dyn std::future::Future<Output = cloudmount_core::Result<String>> + Send>,
            > + Send
            + Sync,
    >,
}

impl GraphClient {
    pub fn new<F, Fut>(token_fn: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = cloudmount_core::Result<String>> + Send + 'static,
    {
        Self {
            http: Client::new(),
            base_url: GRAPH_BASE.to_string(),
            token_fn: Box::new(move || Box::pin(token_fn())),
        }
    }

    pub fn with_base_url<F, Fut>(base_url: String, token_fn: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = cloudmount_core::Result<String>> + Send + 'static,
    {
        Self {
            http: Client::new(),
            base_url,
            token_fn: Box::new(move || Box::pin(token_fn())),
        }
    }

    async fn token(&self) -> cloudmount_core::Result<String> {
        (self.token_fn)().await
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
    ) -> cloudmount_core::Result<T> {
        with_retry(|| async {
            let token = self.token().await?;
            let resp = self
                .http
                .get(url)
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .send()
                .await
                .map_err(|e| cloudmount_core::Error::Network(e.to_string()))?;

            Self::handle_error(resp)
                .await?
                .json::<T>()
                .await
                .map_err(|e| cloudmount_core::Error::GraphApi {
                    status: 0,
                    message: format!("deserialization failed: {e}"),
                })
        })
        .await
    }

    async fn handle_error(resp: reqwest::Response) -> cloudmount_core::Result<reqwest::Response> {
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }

        if status == StatusCode::TOO_MANY_REQUESTS {
            let retry_after = resp
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(5);
            tokio::time::sleep(tokio::time::Duration::from_secs(retry_after)).await;
            return Err(cloudmount_core::Error::GraphApi {
                status: 429,
                message: format!("rate limited, retry after {retry_after}s"),
            });
        }

        let body = resp.text().await.unwrap_or_default();
        if let Ok(err) = serde_json::from_str::<GraphErrorResponse>(&body) {
            return Err(cloudmount_core::Error::GraphApi {
                status: status.as_u16(),
                message: format!("{}: {}", err.error.code, err.error.message),
            });
        }

        Err(cloudmount_core::Error::GraphApi {
            status: status.as_u16(),
            message: body,
        })
    }

    pub async fn get_my_drive(&self) -> cloudmount_core::Result<Drive> {
        let base_url = &self.base_url;
        self.get_json(&format!("{base_url}/me/drive")).await
    }

    pub async fn list_children(
        &self,
        drive_id: &str,
        item_id: &str,
    ) -> cloudmount_core::Result<Vec<DriveItem>> {
        let base_url = &self.base_url;
        let mut items = Vec::new();
        let mut url = format!("{base_url}/drives/{drive_id}/items/{item_id}/children?$top=200");

        loop {
            let page: GraphCollection<DriveItem> = self.get_json(&url).await?;
            items.extend(page.value);
            match page.next_link {
                Some(next) => url = next,
                None => break,
            }
        }

        Ok(items)
    }

    pub async fn list_root_children(
        &self,
        drive_id: &str,
    ) -> cloudmount_core::Result<Vec<DriveItem>> {
        let base_url = &self.base_url;
        let mut items = Vec::new();
        let mut url = format!("{base_url}/drives/{drive_id}/root/children?$top=200");

        loop {
            let page: GraphCollection<DriveItem> = self.get_json(&url).await?;
            items.extend(page.value);
            match page.next_link {
                Some(next) => url = next,
                None => break,
            }
        }

        Ok(items)
    }

    pub async fn get_item(
        &self,
        drive_id: &str,
        item_id: &str,
    ) -> cloudmount_core::Result<DriveItem> {
        let base_url = &self.base_url;
        self.get_json(&format!("{base_url}/drives/{drive_id}/items/{item_id}"))
            .await
    }

    pub async fn download_content(
        &self,
        drive_id: &str,
        item_id: &str,
    ) -> cloudmount_core::Result<Bytes> {
        let base_url = &self.base_url;
        with_retry(|| async {
            let token = self.token().await?;
            let resp = self
                .http
                .get(format!(
                    "{base_url}/drives/{drive_id}/items/{item_id}/content"
                ))
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .send()
                .await
                .map_err(|e| cloudmount_core::Error::Network(e.to_string()))?;

            Self::handle_error(resp)
                .await?
                .bytes()
                .await
                .map_err(|e| cloudmount_core::Error::Network(e.to_string()))
        })
        .await
    }

    pub async fn download_range(
        &self,
        drive_id: &str,
        item_id: &str,
        offset: u64,
        length: u64,
    ) -> cloudmount_core::Result<Bytes> {
        let base_url = &self.base_url;
        with_retry(|| async {
            let token = self.token().await?;
            let range_header = format!("bytes={}-{}", offset, offset + length - 1);
            let resp = self
                .http
                .get(format!(
                    "{base_url}/drives/{drive_id}/items/{item_id}/content"
                ))
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .header(RANGE, &range_header)
                .send()
                .await
                .map_err(|e| cloudmount_core::Error::Network(e.to_string()))?;

            Self::handle_error(resp)
                .await?
                .bytes()
                .await
                .map_err(|e| cloudmount_core::Error::Network(e.to_string()))
        })
        .await
    }

    pub async fn upload_small(
        &self,
        drive_id: &str,
        parent_id: &str,
        name: &str,
        content: Bytes,
    ) -> cloudmount_core::Result<DriveItem> {
        let base_url = &self.base_url;
        let token = self.token().await?;
        let url = format!("{base_url}/drives/{drive_id}/items/{parent_id}:/{name}:/content");
        let resp = self
            .http
            .put(&url)
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(content)
            .send()
            .await
            .map_err(|e| cloudmount_core::Error::Network(e.to_string()))?;

        Self::handle_error(resp)
            .await?
            .json()
            .await
            .map_err(|e| cloudmount_core::Error::GraphApi {
                status: 0,
                message: format!("upload response parse failed: {e}"),
            })
    }

    pub async fn create_upload_session(
        &self,
        drive_id: &str,
        item_id: &str,
    ) -> cloudmount_core::Result<UploadSession> {
        let base_url = &self.base_url;
        let token = self.token().await?;
        let url = format!("{base_url}/drives/{drive_id}/items/{item_id}/createUploadSession");
        let resp = self
            .http
            .post(&url)
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(CONTENT_TYPE, "application/json")
            .body("{}")
            .send()
            .await
            .map_err(|e| cloudmount_core::Error::Network(e.to_string()))?;

        Self::handle_error(resp)
            .await?
            .json()
            .await
            .map_err(|e| cloudmount_core::Error::GraphApi {
                status: 0,
                message: format!("upload session parse failed: {e}"),
            })
    }

    pub async fn upload_large(
        &self,
        drive_id: &str,
        item_id: &str,
        content: Bytes,
    ) -> cloudmount_core::Result<DriveItem> {
        let session = self.create_upload_session(drive_id, item_id).await?;
        let total = content.len();
        let mut offset = 0;

        while offset < total {
            let end = std::cmp::min(offset + UPLOAD_CHUNK_SIZE, total);
            let chunk = content.slice(offset..end);
            let content_range = format!("bytes {offset}-{}/{total}", end - 1);

            let resp = self
                .http
                .put(&session.upload_url)
                .header("Content-Range", &content_range)
                .header(CONTENT_TYPE, "application/octet-stream")
                .body(chunk)
                .send()
                .await
                .map_err(|e| cloudmount_core::Error::Network(e.to_string()))?;

            if end == total {
                return Self::handle_error(resp).await?.json().await.map_err(|e| {
                    cloudmount_core::Error::GraphApi {
                        status: 0,
                        message: format!("final chunk response parse failed: {e}"),
                    }
                });
            }

            Self::handle_error(resp).await?;
            offset = end;
        }

        Err(cloudmount_core::Error::GraphApi {
            status: 0,
            message: "upload completed but no item returned".into(),
        })
    }

    pub async fn upload(
        &self,
        drive_id: &str,
        parent_id: &str,
        item_id: Option<&str>,
        name: &str,
        content: Bytes,
    ) -> cloudmount_core::Result<DriveItem> {
        if content.len() < SMALL_FILE_LIMIT {
            self.upload_small(drive_id, parent_id, name, content).await
        } else {
            let id = item_id.ok_or_else(|| cloudmount_core::Error::GraphApi {
                status: 0,
                message: "item_id required for large file upload".into(),
            })?;
            self.upload_large(drive_id, id, content).await
        }
    }

    pub async fn create_folder(
        &self,
        drive_id: &str,
        parent_id: &str,
        name: &str,
    ) -> cloudmount_core::Result<DriveItem> {
        let base_url = &self.base_url;
        let token = self.token().await?;
        let url = format!("{base_url}/drives/{drive_id}/items/{parent_id}/children");
        let body = serde_json::json!({
            "name": name,
            "folder": {},
            "@microsoft.graph.conflictBehavior": "fail"
        });

        let resp = self
            .http
            .post(&url)
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| cloudmount_core::Error::Network(e.to_string()))?;

        Self::handle_error(resp)
            .await?
            .json()
            .await
            .map_err(|e| cloudmount_core::Error::GraphApi {
                status: 0,
                message: format!("create folder response parse failed: {e}"),
            })
    }

    pub async fn delete_item(&self, drive_id: &str, item_id: &str) -> cloudmount_core::Result<()> {
        let base_url = &self.base_url;
        let token = self.token().await?;
        let url = format!("{base_url}/drives/{drive_id}/items/{item_id}");
        let resp = self
            .http
            .delete(&url)
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .send()
            .await
            .map_err(|e| cloudmount_core::Error::Network(e.to_string()))?;

        let status = resp.status();
        if status == StatusCode::NO_CONTENT || status.is_success() {
            Ok(())
        } else {
            Self::handle_error(resp).await.map(|_| ())
        }
    }

    pub async fn update_item(
        &self,
        drive_id: &str,
        item_id: &str,
        new_name: Option<&str>,
        new_parent_id: Option<&str>,
    ) -> cloudmount_core::Result<DriveItem> {
        let base_url = &self.base_url;
        let token = self.token().await?;
        let url = format!("{base_url}/drives/{drive_id}/items/{item_id}");

        let mut body = serde_json::Map::new();
        if let Some(name) = new_name {
            body.insert("name".into(), serde_json::Value::String(name.to_string()));
        }
        if let Some(parent_id) = new_parent_id {
            body.insert(
                "parentReference".into(),
                serde_json::json!({"id": parent_id}),
            );
        }

        let resp = self
            .http
            .patch(&url)
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| cloudmount_core::Error::Network(e.to_string()))?;

        Self::handle_error(resp)
            .await?
            .json()
            .await
            .map_err(|e| cloudmount_core::Error::GraphApi {
                status: 0,
                message: format!("update item response parse failed: {e}"),
            })
    }

    pub async fn delta_query(
        &self,
        drive_id: &str,
        delta_token: Option<&str>,
    ) -> cloudmount_core::Result<DeltaResponse> {
        let base_url = &self.base_url;
        let url = match delta_token {
            Some(token) => token.to_string(),
            None => format!("{base_url}/drives/{drive_id}/root/delta"),
        };

        let mut all_items = Vec::new();
        let mut current_url = url;
        let final_delta_link;

        loop {
            let result: std::result::Result<DeltaResponse, _> = self.get_json(&current_url).await;

            match result {
                Ok(page) => {
                    all_items.extend(page.value);
                    if let Some(next) = page.next_link {
                        current_url = next;
                    } else {
                        final_delta_link = page.delta_link;
                        break;
                    }
                }
                Err(cloudmount_core::Error::GraphApi { status: 410, .. }) => {
                    tracing::warn!("delta token expired (410 Gone), performing full sync");
                    current_url = format!("{base_url}/drives/{drive_id}/root/delta");
                    all_items.clear();
                }
                Err(e) => return Err(e),
            }
        }

        Ok(DeltaResponse {
            value: all_items,
            delta_link: final_delta_link,
            next_link: None,
        })
    }

    pub async fn search_sites(&self, query: &str) -> cloudmount_core::Result<Vec<Site>> {
        let base_url = &self.base_url;
        let url = format!("{base_url}/sites?search={}", urlencoding::encode(query));
        let collection: GraphCollection<Site> = self.get_json(&url).await?;
        Ok(collection.value)
    }

    pub async fn get_followed_sites(&self) -> cloudmount_core::Result<Vec<Site>> {
        let base_url = &self.base_url;
        let collection: GraphCollection<Site> = self
            .get_json(&format!("{base_url}/me/followedSites"))
            .await?;
        Ok(collection.value)
    }

    pub async fn browse_library_folder(
        &self,
        drive_id: &str,
        folder_id: Option<&str>,
    ) -> cloudmount_core::Result<Vec<DriveItem>> {
        let children = match folder_id {
            Some(id) => self.list_children(drive_id, id).await?,
            None => self.list_root_children(drive_id).await?,
        };
        Ok(children
            .into_iter()
            .filter(|item| item.folder.is_some())
            .collect())
    }

    pub async fn list_site_drives(&self, site_id: &str) -> cloudmount_core::Result<Vec<Drive>> {
        let base_url = &self.base_url;
        let collection: GraphCollection<Drive> = self
            .get_json(&format!("{base_url}/sites/{site_id}/drives"))
            .await?;
        Ok(collection
            .value
            .into_iter()
            .filter(|d| d.drive_type.as_deref() == Some("documentLibrary"))
            .collect())
    }
}
