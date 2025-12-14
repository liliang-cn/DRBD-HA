# Fix Report: Remote Configuration Sync Failure

## The Bug
The `drbd-reactor` configuration files (TOML) and systemd service override files were not being copied to remote nodes. The logs showed `Permission denied` errors when trying to move files into `/etc/systemd/system/`.

## The Cause
The failure was caused by a missing privilege definition in the `ssh-cmd` library.

1.  **Directory Creation:** The sync process attempts to create target directories (e.g., `/etc/systemd/system/mysql.service.d/`) using `mkdir -p`.
2.  **Missing Privilege:** The `mkdir` command was **not** listed in `PRIVILEGED_COMMANDS` in `ssh-cmd`.
3.  **Execution Failure:** Because it wasn't marked as privileged, the SSH manager executed `mkdir` as the regular user (e.g., `liliang`) instead of `root` (via `sudo`). This failed due to lack of permissions on system directories.
4.  **Cascade Failure:** Although `mv` *was* privileged, it failed because the target directory did not exist (or wasn't writable), leading to the `Permission denied` error observed in the logs.

## The Fix
I have updated `ssh-cmd/src/lib.rs` to include `"mkdir"` in the `PRIVILEGED_COMMANDS` list.

```rust
const PRIVILEGED_COMMANDS: &[&str] = &[
    // ...
    "mv",    // Moving files in /etc requires sudo
    "mkdir", // Creating directories in /etc requires sudo  <-- Added
    "cp",    // Copying files in /etc requires sudo
    // ...
];
```

This ensures `mkdir` is executed with `sudo`, successfully creating the necessary directories on remote nodes, allowing the subsequent configuration file placement to succeed.

## Note on `events2` Errors
The logs you provided also show:
`events2: unrecognized option '--full'`

This is an environment issue unrelated to the code:
*   Your `drbd-reactor` version is trying to use a feature (`events2 --full`) that your installed `drbd-utils`/kernel module does not support.
*   **Action:** You may need to upgrade `drbd-utils` or align the versions of `drbd-reactor` and `drbd-utils` on your system.
