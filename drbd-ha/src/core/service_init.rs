//! Service Initialization Logic
//! 
//! Handles the initialization of empty data directories for services like MySQL, PostgreSQL, etc.
//! This prevents services from crashing loop when starting on a fresh DRBD volume.

use crate::core::run_shell_command;
use crate::error::AppResult;
use tracing::info;

/// Trait for service initializers
#[async_trait::async_trait]
pub trait ServiceInitializer {
    /// Initialize the data directory at the given mount point
    async fn initialize(&self, mount_point: &str) -> AppResult<()>;
}

/// Factory to get the correct initializer
pub struct ServiceInitFactory;

impl ServiceInitFactory {
    pub fn get(service_type: &str) -> Option<Box<dyn ServiceInitializer + Send + Sync>> {
        match service_type.to_lowercase().as_str() {
            "mysql" | "mariadb" => Some(Box::new(MysqlInitializer)),
            "postgresql" | "postgres" => Some(Box::new(PostgresInitializer)),
            "redis" => Some(Box::new(RedisInitializer)),
            _ => None,
        }
    }
    
    /// Detect service type from systemd service name
    pub fn detect(service_name: &str) -> Option<Box<dyn ServiceInitializer + Send + Sync>> {
        if service_name.contains("mysql") || service_name.contains("mariadb") {
            return Some(Box::new(MysqlInitializer));
        }
        if service_name.contains("postgresql") || service_name.contains("postgres") {
            return Some(Box::new(PostgresInitializer));
        }
        if service_name.contains("redis") {
            return Some(Box::new(RedisInitializer));
        }
        None
    }
}

/// MySQL / MariaDB Initializer
pub struct MysqlInitializer;

#[async_trait::async_trait]
impl ServiceInitializer for MysqlInitializer {
    async fn initialize(&self, mount_point: &str) -> AppResult<()> {
        info!("Initializing MySQL/MariaDB data directory at {}", mount_point);

        // 1. Fix Permissions
        // Ensure directory exists and is owned by mysql user
        run_shell_command(&format!("chown -R mysql:mysql {}", mount_point), "Fix MySQL permissions").await?;
        run_shell_command(&format!("chmod 750 {}", mount_point), "Fix MySQL chmod").await?;

        // 2. Initialize Data
        // Different commands for MySQL vs MariaDB, but usually mysql_install_db or mysqld --initialize works.
        // On Ubuntu/Debian with MariaDB, mysql_install_db is common.
        // On modern MySQL, mysqld --initialize-insecure.
        
        // Try mysqld first (MySQL 5.7+)
        // check if directory is empty first
        let check_empty = run_shell_command(&format!("ls -A {}", mount_point), "Check empty").await?;
        if !check_empty.stdout.trim().is_empty() {
            info!("MySQL data directory not empty, skipping initialization.");
            return Ok(());
        }

        // Try mysqld --initialize-insecure
        let mysqld_cmd = format!("mysqld --initialize-insecure --user=mysql --datadir={}", mount_point);
        if let Ok(out) = run_shell_command(&mysqld_cmd, "MySQL Initialize").await {
            if out.success() {
                info!("MySQL initialized successfully via mysqld");
                return Ok(());
            }
        }

        // Fallback to mysql_install_db (MariaDB / Older MySQL)
        let install_db_cmd = format!("mysql_install_db --user=mysql --datadir={}", mount_point);
        let out = run_shell_command(&install_db_cmd, "MariaDB Initialize").await?;
        
        if !out.success() {
            return Err(crate::error::AppError::Internal(format!("Failed to initialize MySQL data: {}", out.stderr)));
        }

        info!("MariaDB initialized successfully via mysql_install_db");
        Ok(())
    }
}

/// PostgreSQL Initializer
pub struct PostgresInitializer;

#[async_trait::async_trait]
impl ServiceInitializer for PostgresInitializer {
    async fn initialize(&self, mount_point: &str) -> AppResult<()> {
        info!("Initializing PostgreSQL data directory at {}", mount_point);

        // 1. Fix Permissions (postgres user)
        run_shell_command(&format!("chown -R postgres:postgres {}", mount_point), "Fix Postgres permissions").await?;
        run_shell_command(&format!("chmod 700 {}", mount_point), "Fix Postgres chmod").await?;

        // 2. Check if empty
        let check_empty = run_shell_command(&format!("ls -A {}", mount_point), "Check empty").await?;
        if !check_empty.stdout.trim().is_empty() {
            info!("Postgres data directory not empty, skipping initialization.");
            return Ok(());
        }

        // 3. Initialize
        // Needs to run as postgres user
        let init_cmd = format!("su - postgres -c \"initdb -D {}\"", mount_point);
        let out = run_shell_command(&init_cmd, "Postgres Initialize").await?;

        if !out.success() {
            return Err(crate::error::AppError::Internal(format!("Failed to initialize PostgreSQL: {}", out.stderr)));
        }

        Ok(())
    }
}

/// Redis Initializer
pub struct RedisInitializer;

#[async_trait::async_trait]
impl ServiceInitializer for RedisInitializer {
    async fn initialize(&self, mount_point: &str) -> AppResult<()> {
        info!("Initializing Redis data directory at {}", mount_point);
        
        // Redis just needs permissions
        run_shell_command(&format!("chown -R redis:redis {}", mount_point), "Fix Redis permissions").await?;
        run_shell_command(&format!("chmod 750 {}", mount_point), "Fix Redis chmod").await?;
        
        Ok(())
    }
}
