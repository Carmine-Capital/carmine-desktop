//! Windows named pipe IPC server for Explorer context menu integration.
//!
//! Receives pin/unpin requests from the `--offline-pin` / `--offline-unpin`
//! CLI arguments when they connect to the named pipe as a fallback to the
//! single-instance plugin.

#![cfg(target_os = "windows")]

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

const PIPE_NAME: &str = r"\\.\pipe\CarmineDesktop";
const MAX_MESSAGE_SIZE: usize = 65_536; // 64 KB

#[derive(Deserialize)]
struct IpcRequest {
    action: String,
    path: String,
}

#[derive(Serialize)]
struct IpcResponse {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

pub struct IpcServer {
    cancel: CancellationToken,
}

impl IpcServer {
    /// Start the named pipe server. Returns a handle that can be used to stop it.
    pub fn start(app: tauri::AppHandle) -> Self {
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        tauri::async_runtime::spawn(async move {
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
            use tokio::net::windows::named_pipe::ServerOptions;

            loop {
                let server = match ServerOptions::new()
                    .first_pipe_instance(false)
                    .create(PIPE_NAME)
                {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!("failed to create named pipe: {e}");
                        return;
                    }
                };

                tokio::select! {
                    _ = cancel_clone.cancelled() => break,
                    result = server.connect() => {
                        match result {
                            Ok(()) => {
                                let app = app.clone();
                                tokio::spawn(async move {
                                    let mut reader = BufReader::new(&server);
                                    let mut line = String::new();

                                    // Read with timeout
                                    let read_result = tokio::time::timeout(
                                        std::time::Duration::from_secs(5),
                                        reader.read_line(&mut line),
                                    ).await;

                                    // Drop reader to release the shared borrow's buffer
                                    drop(reader);

                                    let response = match read_result {
                                        Ok(Ok(n)) if n > 0 && n <= MAX_MESSAGE_SIZE => {
                                            handle_ipc_request(&app, &line).await
                                        }
                                        Ok(Ok(n)) if n > MAX_MESSAGE_SIZE => {
                                            IpcResponse {
                                                status: "error".to_string(),
                                                message: Some("message too large".to_string()),
                                            }
                                        }
                                        _ => {
                                            IpcResponse {
                                                status: "error".to_string(),
                                                message: Some("read failed or timed out".to_string()),
                                            }
                                        }
                                    };

                                    if let Ok(json) = serde_json::to_string(&response) {
                                        let mut w = &server;
                                        let _ = w.write_all(json.as_bytes()).await;
                                        let _ = w.write_all(b"\n").await;
                                    }
                                });
                            }
                            Err(e) => {
                                tracing::warn!("named pipe connect failed: {e}");
                            }
                        }
                    }
                }
            }
        });

        Self { cancel }
    }

    /// Stop the server.
    pub fn stop(&self) {
        self.cancel.cancel();
    }
}

async fn handle_ipc_request(app: &tauri::AppHandle, line: &str) -> IpcResponse {
    let request: IpcRequest = match serde_json::from_str(line.trim()) {
        Ok(r) => r,
        Err(e) => {
            return IpcResponse {
                status: "error".to_string(),
                message: Some(format!("invalid JSON: {e}")),
            };
        }
    };

    match request.action.as_str() {
        "pin" => match super::handle_offline_pin(app, &request.path).await {
            Ok(_) => IpcResponse {
                status: "ok".to_string(),
                message: None,
            },
            Err(e) => IpcResponse {
                status: "error".to_string(),
                message: Some(e),
            },
        },
        "unpin" => match super::handle_offline_unpin(app, &request.path).await {
            Ok(_) => IpcResponse {
                status: "ok".to_string(),
                message: None,
            },
            Err(e) => IpcResponse {
                status: "error".to_string(),
                message: Some(e),
            },
        },
        _ => IpcResponse {
            status: "error".to_string(),
            message: Some(format!("unknown action: {}", request.action)),
        },
    }
}
