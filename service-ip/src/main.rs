use anyhow::{Context, Result};
use clap::Parser;
use futures::stream::TryStreamExt;
use ipnetwork::IpNetwork;
use log::{error, info, warn};
use pnet::datalink::{self, Channel};
use pnet::packet::arp::{ArpHardwareTypes, ArpOperations, MutableArpPacket};
use pnet::packet::ethernet::{EtherTypes, MutableEthernetPacket};
use pnet::packet::{MutablePacket, Packet};
use pnet::util::MacAddr;
use rtnetlink::{new_connection, Error as NetlinkError};
use std::net::Ipv4Addr;
use std::num::NonZeroI32;
use std::time::Duration;
use tokio::signal;

#[derive(Parser, Debug)]
#[command(author, version, about = "Industrial Grade Service IP Manager")]
struct Args {
    /// VIP in CIDR format (e.g., 192.168.123.200/24)
    #[arg(long)]
    ip: String,

    /// Network Interface (e.g., eth0)
    #[arg(long)]
    dev: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    let args = Args::parse();

    let vip_net: IpNetwork = args.ip.parse().context("Invalid VIP format")?;
    let vip_addr = if let IpNetwork::V4(net) = vip_net {
        net.ip()
    } else {
        anyhow::bail!("Only IPv4 is supported");
    };

    info!("Starting Service-IP Manager for {} on {}", args.ip, args.dev);

    // 1. Get interface index
    let (connection, handle, _) = new_connection()?;
    tokio::spawn(connection);
    
    let link = handle
        .link()
        .get()
        .match_name(args.dev.clone())
        .execute()
        .try_next()
        .await?
        .context(format!("Interface {} not found", args.dev))?;
    let iface_idx = link.header.index;

    // 2. Start/Daemon loop
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    
    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Check if IP exists
                match check_ip_exists(&handle, iface_idx, vip_addr).await {
                    Ok(true) => {
                        // IP exists, everything is fine
                    },
                    Ok(false) => {
                        warn!("VIP {} missing on {}, adding it now...", vip_addr, args.dev);
                        if let Err(e) = add_ip(&handle, iface_idx, vip_net).await {
                            error!("Failed to add VIP: {}", e);
                        } else {
                            info!("VIP added. Sending Gratuitous ARP...");
                            if let Err(e) = send_gratuitous_arp(&args.dev, vip_addr) {
                                error!("Failed to send ARP: {}", e);
                            }
                        }
                    },
                    Err(e) => error!("Failed to check IP status: {}", e),
                }
            }
            
            // 3. Graceful shutdown signal handling
            _ = signal::ctrl_c() => {
                info!("Signal received, shutting down...");
                break;
            }
        }
    }

    // Cleanup: Remove IP before exit
    info!("Removing VIP {} before exit.", vip_addr);
    let _ = del_ip(&handle, iface_idx, vip_net).await;

    Ok(())
}

// --- Netlink Helpers ---

async fn check_ip_exists(handle: &rtnetlink::Handle, iface_idx: u32, ip: Ipv4Addr) -> Result<bool> {
    let mut addresses = handle
        .address()
        .get()
        .set_link_index_filter(iface_idx)
        .execute();

    while let Some(msg) = addresses.try_next().await? {
        for nla in msg.nlas {
            if let netlink_packet_route::nlas::address::Nla::Address(addr_vec) = nla {
                if addr_vec.len() == 4 {
                    let addr = Ipv4Addr::new(addr_vec[0], addr_vec[1], addr_vec[2], addr_vec[3]);
                    if addr == ip {
                        return Ok(true);
                    }
                }
            }
        }
    }
    Ok(false)
}

async fn add_ip(handle: &rtnetlink::Handle, iface_idx: u32, vip: IpNetwork) -> Result<()> {
    match handle.address().add(iface_idx, vip.ip(), vip.prefix()).execute().await {
        Ok(_) => Ok(()),
        Err(NetlinkError::NetlinkError(err)) if err.code == NonZeroI32::new(-17) => Ok(()), // EEXIST: Already exists
        Err(e) => Err(e.into()),
    }
}

async fn del_ip(handle: &rtnetlink::Handle, iface_idx: u32, vip: IpNetwork) -> Result<()> {
    let mut message = netlink_packet_route::address::AddressMessage::default();
    message.header.index = iface_idx;
    message.header.prefix_len = vip.prefix();
    message.header.family = 2; // AF_INET
    
    // Add the IP address to the message attributes
    let ip_bytes = if let std::net::IpAddr::V4(v4) = vip.ip() {
        v4.octets().to_vec()
    } else {
        anyhow::bail!("Only IPv4 is supported");
    };
    message.nlas.push(netlink_packet_route::nlas::address::Nla::Local(ip_bytes));

    match handle.address().del(message).execute().await {
        Ok(_) => Ok(()),
        Err(NetlinkError::NetlinkError(err)) if err.code == NonZeroI32::new(-99) => Ok(()), // EADDRNOTAVAIL
        Err(e) => Err(e.into()),
    }
}

// --- ARP Helper (Using pnet) ---

fn send_gratuitous_arp(iface_name: &str, vip: Ipv4Addr) -> Result<()> {
    let interface = datalink::interfaces()
        .into_iter()
        .find(|iface| iface.name == iface_name)
        .context("Interface not found for ARP")?;

    let (mut tx, _) = match datalink::channel(&interface, Default::default())? {
        Channel::Ethernet(tx, rx) => (tx, rx),
        _ => anyhow::bail!("Unsupported channel type"),
    };

    let mac = interface.mac.context("Interface has no MAC")?;
    let mut eth_buffer = [0u8; 42];
    let mut eth_packet = MutableEthernetPacket::new(&mut eth_buffer).unwrap();

    eth_packet.set_destination(MacAddr::broadcast());
    eth_packet.set_source(mac);
    eth_packet.set_ethertype(EtherTypes::Arp);

    let mut arp_buffer = [0u8; 28];
    let mut arp_packet = MutableArpPacket::new(&mut arp_buffer).unwrap();

    arp_packet.set_hardware_type(ArpHardwareTypes::Ethernet);
    arp_packet.set_protocol_type(EtherTypes::Ipv4);
    arp_packet.set_hw_addr_len(6);
    arp_packet.set_proto_addr_len(4);
    arp_packet.set_operation(ArpOperations::Request);
    arp_packet.set_sender_hw_addr(mac);
    arp_packet.set_sender_proto_addr(vip);
    arp_packet.set_target_hw_addr(MacAddr::broadcast());
    arp_packet.set_target_proto_addr(vip);

    eth_packet.set_payload(arp_packet.packet_mut());

    // Send 5 times to be sure
    for _ in 0..5 {
        tx.send_to(eth_packet.packet(), None);
        std::thread::sleep(Duration::from_millis(100));
    }
    
    Ok(())
}