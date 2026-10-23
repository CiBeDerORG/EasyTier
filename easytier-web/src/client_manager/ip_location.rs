use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use maxminddb::{Reader, geoip2};
use url::{Host, Url};

use crate::db::{DeviceConnectionObservation, IpLocationStatus};

use super::session::Location;

fn connection_ip(client_url: &Url) -> Option<IpAddr> {
    let ip = match client_url.host()? {
        Host::Ipv4(ip) => IpAddr::V4(ip),
        Host::Ipv6(ip) => IpAddr::V6(ip),
        // Non-special URL schemes (including tcp/udp) may represent an IPv4
        // literal as a Domain. Parse only literals; never resolve client DNS.
        Host::Domain(host) => host.parse().ok()?,
    };
    Some(match ip {
        IpAddr::V6(ip) => ip
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(ip)),
        ip => ip,
    })
}

fn non_public_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, d] = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_documentation()
        || a == 0
        || a >= 224 // Multicast, reserved and limited broadcast.
        || (a == 100 && (64..=127).contains(&b)) // Shared CGNAT space.
        || (a == 198 && (18..=19).contains(&b)) // Benchmarking.
        || (a == 192 && b == 0 && c == 0 && !matches!(d, 9 | 10))
        || (a == 192 && b == 88 && c == 99) // Deprecated 6to4 relay anycast.
}

fn non_public_ipv6(ip: Ipv6Addr) -> bool {
    // Special-use ranges that must not be interpreted as a node's geographic
    // location. Keep ordinary global unicast addresses eligible for MMDB.
    const SPECIAL_PREFIXES: &[(u128, u32)] = &[
        (0, 96), // Unspecified, loopback and deprecated IPv4-compatible space.
        (0x0100_0000_0000_0000_0000_0000_0000_0000, 64), // Discard-only.
        (0x0100_0000_0000_0001_0000_0000_0000_0000, 64), // Dummy prefix.
        (0x0064_ff9b_0001_0000_0000_0000_0000_0000, 48), // Local-use translation.
        (0x2001_0002_0000_0000_0000_0000_0000_0000, 48), // Benchmarking.
        (0x2001_0010_0000_0000_0000_0000_0000_0000, 28), // ORCHID (deprecated).
        (0x2001_0020_0000_0000_0000_0000_0000_0000, 28), // ORCHIDv2.
        (0x2001_0db8_0000_0000_0000_0000_0000_0000, 32), // Documentation.
        (0x3fff_0000_0000_0000_0000_0000_0000_0000, 20), // Documentation.
        (0x5f00_0000_0000_0000_0000_0000_0000_0000, 16), // Local SRv6 SIDs.
        (0xfc00_0000_0000_0000_0000_0000_0000_0000, 7), // Unique-local.
        (0xfe80_0000_0000_0000_0000_0000_0000_0000, 10), // Link-local.
        (0xfec0_0000_0000_0000_0000_0000_0000_0000, 10), // Deprecated site-local.
        (0xff00_0000_0000_0000_0000_0000_0000_0000, 8), // Multicast.
    ];
    let bits = u128::from(ip);
    SPECIAL_PREFIXES
        .iter()
        .any(|&(prefix, length)| bits >> (128 - length) == prefix >> (128 - length))
}

fn preferred_name(chinese: Option<&str>, english: Option<&str>) -> Option<String> {
    chinese
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .or_else(|| english.map(str::trim).filter(|name| !name.is_empty()))
        .map(str::to_owned)
}

pub(super) fn lookup_connection_observation(
    client_url: &Url,
    geoip_db: &Option<Reader<Vec<u8>>>,
) -> Option<DeviceConnectionObservation> {
    let ip = connection_ip(client_url)?;
    let mut observation = DeviceConnectionObservation {
        connection_ip: ip.to_string(),
        ip_country: None,
        ip_region: None,
        ip_city: None,
        ip_location_status: IpLocationStatus::Unavailable,
        ip_observed_at: chrono::Utc::now().fixed_offset(),
    };
    if match ip {
        IpAddr::V4(ip) => non_public_ipv4(ip),
        IpAddr::V6(ip) => non_public_ipv6(ip),
    } {
        observation.ip_location_status = IpLocationStatus::NonPublic;
        return Some(observation);
    }
    let Some(db) = geoip_db else {
        return Some(observation);
    };
    match db
        .lookup(ip)
        .and_then(|result| result.decode::<geoip2::City>())
    {
        Ok(Some(city)) => {
            observation.ip_country = preferred_name(
                city.country.names.simplified_chinese,
                city.country.names.english,
            );
            observation.ip_city =
                preferred_name(city.city.names.simplified_chinese, city.city.names.english);
            let regions: Vec<_> = city
                .subdivisions
                .iter()
                .filter_map(|region| {
                    preferred_name(region.names.simplified_chinese, region.names.english)
                })
                .collect();
            observation.ip_region = (!regions.is_empty()).then(|| regions.join(","));
            observation.ip_location_status = if observation.ip_country.is_some()
                || observation.ip_region.is_some()
                || observation.ip_city.is_some()
            {
                IpLocationStatus::Resolved
            } else {
                IpLocationStatus::NotFound
            };
        }
        Ok(None) => observation.ip_location_status = IpLocationStatus::NotFound,
        Err(error) => {
            tracing::debug!(%ip, %error, "GeoIP lookup failed");
        }
    }
    Some(observation)
}

