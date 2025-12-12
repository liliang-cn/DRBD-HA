//! SQLite database storage for HA profiles and nodes
//!
//! Provides persistent storage using rusqlite.

use crate::error::{AppError, AppResult};
use crate::models::{
    GeneratedUnits, HaProfile, HaProfileStatus, HaType, IscsiConfig, NfsConfig, Node, NodeStatus,
    NvmeOfConfig, PromoterSettings, StoragePool, VipConfig, Volume,
};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

/// Database wrapper for thread-safe access
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Open or create a database at the specified path
    pub fn open<P: AsRef<Path>>(path: P) -> AppResult<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.as_ref().parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    AppError::Config(format!("Failed to create database directory: {}", e))
                })?;
            }
        }

        let conn = Connection::open(path)
            .map_err(|e| AppError::Config(format!("Failed to open database: {}", e)))?;

        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_schema()?;
        Ok(db)
    }

    /// Open an in-memory database (for testing)
    #[cfg(test)]
    pub fn open_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| AppError::Config(format!("Failed to open in-memory database: {}", e)))?;

        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_schema()?;
        Ok(db)
    }

    /// Initialize database schema
    fn init_schema(&self) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(

                    r#"

                    -- Nodes table

                    CREATE TABLE IF NOT EXISTS nodes (

                        id TEXT PRIMARY KEY,

                        hostname TEXT NOT NULL,

                        ip TEXT NOT NULL,

                        ssh_port INTEGER NOT NULL DEFAULT 22,

                        ssh_user TEXT NOT NULL DEFAULT 'root',

                        is_local INTEGER NOT NULL DEFAULT 0,

                        status TEXT NOT NULL DEFAULT 'unknown',

                        last_seen TEXT,

                        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,

                        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP

                    );

        

                    -- HA Profiles table

                    CREATE TABLE IF NOT EXISTS ha_profiles (

                        id TEXT PRIMARY KEY,

                        name TEXT NOT NULL UNIQUE,

                        resource_name TEXT NOT NULL,

                        mount_point TEXT NOT NULL,

                        fs_type TEXT NOT NULL DEFAULT 'xfs',

                        vip_address TEXT,

                        vip_netmask INTEGER,

                        vip_interface TEXT,

                        services TEXT NOT NULL,

                        stop_on_demote INTEGER NOT NULL DEFAULT 1,

                        on_demote_failure TEXT NOT NULL DEFAULT 'reboot',

                        status TEXT NOT NULL DEFAULT 'unknown',

                        generated_units TEXT,

                        ha_type TEXT DEFAULT 'generic',

                        nfs_config TEXT,

                        iscsi_config TEXT,

                        nvmeof_config TEXT,

                        generated_config TEXT,

                        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,

                        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP

                    );

        

                    -- Storage Pools table

                    CREATE TABLE IF NOT EXISTS storage_pools (

                        id TEXT PRIMARY KEY,

                        name TEXT NOT NULL,

                        node_id TEXT NOT NULL,

                        type TEXT NOT NULL,

                        device TEXT NOT NULL,

                        total_size INTEGER NOT NULL DEFAULT 0,

                        free_size INTEGER NOT NULL DEFAULT 0,

                        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,

                        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,

                        UNIQUE(name, node_id)

                    );

        

                    -- Volumes table (LVM Logical Volumes)

                    CREATE TABLE IF NOT EXISTS volumes (

                        id TEXT PRIMARY KEY,

                        pool_id TEXT NOT NULL,

                        name TEXT NOT NULL,

                        size_gb INTEGER NOT NULL,

                        device_path TEXT NOT NULL,

                        drbd_res TEXT, -- Associated DRBD resource name

                        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,

                        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,

                        FOREIGN KEY (pool_id) REFERENCES storage_pools(id) ON DELETE CASCADE

                    );

        

                    -- Create indexes

                    CREATE INDEX IF NOT EXISTS idx_nodes_hostname ON nodes(hostname);

                    CREATE INDEX IF NOT EXISTS idx_ha_profiles_name ON ha_profiles(name);

                    CREATE INDEX IF NOT EXISTS idx_ha_profiles_resource ON ha_profiles(resource_name);

                    CREATE INDEX IF NOT EXISTS idx_storage_pools_name ON storage_pools(name);

                    CREATE INDEX IF NOT EXISTS idx_volumes_pool_id ON volumes(pool_id);

                    CREATE UNIQUE INDEX IF NOT EXISTS idx_volumes_name_pool ON volumes(name, pool_id);

                    "#,

                )
        .map_err(|e| AppError::Config(format!("Failed to initialize database schema: {}", e)))?;

        // Apply migrations for schema updates
        self.apply_migrations(&conn)?;

        Ok(())
    }

    /// Apply database migrations to handle schema updates
    fn apply_migrations(&self, conn: &Connection) -> AppResult<()> {
        // Migration 1: Add ha_type column if it doesn't exist
        if conn
            .execute_batch(
                r#"
            ALTER TABLE ha_profiles ADD COLUMN ha_type TEXT DEFAULT 'generic';
            "#,
            )
            .is_err()
        {
            // Column likely already exists, ignore error
        }

        // Migration 2: Add nfs_config column if it doesn't exist
        if conn
            .execute_batch(
                r#"
            ALTER TABLE ha_profiles ADD COLUMN nfs_config TEXT;
            "#,
            )
            .is_err()
        {
            // Column likely already exists, ignore error
        }

        // Migration 3: Add iscsi_config column if it doesn't exist
        if conn
            .execute_batch(
                r#"
            ALTER TABLE ha_profiles ADD COLUMN iscsi_config TEXT;
            "#,
            )
            .is_err()
        {
            // Column likely already exists, ignore error
        }

        // Migration 5: Add nvmeof_config column if it doesn't exist (if it was missed in earlier migration)
        if conn
            .execute_batch(
                r#"
            ALTER TABLE ha_profiles ADD COLUMN nvmeof_config TEXT;
            "#,
            )
            .is_err()
        {
            // Column likely already exists, ignore error
        }

        // Migration 6: Add generated_config column if it doesn't exist (if it was missed in earlier migration)
        if conn
            .execute_batch(
                r#"
            ALTER TABLE ha_profiles ADD COLUMN generated_config TEXT;
            "#,
            )
            .is_err()
        {
            // Column likely already exists, ignore error
        }

        // Migration 7: Add PromoterSettings advanced fields
        if conn
            .execute_batch(
                r#"
            ALTER TABLE ha_profiles ADD COLUMN dependencies_as TEXT;
            ALTER TABLE ha_profiles ADD COLUMN target_as TEXT;
            ALTER TABLE ha_profiles ADD COLUMN on_quorum_loss TEXT;
            ALTER TABLE ha_profiles ADD COLUMN preferred_nodes TEXT;
            ALTER TABLE ha_profiles ADD COLUMN preferred_nodes_policy TEXT;
            ALTER TABLE ha_profiles ADD COLUMN sleep_before_promote_factor INTEGER;
            "#,
            )
            .is_err()
        {
            // Columns likely already exist, ignore error
        }

        Ok(())
    }

    // ==================== Node Operations ====================

    /// Insert a new node
    pub fn insert_node(&self, node: &Node) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"INSERT INTO nodes (id, hostname, ip, ssh_port, ssh_user, is_local, status, last_seen)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                node.id,
                node.hostname,
                node.ip,
                node.ssh_port,
                node.ssh_user,
                node.is_local as i32,
                format!("{:?}", node.status).to_lowercase(),
                node.last_seen.map(|t| t.to_rfc3339()),
            ],
        )
        .map_err(|e| AppError::Config(format!("Failed to insert node: {}", e)))?;

        Ok(())
    }

    /// Get a node by ID
    pub fn get_node(&self, id: &str) -> AppResult<Option<Node>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, hostname, ip, ssh_port, ssh_user, is_local, status, last_seen FROM nodes WHERE id = ?1",
            params![id],
            |row| {
                Ok(Node {
                    id: row.get(0)?,
                    hostname: row.get(1)?,
                    ip: row.get(2)?,
                    ssh_port: row.get(3)?,
                    ssh_user: row.get(4)?,
                    is_local: row.get::<_, i32>(5)? != 0,
                    status: parse_node_status(&row.get::<_, String>(6)?),
                    last_seen: row.get::<_, Option<String>>(7)?.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc))),
                })
            },
        )
        .optional()
        .map_err(|e| AppError::Config(format!("Failed to get node: {}", e)))
    }

    /// Get all nodes
    pub fn get_all_nodes(&self) -> AppResult<Vec<Node>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, hostname, ip, ssh_port, ssh_user, is_local, status, last_seen FROM nodes")
            .map_err(|e| AppError::Config(format!("Failed to prepare query: {}", e)))?;

        let nodes = stmt
            .query_map([], |row| {
                Ok(Node {
                    id: row.get(0)?,
                    hostname: row.get(1)?,
                    ip: row.get(2)?,
                    ssh_port: row.get(3)?,
                    ssh_user: row.get(4)?,
                    is_local: row.get::<_, i32>(5)? != 0,
                    status: parse_node_status(&row.get::<_, String>(6)?),
                    last_seen: row.get::<_, Option<String>>(7)?.and_then(|s| {
                        DateTime::parse_from_rfc3339(&s)
                            .ok()
                            .map(|dt| dt.with_timezone(&Utc))
                    }),
                })
            })
            .map_err(|e| AppError::Config(format!("Failed to query nodes: {}", e)))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Config(format!("Failed to collect nodes: {}", e)))?;

        Ok(nodes)
    }

    /// Update node status
    pub fn update_node_status(
        &self,
        id: &str,
        status: NodeStatus,
        last_seen: Option<DateTime<Utc>>,
    ) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            UPDATE nodes SET status = ?1, last_seen = ?2, updated_at = CURRENT_TIMESTAMP WHERE id = ?3
            "#,
            params![
                format!("{:?}", status).to_lowercase(),
                last_seen.map(|t| t.to_rfc3339()),
                id
            ],
        )
        .map_err(|e| AppError::Config(format!("Failed to update node status: {}", e)))?;

        Ok(())
    }

    /// Delete a node
    pub fn delete_node(&self, id: &str) -> AppResult<bool> {
        let conn = self.conn.lock().unwrap();
        let rows = conn
            .execute("DELETE FROM nodes WHERE id = ?1", params![id])
            .map_err(|e| AppError::Config(format!("Failed to delete node: {}", e)))?;

        Ok(rows > 0)
    }

    // ==================== HA Profile Operations ====================

    /// Insert a new HA profile
    pub fn insert_ha_profile(&self, profile: &HaProfile) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        let services_json = serde_json::to_string(&profile.promoter.services)
            .map_err(|e| AppError::Config(format!("Failed to serialize services: {}", e)))?;
        let generated_units_json = serde_json::to_string(&profile.generated_units)
            .map_err(|e| AppError::Config(format!("Failed to serialize generated_units: {}", e)))?;

        // Serialize new config fields
        let ha_type = serde_json::to_string(&profile.ha_type)
            .map(|s| s.trim_matches('"').to_string()) // Store as raw string "generic", "nfs" etc.
            .unwrap_or_else(|_| "generic".to_string());

        let nfs_config =
            if let Some(ref c) = profile.nfs {
                Some(serde_json::to_string(c).map_err(|e| {
                    AppError::Config(format!("Failed to serialize nfs config: {}", e))
                })?)
            } else {
                None
            };

        let iscsi_config = if let Some(ref c) = profile.iscsi {
            Some(serde_json::to_string(c).map_err(|e| {
                AppError::Config(format!("Failed to serialize iscsi config: {}", e))
            })?)
        } else {
            None
        };

        let nvmeof_config = if let Some(ref c) = profile.nvmeof {
            Some(serde_json::to_string(c).map_err(|e| {
                AppError::Config(format!("Failed to serialize nvmeof config: {}", e))
            })?)
        } else {
            None
        };

        conn.execute(
            r#"INSERT INTO ha_profiles (
                id, name, resource_name, mount_point, fs_type, vip_address, vip_netmask, vip_interface, 
                services, stop_on_demote, on_demote_failure, status, generated_units,
                ha_type, nfs_config, iscsi_config, nvmeof_config, generated_config,
                dependencies_as, target_as, on_quorum_loss, preferred_nodes, preferred_nodes_policy, sleep_before_promote_factor
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)
            "#,
            params![
                profile.id,
                profile.name,
                profile.resource_name,
                profile.mount_point,
                profile.fs_type,
                profile.vip.as_ref().map(|v| &v.address),
                profile.vip.as_ref().map(|v| v.netmask as i32),
                profile.vip.as_ref().map(|v| &v.interface),
                services_json,
                profile.promoter.stop_on_demote as i32,
                profile.promoter.on_demote_failure,
                format!("{:?}", profile.status).to_lowercase(),
                generated_units_json,
                ha_type,
                nfs_config,
                iscsi_config,
                nvmeof_config,
                profile.generated_config,
                profile.promoter.dependencies_as,
                profile.promoter.target_as,
                profile.promoter.on_quorum_loss,
                profile.promoter.preferred_nodes.as_ref().and_then(|v| serde_json::to_string(v).ok()),
                profile.promoter.preferred_nodes_policy,
                profile.promoter.sleep_before_promote_factor.map(|v| v as i32),
            ],
        )
        .map_err(|e| AppError::Config(format!("Failed to insert HA profile: {}", e)))?;

        Ok(())
    }

    /// Get an HA profile by ID
    pub fn get_ha_profile(&self, id: &str) -> AppResult<Option<HaProfile>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            r#"
            SELECT id, name, resource_name, mount_point, fs_type, vip_address, vip_netmask, vip_interface,
                   services, stop_on_demote, on_demote_failure, status, generated_units,
                   ha_type, nfs_config, iscsi_config, nvmeof_config, generated_config,
                   dependencies_as, target_as, on_quorum_loss, preferred_nodes, preferred_nodes_policy, sleep_before_promote_factor
            FROM ha_profiles WHERE id = ?1
            "#,
            params![id],
            |row| Ok(row_to_ha_profile(row)),
        )
        .optional()
        .map_err(|e| AppError::Config(format!("Failed to get HA profile: {}", e)))?
        .transpose()
    }

    /// Get an HA profile by name
    pub fn get_ha_profile_by_name(&self, name: &str) -> AppResult<Option<HaProfile>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            r#"
            SELECT id, name, resource_name, mount_point, fs_type, vip_address, vip_netmask, vip_interface,
                   services, stop_on_demote, on_demote_failure, status, generated_units,
                   ha_type, nfs_config, iscsi_config, nvmeof_config, generated_config,
                   dependencies_as, target_as, on_quorum_loss, preferred_nodes, preferred_nodes_policy, sleep_before_promote_factor
            FROM ha_profiles WHERE name = ?1
            "#,
            params![name],
            |row| Ok(row_to_ha_profile(row)),
        )
        .optional()
        .map_err(|e| AppError::Config(format!("Failed to get HA profile by name: {}", e)))?
        .transpose()
    }

    /// Get all HA profiles
    pub fn get_all_ha_profiles(&self) -> AppResult<Vec<HaProfile>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                r#"
            SELECT id, name, resource_name, mount_point, fs_type, vip_address, vip_netmask, vip_interface,
                   services, stop_on_demote, on_demote_failure, status, generated_units,
                   ha_type, nfs_config, iscsi_config, nvmeof_config, generated_config,
                   dependencies_as, target_as, on_quorum_loss, preferred_nodes, preferred_nodes_policy, sleep_before_promote_factor
            FROM ha_profiles
            "#,
            )
            .map_err(|e| AppError::Config(format!("Failed to prepare query: {}", e)))?;

        let profiles = stmt
            .query_map([], |row| Ok(row_to_ha_profile(row)))
            .map_err(|e| AppError::Config(format!("Failed to query HA profiles: {}", e)))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Config(format!("Failed to collect HA profiles: {}", e)))?;

        // Filter out any errors during parsing
        let profiles: Vec<HaProfile> = profiles.into_iter().filter_map(|r| r.ok()).collect();
        Ok(profiles)
    }

    /// Update HA profile status
    pub fn update_ha_profile_status(&self, id: &str, status: HaProfileStatus) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE ha_profiles SET status = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![format!("{:?}", status).to_lowercase(), id],
        )
        .map_err(|e| AppError::Config(format!("Failed to update HA profile status: {}", e)))?;

        Ok(())
    }

    /// Update an entire HA profile (including VIP and new configs)
    pub fn update_ha_profile(&self, profile: &HaProfile) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        let services_json = serde_json::to_string(&profile.promoter.services)
            .map_err(|e| AppError::Config(format!("Failed to serialize services: {}", e)))?;
        let generated_units_json = serde_json::to_string(&profile.generated_units)
            .map_err(|e| AppError::Config(format!("Failed to serialize generated_units: {}", e)))?;

        // Serialize new config fields
        let ha_type = serde_json::to_string(&profile.ha_type)
            .map(|s| s.trim_matches('"').to_string())
            .unwrap_or_else(|_| "generic".to_string());

        let nfs_config =
            if let Some(ref c) = profile.nfs {
                Some(serde_json::to_string(c).map_err(|e| {
                    AppError::Config(format!("Failed to serialize nfs config: {}", e))
                })?)
            } else {
                None
            };

        let iscsi_config = if let Some(ref c) = profile.iscsi {
            Some(serde_json::to_string(c).map_err(|e| {
                AppError::Config(format!("Failed to serialize iscsi config: {}", e))
            })?)
        } else {
            None
        };

        let nvmeof_config = if let Some(ref c) = profile.nvmeof {
            Some(serde_json::to_string(c).map_err(|e| {
                AppError::Config(format!("Failed to serialize nvmeof config: {}", e))
            })?)
        } else {
            None
        };

        conn.execute(
            r#"UPDATE ha_profiles SET
                name = ?1,
                resource_name = ?2,
                mount_point = ?3,
                fs_type = ?4,
                vip_address = ?5,
                vip_netmask = ?6,
                vip_interface = ?7,
                services = ?8,
                stop_on_demote = ?9,
                on_demote_failure = ?10,
                status = ?11,
                generated_units = ?12,
                ha_type = ?13,
                nfs_config = ?14,
                iscsi_config = ?15,
                nvmeof_config = ?16,
                generated_config = ?17,
                dependencies_as = ?18,
                target_as = ?19,
                on_quorum_loss = ?20,
                preferred_nodes = ?21,
                preferred_nodes_policy = ?22,
                sleep_before_promote_factor = ?23,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?24
            "#,
            params![
                profile.name,
                profile.resource_name,
                profile.mount_point,
                profile.fs_type,
                profile.vip.as_ref().map(|v| &v.address),
                profile.vip.as_ref().map(|v| v.netmask as i32),
                profile.vip.as_ref().map(|v| &v.interface),
                services_json,
                profile.promoter.stop_on_demote as i32,
                profile.promoter.on_demote_failure,
                format!("{:?}", profile.status).to_lowercase(),
                generated_units_json,
                ha_type,
                nfs_config,
                iscsi_config,
                nvmeof_config,
                profile.generated_config,
                profile.promoter.dependencies_as,
                profile.promoter.target_as,
                profile.promoter.on_quorum_loss,
                profile.promoter.preferred_nodes.as_ref().and_then(|v| serde_json::to_string(v).ok()),
                profile.promoter.preferred_nodes_policy,
                profile.promoter.sleep_before_promote_factor.map(|v| v as i32),
                profile.id,
            ],
        )
        .map_err(|e| AppError::Config(format!("Failed to update HA profile: {}", e)))?;

        Ok(())
    }

    /// Delete an HA profile
    pub fn delete_ha_profile(&self, id: &str) -> AppResult<bool> {
        let conn = self.conn.lock().unwrap();
        let rows = conn
            .execute("DELETE FROM ha_profiles WHERE id = ?1", params![id])
            .map_err(|e| AppError::Config(format!("Failed to delete HA profile: {}", e)))?;

        Ok(rows > 0)
    }

    /// Check if an HA profile name exists
    pub fn ha_profile_name_exists(&self, name: &str) -> AppResult<bool> {
        let conn = self.conn.lock().unwrap();
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM ha_profiles WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .map_err(|e| AppError::Config(format!("Failed to check profile name: {}", e)))?;

        Ok(count > 0)
    }

    // ==================== Storage Pool Operations ====================

    /// Insert a new storage pool
    pub fn insert_storage_pool(&self, pool: &StoragePool, node_id: &str) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO storage_pools (id, name, node_id, type, device, total_size, free_size)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                pool.id,
                pool.name,
                node_id,
                "lvm", // For now, only LVM is supported
                pool.device,
                pool.total_size,
                pool.free_size,
            ],
        )
        .map_err(|e| AppError::Config(format!("Failed to insert storage pool: {}", e)))?;

        Ok(())
    }

    /// Get a storage pool by ID
    pub fn get_storage_pool(&self, id: &str) -> AppResult<Option<StoragePool>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, name, node_id, device, total_size, free_size FROM storage_pools WHERE id = ?1",
            params![id],
            |row| {
                Ok(StoragePool {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    node_id: row.get(2)?,
                    device: row.get(3)?,
                    total_size: row.get(4)?,
                    free_size: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(|e| AppError::Config(format!("Failed to get storage pool: {}", e)))
    }

    /// Get all storage pools
    pub fn get_all_storage_pools(&self) -> AppResult<Vec<StoragePool>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, name, node_id, device, total_size, free_size FROM storage_pools")
            .map_err(|e| AppError::Config(format!("Failed to prepare query: {}", e)))?;

        let pools = stmt
            .query_map([], |row| {
                Ok(StoragePool {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    node_id: row.get(2)?,
                    device: row.get(3)?,
                    total_size: row.get(4)?,
                    free_size: row.get(5)?,
                })
            })
            .map_err(|e| AppError::Config(format!("Failed to query storage pools: {}", e)))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Config(format!("Failed to collect storage pools: {}", e)))?;

        Ok(pools)
    }

    /// Update storage pool sizes (total_size, free_size)
    pub fn update_storage_pool_sizes(
        &self,
        id: &str,
        total_size: u64,
        free_size: u64,
    ) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE storage_pools SET total_size = ?1, free_size = ?2, updated_at = CURRENT_TIMESTAMP WHERE id = ?3",
            params![total_size, free_size, id],
        )
        .map_err(|e| AppError::Config(format!("Failed to update storage pool sizes: {}", e)))?;

        Ok(())
    }

    /// Delete a storage pool
    pub fn delete_storage_pool(&self, id: &str) -> AppResult<bool> {
        let conn = self.conn.lock().unwrap();
        let rows = conn
            .execute("DELETE FROM storage_pools WHERE id = ?1", params![id])
            .map_err(|e| AppError::Config(format!("Failed to delete storage pool: {}", e)))?;

        Ok(rows > 0)
    }

    // ==================== Volume Operations ====================

    /// Insert a new volume
    pub fn insert_volume(&self, volume: &Volume) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO volumes (id, pool_id, name, size_gb, device_path, drbd_res)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                volume.id,
                volume.pool_id,
                volume.name,
                volume.size_gb,
                volume.device_path,
                volume.drbd_res,
            ],
        )
        .map_err(|e| AppError::Config(format!("Failed to insert volume: {}", e)))?;

        Ok(())
    }

    /// Get a volume by ID
    pub fn get_volume(&self, id: &str) -> AppResult<Option<Volume>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, pool_id, name, size_gb, device_path, drbd_res FROM volumes WHERE id = ?1",
            params![id],
            |row| {
                Ok(Volume {
                    id: row.get(0)?,
                    pool_id: row.get(1)?,
                    name: row.get(2)?,
                    size_gb: row.get(3)?,
                    device_path: row.get(4)?,
                    drbd_res: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(|e| AppError::Config(format!("Failed to get volume: {}", e)))
    }

    /// Get all volumes in a specific pool
    pub fn get_all_volumes_in_pool(&self, pool_id: &str) -> AppResult<Vec<Volume>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, pool_id, name, size_gb, device_path, drbd_res FROM volumes WHERE pool_id = ?1")
            .map_err(|e| AppError::Config(format!("Failed to prepare query: {}", e)))?;

        let volumes = stmt
            .query_map(params![pool_id], |row| {
                Ok(Volume {
                    id: row.get(0)?,
                    pool_id: row.get(1)?,
                    name: row.get(2)?,
                    size_gb: row.get(3)?,
                    device_path: row.get(4)?,
                    drbd_res: row.get(5)?,
                })
            })
            .map_err(|e| AppError::Config(format!("Failed to query volumes: {}", e)))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Config(format!("Failed to collect volumes: {}", e)))?;

        Ok(volumes)
    }

    /// Delete a volume
    pub fn delete_volume(&self, id: &str) -> AppResult<bool> {
        let conn = self.conn.lock().unwrap();
        let rows = conn
            .execute("DELETE FROM volumes WHERE id = ?1", params![id])
            .map_err(|e| AppError::Config(format!("Failed to delete volume: {}", e)))?;

        Ok(rows > 0)
    }

    /// Get a volume by DRBD resource name
    pub fn get_volume_by_drbd_res(&self, drbd_res_name: &str) -> AppResult<Option<Volume>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, pool_id, name, size_gb, device_path, drbd_res FROM volumes WHERE drbd_res = ?1",
            params![drbd_res_name],
            |row| {
                Ok(Volume {
                    id: row.get(0)?,
                    pool_id: row.get(1)?,
                    name: row.get(2)?,
                    size_gb: row.get(3)?,
                    device_path: row.get(4)?,
                    drbd_res: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(|e| AppError::Config(format!("Failed to get volume by DRBD resource name: {}", e)))
    }
}

/// Parse node status from string
fn parse_node_status(s: &str) -> NodeStatus {
    match s.to_lowercase().as_str() {
        "online" => NodeStatus::Online,
        "offline" => NodeStatus::Offline,
        "error" => NodeStatus::Error,
        _ => NodeStatus::Unknown,
    }
}

/// Parse HA profile status from string
fn parse_ha_profile_status(s: &str) -> HaProfileStatus {
    match s.to_lowercase().as_str() {
        "active" => HaProfileStatus::Active,
        "standby" => HaProfileStatus::Standby,
        "stopped" => HaProfileStatus::Stopped,
        "error" => HaProfileStatus::Error,
        _ => HaProfileStatus::Unknown,
    }
}

/// Convert a database row to HaProfile
fn row_to_ha_profile(row: &rusqlite::Row) -> AppResult<HaProfile> {
    // Column indices:
    // 0: id, 1: name, 2: resource_name, 3: mount_point, 4: fs_type
    // 5: vip_address, 6: vip_netmask, 7: vip_interface
    // 8: services, 9: stop_on_demote, 10: on_demote_failure, 11: status, 12: generated_units
    // 13: ha_type, 14: nfs_config, 15: iscsi_config, 16: nvmeof_config
    // 17: generated_config

    let services_json: String = row.get(8).map_err(|e| AppError::Config(e.to_string()))?;
    let services: Vec<String> = serde_json::from_str(&services_json)
        .map_err(|e| AppError::Config(format!("Failed to parse services: {}", e)))?;

    let vip = match (
        row.get::<_, Option<String>>(5).ok().flatten(),
        row.get::<_, Option<i32>>(6).ok().flatten(),
        row.get::<_, Option<String>>(7).ok().flatten(),
    ) {
        (Some(address), Some(netmask), Some(interface)) => Some(VipConfig {
            address,
            netmask: netmask as u8,
            interface,
        }),
        _ => None,
    };

    // Parse generated_units from JSON
    let generated_units: GeneratedUnits = row
        .get::<_, Option<String>>(12)
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();

    // Parse HA Type
    let ha_type_str: String = row.get(13).unwrap_or_else(|_| "generic".to_string());
    let ha_type =
        serde_json::from_str::<HaType>(&format!("\"{}\"", ha_type_str)).unwrap_or(HaType::Generic);

    // Parse specific configs
    let nfs = row
        .get::<_, Option<String>>(14)
        .ok()
        .flatten()
        .and_then(|j| serde_json::from_str::<NfsConfig>(&j).ok());

    let iscsi = row
        .get::<_, Option<String>>(15)
        .ok()
        .flatten()
        .and_then(|j| serde_json::from_str::<IscsiConfig>(&j).ok());

    let nvmeof = row
        .get::<_, Option<String>>(16)
        .ok()
        .flatten()
        .and_then(|j| serde_json::from_str::<NvmeOfConfig>(&j).ok());

    let generated_config = row.get::<_, Option<String>>(17).ok().flatten();

    Ok(HaProfile {
        id: row.get(0).map_err(|e| AppError::Config(e.to_string()))?,
        name: row.get(1).map_err(|e| AppError::Config(e.to_string()))?,
        resource_name: row.get(2).map_err(|e| AppError::Config(e.to_string()))?,
        mount_point: row.get(3).map_err(|e| AppError::Config(e.to_string()))?,
        fs_type: row
            .get::<_, String>(4)
            .unwrap_or_else(|_| "xfs".to_string()),
        vip,
                    promoter: PromoterSettings {
                        services,
                        stop_on_demote: row
                            .get::<_, i32>(9)
                            .map_err(|e| AppError::Config(e.to_string()))?
                            != 0,
                        on_demote_failure: row.get(10).map_err(|e| AppError::Config(e.to_string()))?,
                        dependencies_as: row.get::<_, Option<String>>(18).unwrap_or_default().and_then(|s| serde_json::from_str(&s).ok()),
                        target_as: row.get::<_, Option<String>>(19).unwrap_or_default().and_then(|s| serde_json::from_str(&s).ok()),
                        on_quorum_loss: row.get::<_, Option<String>>(20).unwrap_or_default().and_then(|s| serde_json::from_str(&s).ok()),
                        preferred_nodes: row.get::<_, Option<String>>(21).unwrap_or_default().and_then(|s| serde_json::from_str(&s).ok()),
                        preferred_nodes_policy: row.get::<_, Option<String>>(22).unwrap_or_default().and_then(|s| serde_json::from_str(&s).ok()),
                        sleep_before_promote_factor: row.get::<_, Option<u32>>(23).unwrap_or_default(),
                    },        status: parse_ha_profile_status(&row.get::<_, String>(11).unwrap_or_default()),
        generated_units,
        ha_type,
        nfs,
        iscsi,
        nvmeof,
        generated_config,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_init() {
        let db = Database::open_in_memory().unwrap();
        assert!(db.get_all_nodes().unwrap().is_empty());
        assert!(db.get_all_ha_profiles().unwrap().is_empty());
    }

    #[test]
    fn test_node_crud() {
        let db = Database::open_in_memory().unwrap();

        let node = Node {
            id: "test-node-1".to_string(),
            hostname: "node1".to_string(),
            ip: "192.168.1.1".to_string(),
            ssh_port: 22,
            ssh_user: "root".to_string(),
            is_local: false,
            status: NodeStatus::Online,
            last_seen: Some(Utc::now()),
        };

        // Insert
        db.insert_node(&node).unwrap();

        // Get
        let retrieved = db.get_node("test-node-1").unwrap().unwrap();
        assert_eq!(retrieved.hostname, "node1");
        assert_eq!(retrieved.ip, "192.168.1.1");

        // Update status
        db.update_node_status("test-node-1", NodeStatus::Offline, None)
            .unwrap();
        let updated = db.get_node("test-node-1").unwrap().unwrap();
        assert_eq!(updated.status, NodeStatus::Offline);

        // List all
        let all = db.get_all_nodes().unwrap();
        assert_eq!(all.len(), 1);

        // Delete
        assert!(db.delete_node("test-node-1").unwrap());
        assert!(db.get_node("test-node-1").unwrap().is_none());
    }

    #[test]
    fn test_ha_profile_crud() {
        let db = Database::open_in_memory().unwrap();

        let profile = HaProfile {
            id: "test-profile-1".to_string(),
            name: "mysql-ha".to_string(),
            ha_type: HaType::Generic,
            resource_name: "r0".to_string(),
            mount_point: "/var/lib/mysql".to_string(),
            fs_type: "xfs".to_string(),
            vip: Some(VipConfig {
                address: "192.168.1.100".to_string(),
                netmask: 24,
                interface: "eth0".to_string(),
            }),
            promoter: PromoterSettings {
                services: vec!["mysql.service".to_string()],
                stop_on_demote: true,
                on_demote_failure: "reboot".to_string(),
            },
            status: HaProfileStatus::Unknown,
            generated_units: GeneratedUnits::default(),
            nfs: None,
            iscsi: None,
            nvmeof: None,
            generated_config: None,
        };

        // Insert
        db.insert_ha_profile(&profile).unwrap();

        // Get
        let retrieved = db.get_ha_profile("test-profile-1").unwrap().unwrap();
        assert_eq!(retrieved.name, "mysql-ha");
        assert_eq!(retrieved.resource_name, "r0");
        assert!(retrieved.vip.is_some());

        // Check name exists
        assert!(db.ha_profile_name_exists("mysql-ha").unwrap());
        assert!(!db.ha_profile_name_exists("nonexistent").unwrap());

        // Update status
        db.update_ha_profile_status("test-profile-1", HaProfileStatus::Active)
            .unwrap();
        let updated = db.get_ha_profile("test-profile-1").unwrap().unwrap();
        assert!(matches!(updated.status, HaProfileStatus::Active));

        // List all
        let all = db.get_all_ha_profiles().unwrap();
        assert_eq!(all.len(), 1);

        // Delete
        assert!(db.delete_ha_profile("test-profile-1").unwrap());
        assert!(db.get_ha_profile("test-profile-1").unwrap().is_none());
    }
}
