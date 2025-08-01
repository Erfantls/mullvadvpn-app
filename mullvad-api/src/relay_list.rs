//! A module dedicated to retrieving the relay list from the Mullvad API.

use crate::rest;

use hyper::{StatusCode, body::Incoming, header};
use mullvad_types::{location, relay_list};
use talpid_types::net::wireguard;

use std::{
    collections::{BTreeMap, HashSet},
    future::Future,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    ops::RangeInclusive,
    time::Duration,
};

/// Fetches relay list from <https://api.mullvad.net/app/v1/relays>
#[derive(Clone)]
pub struct RelayListProxy {
    handle: rest::MullvadRestHandle,
}

const RELAY_LIST_TIMEOUT: Duration = Duration::from_secs(15);

impl RelayListProxy {
    /// Construct a new relay list rest client
    pub fn new(handle: rest::MullvadRestHandle) -> Self {
        Self { handle }
    }

    /// Fetch the relay list
    pub fn relay_list(
        &self,
        etag: Option<String>,
    ) -> impl Future<Output = Result<Option<relay_list::RelayList>, rest::Error>> {
        let request = self.relay_list_response(etag.clone());

        async move {
            let response = request.await?;

            if etag.is_some() && response.status() == StatusCode::NOT_MODIFIED {
                return Ok(None);
            }

            let etag = Self::extract_etag(&response);

            let relay_list: ServerRelayList = response.deserialize().await?;
            Ok(Some(relay_list.into_relay_list(etag)))
        }
    }

    pub fn relay_list_response(
        &self,
        etag: Option<String>,
    ) -> impl Future<Output = Result<rest::Response<Incoming>, rest::Error>> {
        let service = self.handle.service.clone();
        let request = self.handle.factory.get("app/v1/relays");

        async move {
            let mut request = request?
                .timeout(RELAY_LIST_TIMEOUT)
                .expected_status(&[StatusCode::NOT_MODIFIED, StatusCode::OK]);

            if let Some(ref tag) = etag {
                request = request.header(header::IF_NONE_MATCH, tag)?;
            }

            let response = service.request(request).await?;

            Ok(response)
        }
    }

    pub fn extract_etag(response: &rest::Response<Incoming>) -> Option<String> {
        response
            .headers()
            .get(header::ETAG)
            .and_then(|tag| match tag.to_str() {
                Ok(tag) => Some(tag.to_string()),
                Err(_) => {
                    log::error!("Ignoring invalid tag from server: {:?}", tag.as_bytes());
                    None
                }
            })
    }
}

#[derive(Debug, serde::Deserialize)]
struct ServerRelayList {
    locations: BTreeMap<String, Location>,
    openvpn: OpenVpn,
    wireguard: Wireguard,
    bridge: Bridges,
}

impl ServerRelayList {
    fn into_relay_list(self, etag: Option<String>) -> relay_list::RelayList {
        let mut countries = BTreeMap::new();
        let Self {
            locations,
            openvpn,
            wireguard,
            bridge,
        } = self;

        for (code, location) in locations.into_iter() {
            match split_location_code(&code) {
                Some((country_code, city_code)) => {
                    let country_code = country_code.to_lowercase();
                    let city_code = city_code.to_lowercase();
                    let country = countries
                        .entry(country_code.clone())
                        .or_insert_with(|| location_to_country(&location, country_code));
                    country.cities.push(location_to_city(&location, city_code));
                }
                None => {
                    log::error!("Bad location code:{}", code);
                    continue;
                }
            }
        }

        relay_list::RelayList {
            etag: etag.map(|mut tag| {
                if tag.starts_with('"') {
                    tag.insert_str(0, "W/");
                }
                tag
            }),
            openvpn: openvpn.extract_relays(&mut countries),
            wireguard: wireguard.extract_relays(&mut countries),
            bridge: bridge.extract_relays(&mut countries),
            countries: countries.into_values().collect(),
        }
    }
}

/// Splits a location code into a country code and a city code. The input is expected to be in a
/// format like `se-mma`, with `se` being the country code, `mma` being the city code.
fn split_location_code(location: &str) -> Option<(&str, &str)> {
    let mut parts = location.split('-');
    let country = parts.next()?;
    let city = parts.next()?;

    Some((country, city))
}

fn location_to_country(location: &Location, code: String) -> relay_list::RelayListCountry {
    relay_list::RelayListCountry {
        cities: vec![],
        name: location.country.clone(),
        code,
    }
}

fn location_to_city(location: &Location, code: String) -> relay_list::RelayListCity {
    relay_list::RelayListCity {
        name: location.city.clone(),
        code,
        latitude: location.latitude,
        longitude: location.longitude,
        relays: vec![],
    }
}

