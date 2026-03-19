use crate::error::Result;
use crate::models::{EvictOptions, ReactorProfileStatus, StatusOptions};
use crate::parser;
use std::path::Path;

pub struct DrbdReactorClient;

impl DrbdReactorClient {
    /// Build the drbd-reactorctl evict command arguments as a string
    ///
    /// This is useful when you need to execute the command via SSH or other means.
    pub fn build_evict_command(profile_name: &str, options: Option<&EvictOptions>) -> String {
        let default_opts = EvictOptions::default();
        let opts = options.unwrap_or(&default_opts);
        let mut cmd = String::from("drbd-reactorctl evict");

        // Add context if specified
        if let Some(context) = &opts.context {
            cmd.push_str(&format!(" --context {}", context));
        }

        // Add nodes filter if specified
        if let Some(nodes) = &opts.nodes {
            if !nodes.is_empty() {
                cmd.push_str(&format!(" --nodes {}", nodes.join(" ")));
            }
        }

        // Add delay if specified (default is 20)
        if let Some(delay) = opts.delay {
            cmd.push_str(&format!(" --delay {}", delay));
        }

        // Add force flag if specified
        if opts.force {
            cmd.push_str(" --force");
        }

        // Add keep_masked flag if specified
        if opts.keep_masked {
            cmd.push_str(" --keep-masked");
        }

        // Add unmask flag if specified
        if opts.unmask {
            cmd.push_str(" --unmask");
        }

        // Add profile name
        cmd.push_str(" ");
        cmd.push_str(profile_name);

        cmd
    }

    /// Get status of drbd-reactor profiles
    ///
    /// # Arguments
    /// * `profile_name` - Optional profile name to filter
    /// * `options` - Optional status options (resource filter, verbose)
    pub async fn status(
        profile_name: Option<&str>,
        options: Option<StatusOptions>,
    ) -> Result<(Vec<ReactorProfileStatus>, String)> {
        let opts = options.unwrap_or_default();

        // Build command arguments
        let mut args_vec = vec!["status".to_string(), "--json".to_string()];

        // Add resource filter if specified
        if let Some(resources) = opts.resources {
            if !resources.is_empty() {
                args_vec.push("-r".to_string());
                args_vec.extend(resources);
            }
        }

        // Add verbose flag if specified
        if opts.verbose {
            args_vec.push("-v".to_string());
        }

        // Add profile name if specified
        if let Some(name) = profile_name {
            args_vec.push(name.to_string());
        }

        // Convert to &str slice for run_command
        let args: Vec<&str> = args_vec.iter().map(|s| s.as_str()).collect();

        // Suppress stderr to avoid noise if not found
        let output = crate::error::run_command("drbd-reactorctl", &args)
            .await
            .unwrap_or_default();

        let statuses = parser::parse_reactor_status(&output, profile_name);
        Ok((statuses, output))
    }

    /// Disable a drbd-reactor profile
    ///
    /// This renames the config file from `/etc/drbd-reactor.d/{name}.toml`
    /// to `/etc/drbd-reactor.d/{name}.toml.disabled`
    ///
    /// # Arguments
    /// * `profile_name` - Profile name to disable
    pub async fn disable(profile_name: &str) -> Result<()> {
        crate::error::run_command("drbd-reactorctl", &["disable", profile_name]).await?;
        Ok(())
    }

    /// Enable a drbd-reactor profile
    ///
    /// This renames the config file from `/etc/drbd-reactor.d/{name}.toml.disabled`
    /// back to `/etc/drbd-reactor.d/{name}.toml`
    ///
    /// # Arguments
    /// * `profile_name` - Profile name to enable
    pub async fn enable(profile_name: &str) -> Result<()> {
        crate::error::run_command("drbd-reactorctl", &["enable", profile_name]).await?;
        Ok(())
    }

    /// Check if a profile is disabled on the local node
    ///
    /// # Arguments
    /// * `profile_name` - Profile name to check
    /// * `reactor_dir` - Path to drbd-reactor config directory (default: "/etc/drbd-reactor.d")
    ///
    /// # Returns
    /// * `Ok(true)` if the profile is disabled (.toml.disabled exists)
    /// * `Ok(false)` if the profile is enabled (.toml exists)
    /// * `Err` if there's an error checking
    pub fn is_disabled_with_dir(profile_name: &str, reactor_dir: &str) -> Result<bool> {
        let disabled_path = Path::new(reactor_dir).join(format!("{}.toml.disabled", profile_name));
        let enabled_path = Path::new(reactor_dir).join(format!("{}.toml", profile_name));

        // Profile is disabled if .toml.disabled exists
        if disabled_path.exists() {
            Ok(true)
        } else if enabled_path.exists() {
            Ok(false)
        } else {
            // Neither exists - treat as not disabled (profile may not exist)
            Ok(false)
        }
    }

    /// Check if a profile is disabled on the local node (uses default reactor config directory)
    ///
    /// # Arguments
    /// * `profile_name` - Profile name to check
    ///
    /// # Returns
    /// * `Ok(true)` if the profile is disabled (.toml.disabled exists)
    /// * `Ok(false)` if the profile is enabled (.toml exists)
    /// * `Err` if there's an error checking
    pub fn is_disabled(profile_name: &str) -> Result<bool> {
        Self::is_disabled_with_dir(profile_name, crate::ConfigPaths::REACTOR_CONF_DIR)
    }

    /// Evict a promoter plugin controlled resource
    ///
    /// # Arguments
    /// * `profile_name` - Profile name to evict
    /// * `options` - Optional evict options (delay, force, keep_masked, etc.)
    pub async fn evict(profile_name: &str, options: Option<EvictOptions>) -> Result<()> {
        let opts = options.unwrap_or_default();

        // Build command arguments
        let mut args_vec = vec!["evict".to_string()];

        // Add context if specified
        if let Some(context) = &opts.context {
            args_vec.push("--context".to_string());
            args_vec.push(context.clone());
        }

        // Add nodes filter if specified
        if let Some(nodes) = &opts.nodes {
            if !nodes.is_empty() {
                args_vec.push("--nodes".to_string());
                args_vec.extend(nodes.clone());
            }
        }

        // Add delay if specified (default is 20)
        if let Some(delay) = opts.delay {
            args_vec.push("--delay".to_string());
            args_vec.push(delay.to_string());
        }

        // Add force flag if specified
        if opts.force {
            args_vec.push("--force".to_string());
        }

        // Add keep_masked flag if specified
        if opts.keep_masked {
            args_vec.push("--keep-masked".to_string());
        }

        // Add unmask flag if specified
        if opts.unmask {
            args_vec.push("--unmask".to_string());
        }

        // Add profile name
        args_vec.push(profile_name.to_string());

        // Convert to &str slice for run_command
        let args: Vec<&str> = args_vec.iter().map(|s| s.as_str()).collect();

        crate::error::run_command("drbd-reactorctl", &args).await?;
        Ok(())
    }

    pub async fn restart() -> Result<()> {
        // This usually requires systemctl, which is outside reactorctl
        // But we can implement a helper if needed.
        // For now, keep strictly to reactorctl wrappers or things that logic was doing.
        Ok(())
    }
}
