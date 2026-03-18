use carminedesktop_core::types::{
    ActivityEntry, CacheManagerStats, CacheStatsResponse, DashboardError, DashboardStatus,
    DriveStatus, ObsEvent, PinHealthInfo, UploadQueueInfo, WritebackEntry,
};

#[test]
fn test_obs_event_error_serializes_with_type_error() {
    let event = ObsEvent::Error {
        drive_id: Some("drive-1".into()),
        file_name: Some("report.xlsx".into()),
        remote_path: Some("/Documents/report.xlsx".into()),
        error_type: "upload_failed".into(),
        message: "Upload failed for report.xlsx".into(),
        action_hint: Some("Upload failed -- check file permissions and size".into()),
        timestamp: "2026-03-18T12:00:00Z".into(),
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "error");
    assert_eq!(json["driveId"], "drive-1");
    assert_eq!(json["fileName"], "report.xlsx");
    assert_eq!(json["remotePath"], "/Documents/report.xlsx");
    assert_eq!(json["errorType"], "upload_failed");
    assert_eq!(json["message"], "Upload failed for report.xlsx");
    assert_eq!(
        json["actionHint"],
        "Upload failed -- check file permissions and size"
    );
    assert_eq!(json["timestamp"], "2026-03-18T12:00:00Z");
}

#[test]
fn test_obs_event_activity_serializes_with_type_activity() {
    let event = ObsEvent::Activity {
        drive_id: "drive-1".into(),
        file_path: "/Documents/Reports/Q4.xlsx".into(),
        activity_type: "synced".into(),
        timestamp: "2026-03-18T12:00:00Z".into(),
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "activity");
    assert_eq!(json["driveId"], "drive-1");
    assert_eq!(json["filePath"], "/Documents/Reports/Q4.xlsx");
    assert_eq!(json["activityType"], "synced");
    assert_eq!(json["timestamp"], "2026-03-18T12:00:00Z");
}

#[test]
fn test_obs_event_sync_state_changed_serializes() {
    let event = ObsEvent::SyncStateChanged {
        drive_id: "drive-2".into(),
        state: "syncing".into(),
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "syncStateChanged");
    assert_eq!(json["driveId"], "drive-2");
    assert_eq!(json["state"], "syncing");
}

#[test]
fn test_obs_event_online_state_changed_serializes() {
    let event = ObsEvent::OnlineStateChanged {
        drive_id: "drive-3".into(),
        online: false,
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "onlineStateChanged");
    assert_eq!(json["driveId"], "drive-3");
    assert_eq!(json["online"], false);
}

#[test]
fn test_obs_event_auth_state_changed_serializes() {
    let event = ObsEvent::AuthStateChanged { degraded: true };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "authStateChanged");
    assert_eq!(json["degraded"], true);
}

#[test]
fn test_dashboard_status_serializes_camel_case() {
    let status = DashboardStatus {
        drives: vec![DriveStatus {
            drive_id: "drive-1".into(),
            name: "My Drive".into(),
            mount_point: "/mnt/onedrive".into(),
            online: true,
            last_synced: Some("2026-03-18T12:00:00Z".into()),
            sync_state: "up_to_date".into(),
            upload_queue: UploadQueueInfo {
                queue_depth: 0,
                in_flight: 0,
                failed_count: 0,
                total_uploaded: 42,
                total_failed: 1,
            },
        }],
        authenticated: true,
        auth_degraded: false,
    };
    let json = serde_json::to_value(&status).unwrap();
    assert_eq!(json["authenticated"], true);
    assert_eq!(json["authDegraded"], false);

    let drive = &json["drives"][0];
    assert_eq!(drive["driveId"], "drive-1");
    assert_eq!(drive["name"], "My Drive");
    assert_eq!(drive["mountPoint"], "/mnt/onedrive");
    assert_eq!(drive["online"], true);
    assert_eq!(drive["lastSynced"], "2026-03-18T12:00:00Z");
    assert_eq!(drive["syncState"], "up_to_date");

    let queue = &drive["uploadQueue"];
    assert_eq!(queue["queueDepth"], 0);
    assert_eq!(queue["inFlight"], 0);
    assert_eq!(queue["failedCount"], 0);
    assert_eq!(queue["totalUploaded"], 42);
    assert_eq!(queue["totalFailed"], 1);
}

