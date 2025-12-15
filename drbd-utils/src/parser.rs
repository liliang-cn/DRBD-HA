use crate::models::{DrbdPeerStatus, DrbdResourceStatus};

/// Parse drbdadm status output into structured format
pub fn parse_drbdadm_status(output: &str, resource_name: &str) -> Option<DrbdResourceStatus> {
    let lines: Vec<&str> = output.lines().collect();
    if lines.is_empty() {
        return None;
    }

    // First line: "mongodb-data role:Primary"
    let first_line = lines.first()?;
    if !first_line.starts_with(resource_name) {
        return None;
    }

    // Parse role from first line
    let role = first_line
        .split_whitespace()
        .find(|s| s.starts_with("role:"))
        .and_then(|s| s.strip_prefix("role:"))
        .unwrap_or("Unknown")
        .to_string();

    // Parse local disk status and open state from second line
    // "  disk:UpToDate open:yes"
    let (disk, open) = if lines.len() > 1 {
        let disk_line = lines[1].trim();
        let disk = disk_line
            .split_whitespace()
            .find(|s| s.starts_with("disk:"))
            .and_then(|s| s.strip_prefix("disk:"))
            .unwrap_or("Unknown")
            .to_string();
        let open = disk_line
            .split_whitespace()
            .find(|s| s.starts_with("open:"))
            .and_then(|s| s.strip_prefix("open:"))
            .map(|s| s == "yes")
            .unwrap_or(false);
        (disk, open)
    } else {
        ("Unknown".to_string(), false)
    };

    // Parse peer nodes
    let mut peers = Vec::new();
    let mut i = 2;
    while i < lines.len() {
        let line = lines[i].trim();

        // Peer line format: "gui02 role:Secondary" or "gui02 role:Secondary connection:Connected"
        if !line.starts_with("peer-disk:") && !line.is_empty() && !line.starts_with("disk:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if !parts.is_empty() {
                let peer_name = parts[0].to_string();

                // Skip if it looks like a status field rather than a hostname
                if peer_name.contains(':') {
                    i += 1;
                    continue;
                }

                let peer_role = parts
                    .iter()
                    .find(|s| s.starts_with("role:"))
                    .and_then(|s| s.strip_prefix("role:"))
                    .unwrap_or("Unknown")
                    .to_string();

                let connection = parts
                    .iter()
                    .find(|s| s.starts_with("connection:"))
                    .and_then(|s| s.strip_prefix("connection:"))
                    .map(|s| s.to_string());

                let replication = parts
                    .iter()
                    .find(|s| s.starts_with("replication:"))
                    .and_then(|s| s.strip_prefix("replication:"))
                    .map(|s| s.to_string());

                // Next line should have peer-disk
                let peer_disk = if i + 1 < lines.len() {
                    let next_line = lines[i + 1].trim();
                    if next_line.starts_with("peer-disk:") {
                        next_line
                            .split_whitespace()
                            .find(|s| s.starts_with("peer-disk:"))
                            .and_then(|s| s.strip_prefix("peer-disk:"))
                            .unwrap_or("Unknown")
                            .to_string()
                    } else {
                        "Unknown".to_string()
                    }
                } else {
                    "Unknown".to_string()
                };

                peers.push(DrbdPeerStatus {
                    name: peer_name,
                    role: peer_role,
                    peer_disk,
                    connection,
                    replication,
                });
            }
        }
        i += 1;
    }

    Some(DrbdResourceStatus {
        resource: resource_name.to_string(),
        role,
        disk,
        open,
        peers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_drbdadm_status_primary_up_to_date() {
        let output = r#"mysql_data role:Primary
  disk:UpToDate open:yes
  node2 role:Secondary
    peer-disk:UpToDate"#;
        let status = parse_drbdadm_status(output, "mysql_data").unwrap();
        assert_eq!(status.resource, "mysql_data");
        assert_eq!(status.role, "Primary");
        assert_eq!(status.disk, "UpToDate");
        assert!(status.open);
        assert_eq!(status.peers.len(), 1);
        assert_eq!(status.peers[0].name, "node2");
        assert_eq!(status.peers[0].role, "Secondary");
        assert_eq!(status.peers[0].peer_disk, "UpToDate");
    }

    #[test]
    fn test_parse_drbdadm_status_secondary_inconsistent() {
        let output = r#"r0 role:Secondary
  disk:Inconsistent
  node1 role:Primary
    peer-disk:UpToDate"#;
        let status = parse_drbdadm_status(output, "r0").unwrap();
        assert_eq!(status.resource, "r0");
        assert_eq!(status.role, "Secondary");
        assert_eq!(status.disk, "Inconsistent");
        assert!(!status.open);
        assert_eq!(status.peers.len(), 1);
        assert_eq!(status.peers[0].name, "node1");
        assert_eq!(status.peers[0].role, "Primary");
        assert_eq!(status.peers[0].peer_disk, "UpToDate");
    }
}
