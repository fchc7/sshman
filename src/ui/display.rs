use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, Color, Table};
use std::net::IpAddr;

use crate::models::connection::Connection;

pub fn print_connections(
    connections: &[&Connection],
    local_ips: Option<&[IpAddr]>,
    total: usize,
) {
    if connections.is_empty() {
        println!("No connections found.");
        return;
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.apply_modifier(UTF8_ROUND_CORNERS);

    let mut header = vec![
        Cell::new("#"),
        Cell::new("Alias"),
        Cell::new("Host"),
        Cell::new("User"),
        Cell::new("Port"),
        Cell::new("Tags"),
    ];
    if local_ips.is_some() {
        header.push(Cell::new("Subnet"));
    }
    table.set_header(header);

    for (i, conn) in connections.iter().enumerate() {
        let color = parse_color(conn.color.as_deref());

        let mut row = vec![
            Cell::new(i + 1),
            Cell::new(&conn.alias).fg(color),
            Cell::new(&conn.host).fg(color),
            Cell::new(&conn.user),
            Cell::new(conn.port),
            Cell::new(format_tags(&conn.tags)),
        ];
        if local_ips.is_some() {
            let subnet = if let Some(ips) = local_ips {
                conn.host.parse::<IpAddr>().ok().map(|target| {
                    ips.iter()
                        .find(|&local| crate::network::subnet::is_in_same_subnet(*local, target))
                        .map(|local| format_subnet(local))
                        .unwrap_or_default()
                })
            } else {
                None
            };
            row.push(Cell::new(subnet.unwrap_or_else(|| "-".to_string())));
        }
        table.add_row(row);
    }

    println!("{table}");

    if let Some(ips) = local_ips {
        let ip_list: Vec<String> = ips.iter().map(|ip| ip.to_string()).collect();
        println!("\nLocal IPs: {}", ip_list.join(", "));
    }

    if total > connections.len() {
        println!("{} of {} connections shown", connections.len(), total);
    } else {
        println!("{} connection(s) total", connections.len());
    }
}

fn format_tags(tags: &[String]) -> String {
    if tags.is_empty() {
        return "-".to_string();
    }
    format!("[{}]", tags.join(", "))
}

fn format_subnet(ip: &IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            format!("{}.{}.{}.0/24", o[0], o[1], o[2])
        }
        IpAddr::V6(_) => "IPv6".to_string(),
    }
}

fn parse_color(color: Option<&str>) -> Color {
    match color.map(|c| c.to_lowercase()).as_deref() {
        Some("red") => Color::Red,
        Some("green") => Color::Green,
        Some("yellow") => Color::Yellow,
        Some("blue") => Color::Blue,
        Some("magenta") | Some("purple") => Color::Magenta,
        Some("cyan") => Color::Cyan,
        Some("white") => Color::White,
        _ => Color::Reset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_tags_empty() {
        assert_eq!(format_tags(&[]), "-");
    }

    #[test]
    fn test_format_tags_single() {
        assert_eq!(format_tags(&["web".to_string()]), "[web]");
    }

    #[test]
    fn test_format_tags_multiple() {
        assert_eq!(
            format_tags(&["web".to_string(), "nginx".to_string()]),
            "[web, nginx]"
        );
    }

    #[test]
    fn test_format_subnet_v4() {
        let ip: IpAddr = "192.168.1.50".parse().unwrap();
        assert_eq!(format_subnet(&ip), "192.168.1.0/24");
    }

    #[test]
    fn test_format_subnet_another() {
        let ip: IpAddr = "10.0.0.10".parse().unwrap();
        assert_eq!(format_subnet(&ip), "10.0.0.0/24");
    }

    #[test]
    fn test_parse_color_red() {
        assert_eq!(parse_color(Some("red")), Color::Red);
    }

    #[test]
    fn test_parse_color_green() {
        assert_eq!(parse_color(Some("green")), Color::Green);
    }

    #[test]
    fn test_parse_color_case_insensitive() {
        assert_eq!(parse_color(Some("RED")), Color::Red);
        assert_eq!(parse_color(Some("Blue")), Color::Blue);
    }

    #[test]
    fn test_parse_color_none() {
        assert_eq!(parse_color(None), Color::Reset);
    }

    #[test]
    fn test_parse_color_purple() {
        assert_eq!(parse_color(Some("purple")), Color::Magenta);
    }

    #[test]
    fn test_parse_color_unknown() {
        assert_eq!(parse_color(Some("orange")), Color::Reset);
    }
}
