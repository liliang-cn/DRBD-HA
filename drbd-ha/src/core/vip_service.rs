use crate::models::VipConfig;

/// Generator for service-ip systemd units
pub struct VipServiceGenerator;

impl VipServiceGenerator {
    /// Generate the content of the systemd unit file
    pub fn generate_content(resource_name: &str, vip: &VipConfig) -> String {
        format!(r#"[Unit]
Description=Service IP Manager for {} ({})
BindsTo=sys-subsystem-net-devices-{}.device
After=sys-subsystem-net-devices-{}.device

[Service]
Type=simple
ExecStart=/usr/local/bin/service-ip --ip {} --dev {}
Restart=always
RestartSec=1s
OOMScoreAdjust=-1000
CPUSchedulingPolicy=fifo
CPUSchedulingPriority=50

[Install]
WantedBy=multi-user.target
"#,
            vip.cidr(),
            resource_name,
            vip.interface,
            vip.interface,
            vip.cidr(),
            vip.interface
        )
    }

    /// Get the service name for a given resource
    pub fn service_name(resource_name: &str) -> String {
        format!("service-ip-{}.service", resource_name)
    }
    
    /// Get the path to the systemd unit file
    pub fn unit_path(resource_name: &str) -> String {
        format!("/etc/systemd/system/{}", Self::service_name(resource_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_content() {
        let vip = VipConfig {
            address: "192.168.1.100".to_string(),
            netmask: 24,
            interface: "eth0".to_string(),
        };
        let content = VipServiceGenerator::generate_content("r0", &vip);
        assert!(content.contains("Description=Service IP Manager for 192.168.1.100/24 (r0)"));
        assert!(content.contains("ExecStart=/usr/local/bin/service-ip --ip 192.168.1.100/24 --dev eth0"));
        assert!(content.contains("BindsTo=sys-subsystem-net-devices-eth0.device"));
    }
    
    #[test]
    fn test_service_name() {
        assert_eq!(VipServiceGenerator::service_name("r0"), "service-ip-r0.service");
    }
}
