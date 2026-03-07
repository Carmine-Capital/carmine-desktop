## CHANGED Requirements

### Requirement: Field selection on Graph API list queries

List queries must request only the fields used by DriveItem deserialization, reducing JSON payload size.

#### Scenario: list_children includes $select parameter

- **WHEN** `list_children(drive_id, item_id)` is called
- **THEN** the request URL includes `$select=id,name,size,lastModifiedDateTime,createdDateTime,eTag,parentReference,folder,file,@microsoft.graph.downloadUrl`
- **AND** the response is smaller due to excluded unused fields
- **AND** DriveItem deserialization continues to work (serde ignores missing optional fields)

#### Scenario: list_root_children includes $select parameter

- **WHEN** `list_root_children(drive_id)` is called
- **THEN** the request URL includes the same `$select` parameter
- **AND** pagination via `@odata.nextLink` continues to work (server preserves $select across pages)

#### Scenario: delta_query is NOT modified

- **WHEN** `delta_query` is called
- **THEN** no `$select` parameter is added
- **AND** because delta responses include `deleted` facets and other fields needed for sync processing
