use std::net::{Ipv4Addr, Ipv6Addr};
use talpid_types::net::{Endpoint, TransportProtocol, wireguard};

/// Contains server data needed to connect to a single mullvad endpoint
#[derive(Debug, Clone)]
pub enum MullvadEndpoint {
    OpenVpn(Endpoint),
    Wireguard(MullvadWireguardEndpoint),
}

/// Contains WireGuard server data needed to connect to a WireGuard endpoint
#[derive(Debug, Clone)]
pub struct MullvadWireguardEndpoint {
    pub peer: wireguard::PeerConfig,
    pub exit_peer: Option<wireguard::PeerConfig>,
    pub ipv4_gateway: Ipv4Addr,
    pub ipv6_gateway: Ipv6Addr,
}

impl MullvadEndpoint {
    /// Returns this tunnel endpoint as an `Endpoint`.
    pub fn to_endpoint(&self) -> Endpoint {
        match self {
            MullvadEndpoint::OpenVpn(endpoint) => *endpoint,
            MullvadEndpoint::Wireguard(wireguard_relay) => Endpoint::new(
                wireguard_relay.peer.endpoint.ip(),
                wireguard_relay.peer.endpoint.port(),
                TransportProtocol::Udp,
            ),
        }
    }
}