#[test]
fn test_dashboard_error_serializes_camel_case() {
    let err = DashboardError {
        drive_id: Some("drive-1".into()),
        file_name: Some("Q4-Report.xlsx".into()),
        remote_path: Some("/Documents/Reports/Q4-Report.xlsx".into()),
        error_type: "upload_failed".into(),
        message: "Upload failed for Q4-Report.xlsx".into(),
        action_hint: Some("Upload failed -- check file permissions and size".into()),
        timestamp: "2026-03-18T12:05:30Z".into(),
    };
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["driveId"], "drive-1");
    assert_eq!(json["fileName"], "Q4-Report.xlsx");
    assert_eq!(json["remotePath"], "/Documents/Reports/Q4-Report.xlsx");
    assert_eq!(json["errorType"], "upload_failed");
    assert_eq!(json["message"], "Upload failed for Q4-Report.xlsx");
    assert_eq!(
        json["actionHint"],
        "Upload failed -- check file permissions and size"
    );
    assert_eq!(json["timestamp"], "2026-03-18T12:05:30Z");
}

#[test]
fn test_cache_stats_response_serializes_camel_case() {
    let stats = CacheStatsResponse {
        disk_used_bytes: 2_200_000_000,
        disk_max_bytes: 5_368_709_120,
        memory_entry_count: 1523,
        pinned_items: vec![PinHealthInfo {
            drive_id: "drive-1".into(),
            item_id: "item-abc".into(),
            folder_name: "Reports".into(),
            status: "downloaded".into(),
            total_files: 52,
            cached_files: 52,
            pinned_at: "2026-03-18T10:00:00Z".into(),
            expires_at: "2026-03-25T10:00:00Z".into(),
        }],
        writeback_queue: vec![WritebackEntry {
            drive_id: "drive-1".into(),
            item_id: "item-xyz".into(),
            file_name: Some("draft.docx".into()),
        }],
    };
    let json = serde_json::to_value(&stats).unwrap();
    assert_eq!(json["diskUsedBytes"], 2_200_000_000u64);
    assert_eq!(json["diskMaxBytes"], 5_368_709_120u64);
    assert_eq!(json["memoryEntryCount"], 1523);

    let pin = &json["pinnedItems"][0];
    assert_eq!(pin["driveId"], "drive-1");
    assert_eq!(pin["itemId"], "item-abc");
    assert_eq!(pin["folderName"], "Reports");
    assert_eq!(pin["status"], "downloaded");
    assert_eq!(pin["totalFiles"], 52);
    assert_eq!(pin["cachedFiles"], 52);
    assert_eq!(pin["pinnedAt"], "2026-03-18T10:00:00Z");
    assert_eq!(pin["expiresAt"], "2026-03-25T10:00:00Z");

    let wb = &json["writebackQueue"][0];
    assert_eq!(wb["driveId"], "drive-1");
    assert_eq!(wb["itemId"], "item-xyz");
    assert_eq!(wb["fileName"], "draft.docx");
}

#[test]
fn test_activity_entry_serializes_camel_case() {
    let entry = ActivityEntry {
        drive_id: "drive-1".into(),
        file_path: "/Documents/Reports/Q4.xlsx".into(),
        activity_type: "uploaded".into(),
        timestamp: "2026-03-18T12:00:00Z".into(),
    };
    let json = serde_json::to_value(&entry).unwrap();
    assert_eq!(json["driveId"], "drive-1");
    assert_eq!(json["filePath"], "/Documents/Reports/Q4.xlsx");
    assert_eq!(json["activityType"], "uploaded");
    assert_eq!(json["timestamp"], "2026-03-18T12:00:00Z");
}

#[test]
fn test_cache_manager_stats_fields() {
    let stats = CacheManagerStats {
        memory_entry_count: 100,
        disk_used_bytes: 5_000_000,
        disk_max_bytes: 10_000_000,
        dirty_inode_count: 3,
    };
    assert_eq!(stats.memory_entry_count, 100);
    assert_eq!(stats.disk_used_bytes, 5_000_000);
    assert_eq!(stats.disk_max_bytes, 10_000_000);
    assert_eq!(stats.dirty_inode_count, 3);
}
