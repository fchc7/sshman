use std::net::{IpAddr, Ipv4Addr};

pub fn get_local_ips() -> Vec<IpAddr> {
    match local_ip_address::list_afinet_netifas() {
        Ok(interfaces) => interfaces
            .into_iter()
            .map(|(_, ip)| ip)
            .filter(|ip| !ip.is_unspecified() && !ip.is_loopback())
            .collect(),
        Err(_) => Vec::new(),
    }
}

pub fn is_in_same_subnet(a: IpAddr, b: IpAddr) -> bool {
    match (a, b) {
        (IpAddr::V4(a4), IpAddr::V4(b4)) => is_ipv4_same_class_c(a4, b4),
        (IpAddr::V6(_), IpAddr::V6(_)) => false,
        _ => false,
    }
}

fn is_ipv4_same_class_c(a: Ipv4Addr, b: Ipv4Addr) -> bool {
    let a_octets = a.octets();
    let b_octets = b.octets();
    a_octets[0] == b_octets[0] && a_octets[1] == b_octets[1] && a_octets[2] == b_octets[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_same_class_c() {
        let a: IpAddr = "192.168.1.1".parse().unwrap();
        let b: IpAddr = "192.168.1.100".parse().unwrap();
        assert!(is_in_same_subnet(a, b));
    }

    #[test]
    fn test_different_class_c() {
        let a: IpAddr = "192.168.1.1".parse().unwrap();
        let b: IpAddr = "192.168.2.1".parse().unwrap();
        assert!(!is_in_same_subnet(a, b));
    }

    #[test]
    fn test_different_class_b() {
        let a: IpAddr = "192.168.1.1".parse().unwrap();
        let b: IpAddr = "192.169.1.1".parse().unwrap();
        assert!(!is_in_same_subnet(a, b));
    }

    #[test]
    fn test_10_network_same() {
        let a: IpAddr = "10.0.0.1".parse().unwrap();
        let b: IpAddr = "10.0.0.200".parse().unwrap();
        assert!(is_in_same_subnet(a, b));
    }

    #[test]
    fn test_10_network_different() {
        let a: IpAddr = "10.0.0.1".parse().unwrap();
        let b: IpAddr = "10.0.1.1".parse().unwrap();
        assert!(!is_in_same_subnet(a, b));
    }

    #[test]
    fn test_172_network() {
        let a: IpAddr = "172.16.0.10".parse().unwrap();
        let b: IpAddr = "172.16.0.20".parse().unwrap();
        assert!(is_in_same_subnet(a, b));
    }

    #[test]
    fn test_loopback_filtered() {
        let ips = get_local_ips();
        assert!(!ips.iter().any(|ip| ip.is_loopback()));
    }

    #[test]
    fn test_unspecified_filtered() {
        let ips = get_local_ips();
        assert!(!ips.iter().any(|ip| ip.is_unspecified()));
    }

    #[test]
    fn test_ipv4_same_class_c_helper() {
        assert!(is_ipv4_same_class_c(
            Ipv4Addr::new(192, 168, 1, 1),
            Ipv4Addr::new(192, 168, 1, 254),
        ));
        assert!(!is_ipv4_same_class_c(
            Ipv4Addr::new(192, 168, 1, 1),
            Ipv4Addr::new(192, 168, 2, 1),
        ));
    }

    #[test]
    fn test_mixed_ip_versions() {
        let v4: IpAddr = "192.168.1.1".parse().unwrap();
        let v6: IpAddr = "::1".parse().unwrap();
        assert!(!is_in_same_subnet(v4, v6));
    }
}
