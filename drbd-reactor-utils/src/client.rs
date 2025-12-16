use crate::error::Result;
use crate::models::ReactorProfileStatus;
use crate::parser;

pub struct DrbdReactorClient;

impl DrbdReactorClient {
    pub async fn status(profile_name: Option<&str>) -> Result<(Vec<ReactorProfileStatus>, String)> {
        let args = if let Some(name) = profile_name {
            vec!["status", name]
        } else {
            vec!["status"]
        };

        // Suppress stderr to avoid noise if not found
        let output = crate::error::run_command("drbd-reactorctl", &args)
            .await
            .unwrap_or_default();

        let statuses = parser::parse_reactor_status(&output, profile_name);
        Ok((statuses, output))
    }

    pub async fn disable(profile_name: &str) -> Result<()> {
        crate::error::run_command("drbd-reactorctl", &["disable", profile_name]).await?;
        Ok(())
    }

    pub async fn evict(profile_name: &str) -> Result<()> {
        crate::error::run_command("drbd-reactorctl", &["evict", profile_name]).await?;
        Ok(())
    }

    pub async fn restart() -> Result<()> {
        // This usually requires systemctl, which is outside reactorctl
        // But we can implement a helper if needed.
        // For now, keep strictly to reactorctl wrappers or things that logic was doing.
        Ok(())
    }
}