pub(super) fn legacy_location(observation: &DeviceConnectionObservation) -> Option<Location> {
    // The existing API requires a country string. Keep partial region/city
    // results in the new fields instead of inventing a country placeholder.
    Some(Location {
        country: observation.ip_country.clone()?,
        city: observation.ip_city.clone(),
        region: observation.ip_region.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_address_is_normalized_without_resolving_hostnames() {
        for (url, expected) in [
            ("tcp://8.8.8.8:22020", "8.8.8.8"),
            ("udp://[2001:4860:4860::8888]:22020", "2001:4860:4860::8888"),
            ("tcp://[::ffff:192.168.1.2]:22020", "192.168.1.2"),
        ] {
            let observation = lookup_connection_observation(&url.parse().unwrap(), &None).unwrap();
            assert_eq!(observation.connection_ip, expected);
        }
        assert!(
            lookup_connection_observation(&"tcp://node.example:22020".parse().unwrap(), &None)
                .is_none()
        );
    }

    #[test]
    fn special_addresses_are_not_geolocated_even_without_a_database() {
        for address in [
            "0.1.2.3",
            "10.0.0.1",
            "100.64.0.1",
            "100.127.255.254",
            "127.0.0.1",
            "169.254.1.1",
            "172.16.0.1",
            "172.31.255.254",
            "192.0.0.8",
            "192.0.2.1",
            "192.168.1.1",
            "198.18.0.1",
            "198.19.255.254",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "240.0.0.1",
            "255.255.255.255",
            "[::]",
            "[::1]",
            "[::ffff:10.0.0.1]",
            "[64:ff9b:1::1]",
            "[100::1]",
            "[100:0:0:1::1]",
            "[2001:2::1]",
            "[2001:db8::1]",
            "[3fff::1]",
            "[fc00::1]",
            "[fd12::1]",
            "[fe80::1]",
            "[fec0::1]",
            "[ff02::1]",
        ] {
            let url = format!("tcp://{address}:22020").parse().unwrap();
            let observation = lookup_connection_observation(&url, &None).unwrap();
            assert_eq!(
                observation.ip_location_status,
                IpLocationStatus::NonPublic,
                "{address}"
            );
            assert!(observation.ip_country.is_none());
            assert!(observation.ip_region.is_none());
            assert!(observation.ip_city.is_none());
            assert!(legacy_location(&observation).is_none());
        }
    }

    #[test]
    fn public_addresses_without_database_are_unavailable_not_overseas() {
        for address in [
            "8.8.8.8",
            "100.63.255.254",
            "100.128.0.1",
            "172.15.255.254",
            "172.32.0.1",
            "192.0.0.9",
            "192.0.0.10",
            "[2001:4860:4860::8888]",
        ] {
            let url = format!("udp://{address}:22020").parse().unwrap();
            let observation = lookup_connection_observation(&url, &None).unwrap();
            assert_eq!(
                observation.ip_location_status,
                IpLocationStatus::Unavailable,
                "{address}"
            );
            assert!(legacy_location(&observation).is_none());
        }
    }

    #[test]
    fn embedded_country_database_does_not_invent_city_information() {
        let db = super::super::load_geoip_db(None);
        assert!(db.is_some());
        let observation =
            lookup_connection_observation(&"tcp://114.114.114.114:22020".parse().unwrap(), &db)
                .unwrap();
        assert_eq!(observation.ip_location_status, IpLocationStatus::Resolved);
        assert_eq!(observation.ip_country.as_deref(), Some("中国"));
        assert!(observation.ip_region.is_none());
        assert!(observation.ip_city.is_none());
        assert_eq!(legacy_location(&observation).unwrap().country, "中国");
    }

    #[test]
    fn embedded_country_database_miss_is_not_found() {
        let db = super::super::load_geoip_db(None);
        // The embedded geoip2-cn file contains Chinese address ranges only.
        let observation =
            lookup_connection_observation(&"udp://8.8.8.8:22020".parse().unwrap(), &db).unwrap();
        assert_eq!(observation.ip_location_status, IpLocationStatus::NotFound);
        assert!(observation.ip_country.is_none());
        assert!(legacy_location(&observation).is_none());
    }

    #[test]
    fn empty_localized_names_fall_back_without_fabricating_values() {
        assert_eq!(
            preferred_name(Some("  "), Some(" China ")).as_deref(),
            Some("China")
        );
        assert_eq!(
            preferred_name(Some("中国"), Some("China")).as_deref(),
            Some("中国")
        );
        assert_eq!(preferred_name(None, Some(" ")), None);
    }
}