fn into_mullvad_relay(
    relay: Relay,
    location: location::Location,
    endpoint_data: relay_list::RelayEndpointData,
) -> relay_list::Relay {
    relay_list::Relay {
        hostname: relay.hostname,
        ipv4_addr_in: relay.ipv4_addr_in,
        ipv6_addr_in: relay.ipv6_addr_in,
        overridden_ipv4: false,
        overridden_ipv6: false,
        include_in_country: relay.include_in_country,
        active: relay.active,
        owned: relay.owned,
        provider: relay.provider,
        weight: relay.weight,
        endpoint_data,
        location,
    }
}

#[derive(Debug, serde::Deserialize)]
struct Location {
    city: String,
    country: String,
    latitude: f64,
    longitude: f64,
}

#[derive(Debug, serde::Deserialize)]
struct OpenVpn {
    #[serde(flatten)]
    ports: relay_list::OpenVpnEndpointData,
    relays: Vec<Relay>,
}

impl OpenVpn {
    /// Consumes `self` and appends all its relays to `countries`.
    fn extract_relays(
        self,
        countries: &mut BTreeMap<String, relay_list::RelayListCountry>,
    ) -> relay_list::OpenVpnEndpointData {
        for mut openvpn_relay in self.relays.into_iter() {
            openvpn_relay.convert_to_lowercase();
            if let Some((country_code, city_code)) = split_location_code(&openvpn_relay.location)
                && let Some(country) = countries.get_mut(country_code)
                && let Some(city) = country
                    .cities
                    .iter_mut()
                    .find(|city| city.code == city_code)
            {
                let location = location::Location {
                    country: country.name.clone(),
                    country_code: country.code.clone(),
                    city: city.name.clone(),
                    city_code: city.code.clone(),
                    latitude: city.latitude,
                    longitude: city.longitude,
                };
                let relay = openvpn_relay.into_openvpn_mullvad_relay(location);
                city.relays.push(relay);
            };
        }
        self.ports
    }
}

#[derive(Debug, serde::Deserialize)]
struct Relay {
    hostname: String,
    active: bool,
    owned: bool,
    location: String,
    provider: String,
    ipv4_addr_in: Ipv4Addr,
    ipv6_addr_in: Option<Ipv6Addr>,
    weight: u64,
    include_in_country: bool,
}

impl Relay {
    fn into_openvpn_mullvad_relay(self, location: location::Location) -> relay_list::Relay {
        into_mullvad_relay(self, location, relay_list::RelayEndpointData::Openvpn)
    }

    fn into_bridge_mullvad_relay(self, location: location::Location) -> relay_list::Relay {
        into_mullvad_relay(self, location, relay_list::RelayEndpointData::Bridge)
    }

    fn convert_to_lowercase(&mut self) {
        self.hostname = self.hostname.to_lowercase();
        self.location = self.location.to_lowercase();
    }
}

#[derive(Debug, serde::Deserialize)]
struct Wireguard {
    port_ranges: Vec<(u16, u16)>,
    ipv4_gateway: Ipv4Addr,
    ipv6_gateway: Ipv6Addr,
    /// Shadowsocks port ranges available on all WireGuard relays
    #[serde(default)]
    shadowsocks_port_ranges: Vec<(u16, u16)>,
    relays: Vec<WireGuardRelay>,
}

impl From<&Wireguard> for relay_list::WireguardEndpointData {
    fn from(wg: &Wireguard) -> Self {
        Self {
            port_ranges: inclusive_range_from_pair_set(wg.port_ranges.clone()).collect(),
            ipv4_gateway: wg.ipv4_gateway,
            ipv6_gateway: wg.ipv6_gateway,
            shadowsocks_port_ranges: inclusive_range_from_pair_set(
                wg.shadowsocks_port_ranges.clone(),
            )
            .collect(),
            udp2tcp_ports: vec![],
        }
    }
}

fn inclusive_range_from_pair_set<T>(
    set: impl IntoIterator<Item = (T, T)>,
) -> impl Iterator<Item = RangeInclusive<T>> {
    set.into_iter().map(inclusive_range_from_pair)
}

fn inclusive_range_from_pair<T>(pair: (T, T)) -> RangeInclusive<T> {
    RangeInclusive::new(pair.0, pair.1)
}

