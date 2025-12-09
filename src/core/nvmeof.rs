use crate::models::NvmeOfConfig;

pub struct NvmeOfGenerator;

impl NvmeOfGenerator {
    /// Generate nvmetcli commands to set up the NVMe-oF target
    pub fn generate_setup_commands(
        _resource_name: &str,
        backing_device: &str,
        config: &NvmeOfConfig,
        vip: &str,
    ) -> Vec<String> {
        let subsystem = &config.nqn;
        let port_id = "1"; // Simplification: Assuming port 1. Real-world might need dynamic allocation.
        
        let mut commands = Vec::new();

        // 1. Create Subsystem
        commands.push(format!("create subsystem {}", subsystem));

        // 2. Enable Any Host access (if no ACLs) or set ACLs
        // Simplified: allow_any_host=1
        commands.push(format!("set subsystem {} attr allow_any_host=1", subsystem));

        // 3. Create Namespace
        commands.push(format!("create namespace 1 on subsystem {}", subsystem));
        commands.push(format!("set subsystem {} namespace 1 device path={}", subsystem, backing_device));
        commands.push(format!("set subsystem {} namespace 1 enable=1", subsystem));

        // 4. Create Port
        commands.push(format!("create port {}", port_id));
        commands.push(format!("set port {} addr adrfam=ipv4 traddr={} trtype={} trsvcid={}", 
            port_id, vip, config.fabric_type, config.trsvcid));

        // 5. Link Subsystem to Port
        commands.push(format!("set port {} subsys {}", port_id, subsystem));

        // 6. Save
        commands.push("saveconfig".to_string());

        commands
    }
    
    // Teardown commands...
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::NvmeOfConfig;

    #[test]
    fn test_generate_setup_commands() {
        let config = NvmeOfConfig {
            nqn: "nqn.2025-01.com.example:nvme-storage".to_string(),
            allowed_nqns: vec![],
            fabric_type: "tcp".to_string(),
            trsvcid: "4420".to_string(),
        };
        let commands = NvmeOfGenerator::generate_setup_commands(
            "res_nvmeof",
            "/dev/drbd0",
            &config,
            "192.168.1.202",
        );

        assert_eq!(commands.len(), 9);
        assert_eq!(commands[0], "create subsystem nqn.2025-01.com.example:nvme-storage");
        assert_eq!(commands[1], "set subsystem nqn.2025-01.com.example:nvme-storage attr allow_any_host=1");
        assert_eq!(commands[2], "create namespace 1 on subsystem nqn.2025-01.com.example:nvme-storage");
        assert_eq!(commands[3], "set subsystem nqn.2025-01.com.example:nvme-storage namespace 1 device path=/dev/drbd0");
        assert_eq!(commands[4], "set subsystem nqn.2025-01.com.example:nvme-storage namespace 1 enable=1");
        assert_eq!(commands[5], "create port 1");
        assert_eq!(commands[6], "set port 1 addr adrfam=ipv4 traddr=192.168.1.202 trtype=tcp trsvcid=4420");
        assert_eq!(commands[7], "set port 1 subsys nqn.2025-01.com.example:nvme-storage");
        assert_eq!(commands[8], "saveconfig");
    }

    #[test]
    fn test_generate_setup_commands_rdma() {
        let config = NvmeOfConfig {
            nqn: "nqn.2025-01.com.example:nvme-rdma".to_string(),
            allowed_nqns: vec![],
            fabric_type: "rdma".to_string(),
            trsvcid: "4420".to_string(),
        };
        let commands = NvmeOfGenerator::generate_setup_commands(
            "res_nvmeof_rdma",
            "/dev/drbd1",
            &config,
            "192.168.1.203",
        );
        assert_eq!(commands.len(), 9);
        assert!(commands[6].contains("trtype=rdma"));
    }
}

