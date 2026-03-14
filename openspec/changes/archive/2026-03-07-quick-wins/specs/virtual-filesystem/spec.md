## CHANGED Requirements

### Requirement: FUSE mount options for performance (Linux/macOS)

The FUSE mount must configure kernel-level performance options to maximize I/O throughput.

#### Scenario: Mount includes max_read option

- **WHEN** carminedesktopFs mounts on Linux or macOS
- **THEN** the mount options include `max_read=1048576` (1MB)
- **AND** the kernel sends read requests up to 1MB instead of the default 128KB

#### Scenario: init() enables writeback cache capability

- **WHEN** the FUSE `init()` callback is invoked
- **THEN** the filesystem requests `FUSE_CAP_WRITEBACK_CACHE` via `KernelConfig::add_capabilities()`
- **AND** if the kernel supports it, writes are coalesced before reaching userspace
- **AND** if the kernel does not support it, the mount proceeds without it (graceful degradation)

#### Scenario: init() enables parallel directory operations

- **WHEN** the FUSE `init()` callback is invoked
- **THEN** the filesystem requests `FUSE_CAP_PARALLEL_DIROPS` via `KernelConfig::add_capabilities()`
- **AND** if the kernel supports it, directory operations are not serialized

#### Scenario: NoAtime reduces unnecessary metadata updates

- **WHEN** carminedesktopFs mounts
- **THEN** the mount options include `NoAtime`
- **AND** file access times are not updated on read