impl Wireguard {
    /// Consumes `self` and appends all its relays to `countries`.
    fn extract_relays(
        self,
        countries: &mut BTreeMap<String, relay_list::RelayListCountry>,
    ) -> relay_list::WireguardEndpointData {
        let endpoint_data = relay_list::WireguardEndpointData::from(&self);
        let relays = self.relays;

        for mut wireguard_relay in relays {
            wireguard_relay.relay.convert_to_lowercase();
            if let Some((country_code, city_code)) =
                split_location_code(&wireguard_relay.relay.location)
                && let Some(country) = countries.get_mut(country_code)
                && let Some(city) = country
                    .cities
                    .iter_mut()
                    .find(|city| city.code == city_code)
            {
                let location = location::Location {
                    country: country.name.clone(),
                    country_code: country.code.clone(),
                    city: city.name.clone(),
                    city_code: city.code.clone(),
                    latitude: city.latitude,
                    longitude: city.longitude,
                };

                let relay = wireguard_relay.into_mullvad_relay(location);
                city.relays.push(relay);
            };
        }

        endpoint_data
    }
}

#[derive(Debug, serde::Deserialize)]
struct WireGuardRelay {
    #[serde(flatten)]
    relay: Relay,
    public_key: wireguard::PublicKey,
    #[serde(default)]
    daita: bool,
    #[serde(default)]
    shadowsocks_extra_addr_in: Vec<IpAddr>,
    #[serde(default)]
    features: Features,
}

impl WireGuardRelay {
    fn into_mullvad_relay(self, location: location::Location) -> relay_list::Relay {
        // Sanity check that new 'features' key is in sync with the old, superceded keys.
        // TODO: Remove `self.daita` (and this check 👇) when `features` key has been completely
        // rolled out to production.
        if self.features.daita.is_some() {
            debug_assert!(self.daita)
        }

        let relay = self.relay;
        let endpoint_data =
            relay_list::RelayEndpointData::Wireguard(relay_list::WireguardRelayEndpointData {
                public_key: self.public_key,
                // FIXME: This hack is forward-compatible with 'features' being rolled out.
                //        Should unwrap to 'false' once 'daita' field is removed.
                daita: self.features.daita.map(|_| true).unwrap_or(self.daita),
                shadowsocks_extra_addr_in: HashSet::from_iter(self.shadowsocks_extra_addr_in),
                quic: self.features.quic.map(relay_list::Quic::from),
            });

        into_mullvad_relay(relay, location, endpoint_data)
    }
}

/// Extra features enabled on some (Wireguard) relay, such as obfuscation daemons or Daita.
#[derive(Debug, Default, Clone, serde::Deserialize)]
struct Features {
    daita: Option<Daita>,
    quic: Option<Quic>,
}

/// DAITA doesn't have any configuration options (exposed by the API).
///
/// Note, an empty struct is not the same as an empty tuple struct according to serde_json!
#[derive(Debug, Clone, serde::Deserialize)]
struct Daita {}

/// Parameters for setting up a QUIC obfuscator (connecting to a masque-proxy running on a relay).
#[derive(Debug, Clone, serde::Deserialize)]
struct Quic {
    /// In-addresses for the QUIC obfuscator.
    ///
    /// There may be 0, 1 or 2 in IPs, depending on how many masque-proxy daemons running on the
    /// relay. Hopefully the API will tell use the correct amount🤞.
    addr_in: Vec<IpAddr>,
    /// Authorization token
    token: String,
    /// Hostname where masque proxy is hosted
    domain: String,
}

impl From<Quic> for relay_list::Quic {
    fn from(value: Quic) -> Self {
        Self::new(value.addr_in, value.token, value.domain)
    }
}

#[derive(Debug, serde::Deserialize)]
struct Bridges {
    shadowsocks: Vec<relay_list::ShadowsocksEndpointData>,
    relays: Vec<Relay>,
}

impl Bridges {
    /// Consumes `self` and appends all its relays to `countries`.
    fn extract_relays(
        self,
        countries: &mut BTreeMap<String, relay_list::RelayListCountry>,
    ) -> relay_list::BridgeEndpointData {
        for mut bridge_relay in self.relays {
            bridge_relay.convert_to_lowercase();
            if let Some((country_code, city_code)) = split_location_code(&bridge_relay.location)
                && let Some(country) = countries.get_mut(country_code)
                && let Some(city) = country
                    .cities
                    .iter_mut()
                    .find(|city| city.code == city_code)
            {
                let location = location::Location {
                    country: country.name.clone(),
                    country_code: country.code.clone(),
                    city: city.name.clone(),
                    city_code: city.code.clone(),
                    latitude: city.latitude,
                    longitude: city.longitude,
                };

                let relay = bridge_relay.into_bridge_mullvad_relay(location);
                city.relays.push(relay);
            };
        }

        relay_list::BridgeEndpointData {
            shadowsocks: self.shadowsocks,
        }
    }
}
