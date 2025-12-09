use crate::models::IscsiConfig;

pub struct IscsiGenerator;

impl IscsiGenerator {
    /// Generate targetcli commands to set up the iSCSI target
    pub fn generate_setup_commands(
        resource_name: &str,
        backing_device: &str,
        config: &IscsiConfig,
        vip: &str,
    ) -> Vec<String> {
        let backstore_name = format!("bs_{}", resource_name);
        let tpg = "tpg1";

        let mut commands = Vec::new();

        // 1. Create Backstore
        // /backstores/block create name=bs_res_iscsi dev=/dev/drbdX
        commands.push(format!(
            "/backstores/block create name={} dev={}",
            backstore_name, backing_device
        ));

        // 2. Create Target
        // /iscsi create iqn.2025-01...
        commands.push(format!("/iscsi create {}", config.iqn));

        // 3. Create Portal
        // /iscsi/iqn.../tpg1/portals create 192.168.1.201
        commands.push(format!(
            "/iscsi/{}/{}/portals create {}",
            config.iqn, tpg, vip
        ));

        // 4. Create LUN
        // /iscsi/iqn.../tpg1/luns create /backstores/block/bs_res_iscsi
        commands.push(format!(
            "/iscsi/{}/{}/luns create /backstores/block/{}",
            config.iqn, tpg, backstore_name
        ));

        // 5. ACLs (if any)
        for initiator in &config.allowed_initiators {
            commands.push(format!(
                "/iscsi/{}/{}/acls create {}",
                config.iqn, tpg, initiator
            ));
        }

        // 6. Save
        commands.push("saveconfig".to_string());

        commands
    }

    /// Generate command to remove iSCSI configuration
    pub fn generate_teardown_commands(resource_name: &str, config: &IscsiConfig) -> Vec<String> {
        let backstore_name = format!("bs_{}", resource_name);
        let mut commands = Vec::new();

        // Delete Target (this recursively deletes TPG, LUNs, Portals, ACLs)
        commands.push(format!("delete /iscsi/{}", config.iqn));

        // Delete Backstore
        commands.push(format!("delete /backstores/block/{}", backstore_name));

        // Save
        commands.push("saveconfig".to_string());

        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::IscsiConfig;

    #[test]
    fn test_generate_setup_commands() {
        let config = IscsiConfig {
            iqn: "iqn.2025-01.com.example:iscsi-storage".to_string(),
            allowed_initiators: vec![
                "iqn.1991-05.com.microsoft:win-initiator".to_string(),
                "iqn.2001-04.com.example:linux-initiator".to_string(),
            ],
        };
        let commands = IscsiGenerator::generate_setup_commands(
            "res_iscsi",
            "/dev/drbd1",
            &config,
            "192.168.1.201",
        );

        assert_eq!(commands.len(), 7);
        assert_eq!(
            commands[0],
            "/backstores/block create name=bs_res_iscsi dev=/dev/drbd1"
        );
        assert_eq!(
            commands[1],
            "/iscsi create iqn.2025-01.com.example:iscsi-storage"
        );
        assert_eq!(
            commands[2],
            "/iscsi/iqn.2025-01.com.example:iscsi-storage/tpg1/portals create 192.168.1.201"
        );
        assert_eq!(commands[3], "/iscsi/iqn.2025-01.com.example:iscsi-storage/tpg1/luns create /backstores/block/bs_res_iscsi");
        assert_eq!(commands[4], "/iscsi/iqn.2025-01.com.example:iscsi-storage/tpg1/acls create iqn.1991-05.com.microsoft:win-initiator");
        assert_eq!(commands[5], "/iscsi/iqn.2025-01.com.example:iscsi-storage/tpg1/acls create iqn.2001-04.com.example:linux-initiator");
        assert_eq!(commands[6], "saveconfig");
    }

    #[test]
    fn test_generate_setup_commands_no_acl() {
        let config = IscsiConfig {
            iqn: "iqn.2025-01.com.example:iscsi-storage-noacl".to_string(),
            allowed_initiators: vec![],
        };
        let commands = IscsiGenerator::generate_setup_commands(
            "res_iscsi_noacl",
            "/dev/drbd2",
            &config,
            "192.168.1.202",
        );

        assert_eq!(commands.len(), 5); // No ACL commands
        assert_eq!(
            commands[0],
            "/backstores/block create name=bs_res_iscsi_noacl dev=/dev/drbd2"
        );
        assert_eq!(
            commands[1],
            "/iscsi create iqn.2025-01.com.example:iscsi-storage-noacl"
        );
        assert_eq!(commands[3], "/iscsi/iqn.2025-01.com.example:iscsi-storage-noacl/tpg1/luns create /backstores/block/bs_res_iscsi_noacl");
        assert_eq!(commands[4], "saveconfig");
    }

    #[test]
    fn test_generate_teardown_commands() {
        let config = IscsiConfig {
            iqn: "iqn.2025-01.com.example:iscsi-storage".to_string(),
            allowed_initiators: vec![],
        };
        let commands = IscsiGenerator::generate_teardown_commands("res_iscsi", &config);

        assert_eq!(commands.len(), 3);
        assert_eq!(
            commands[0],
            "delete /iscsi/iqn.2025-01.com.example:iscsi-storage"
        );
        assert_eq!(commands[1], "delete /backstores/block/bs_res_iscsi");
        assert_eq!(commands[2], "saveconfig");
    }
}
