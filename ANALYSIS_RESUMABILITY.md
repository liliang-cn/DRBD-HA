# Analysis: Persistent Operations and Resumability

## Current State
Currently, the system uses an ephemeral `operation_id` for tracking progress.
- **Backend:** Generates a random UUID for each operation (`create_profile`, `delete_profile`) and broadcasts progress via SSE. This state is not persisted in the database.
- **Frontend:** Listens for events matching the resource name it initiated. This state is held in React memory (`useState`).

## Limitation
If the user refreshes the page or closes the browser during a long-running operation (like data migration or deletion):
1.  **Frontend Loss:** The UI loses the `operation_id` and the context of the running task.
2.  **Disconnect:** Upon reconnection, the frontend doesn't know what to listen for. The operation continues in the background, but the user sees no feedback and might assume it failed or finished.

## Proposed Solution: Persistent `DeploymentID` / `Operation`

To enable **Resumability** (restoring progress bars after reload), we need to persist the operation state.

### 1. Database Schema
Introduce an `operations` table:
```sql
CREATE TABLE operations (
    id TEXT PRIMARY KEY,          -- The deployment_id / operation_id
    type TEXT NOT NULL,           -- e.g., "create_ha_profile", "delete_ha_profile"
    resource_id TEXT,             -- Target resource ID
    status TEXT NOT NULL,         -- "running", "completed", "failed"
    progress INTEGER DEFAULT 0,   -- 0-100
    message TEXT,                 -- Last status message
    logs TEXT,                    -- JSON array of log history
    started_at DATETIME,
    updated_at DATETIME
);
```

### 2. Backend Changes
- **Start:** When an API (e.g., `create_profile`) is called, create a record in `operations`.
- **Update:** Inside `send_progress`, update the corresponding DB record's `progress`, `message`, and `logs`.
- **API:** Add endpoints to fetch active operations:
    - `GET /api/v1/operations` (List all running operations)
    - `GET /api/v1/operations/:id` (Get details for a specific operation)

### 3. Frontend Changes
- **Initialization:** On app load (`App.tsx` or Store initialization), fetch `GET /api/v1/operations?status=running`.
- **Restoration:** If a running operation matches a known resource, restore the "Progress Modal" or "Wizard Step" and re-subscribe to SSE events for that `id`.
- **UX:** Add a global "Background Tasks" indicator (e.g., a spinner in the header) that lists active operations.

## Benefits
1.  **Resilience:** Users can refresh or switch devices without losing track of long-running tasks.
2.  **Audit:** The `operations` table serves as a history log of who did what and when.
3.  **Concurrency:** Multiple users viewing the dashboard will see the same "Deleting..." status instead of just the initiator.

## Conclusion
Yes, introducing a persisted `deployment_id` (stored in a DB table) is the standard and recommended architectural pattern to support resumability and robust task management in this type of system.
