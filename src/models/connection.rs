use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

use crate::network::subnet::is_in_same_subnet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Connection {
    pub alias: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub tags: Vec<String>,
    pub color: Option<String>,
    pub created_at: DateTime<Local>,
    pub last_connected: Option<DateTime<Local>>,
}

impl Connection {
    pub fn new(alias: String, host: String, port: u16, user: String) -> Self {
        Self {
            alias,
            host,
            port,
            user,
            tags: Vec::new(),
            color: None,
            created_at: Local::now(),
            last_connected: None,
        }
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_color(mut self, color: String) -> Self {
        self.color = Some(color);
        self
    }

    pub fn display_host(&self) -> String {
        format!("{}@{}:{}", self.user, self.host, self.port)
    }

    pub fn matches_keyword(&self, keyword: &str) -> bool {
        let kw = keyword.to_lowercase();
        self.alias.to_lowercase().contains(&kw)
            || self.host.to_lowercase().contains(&kw)
            || self.user.to_lowercase().contains(&kw)
            || self
                .tags
                .iter()
                .any(|t| t.to_lowercase().contains(&kw))
    }

    pub fn matches_tag(&self, tag: &str) -> bool {
        self.tags
            .iter()
            .any(|t| t.eq_ignore_ascii_case(tag))
    }

    pub fn is_local_network(&self, local_ips: &[IpAddr]) -> bool {
        let Ok(target) = self.host.parse::<IpAddr>() else {
            return false;
        };
        local_ips.iter().any(|local_ip| is_in_same_subnet(*local_ip, target))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ConnectionStore {
    pub connections: Vec<Connection>,
}

impl ConnectionStore {
    pub fn new() -> Self {
        Self {
            connections: Vec::new(),
        }
    }

    pub fn add(&mut self, conn: Connection) -> Result<(), String> {
        if self.find(&conn.alias).is_some() {
            return Err(format!("alias '{}' already exists", conn.alias));
        }
        self.connections.push(conn);
        Ok(())
    }

    pub fn remove(&mut self, alias: &str) -> Result<Connection, String> {
        let idx = self
            .connections
            .iter()
            .position(|c| c.alias == alias)
            .ok_or_else(|| format!("alias '{}' not found", alias))?;
        Ok(self.connections.remove(idx))
    }

    pub fn find(&self, alias: &str) -> Option<&Connection> {
        self.connections.iter().find(|c| c.alias == alias)
    }

    pub fn find_mut(&mut self, alias: &str) -> Option<&mut Connection> {
        self.connections.iter_mut().find(|c| c.alias == alias)
    }

    pub fn search(&self, keyword: &str) -> Vec<&Connection> {
        self.connections
            .iter()
            .filter(|c| c.matches_keyword(keyword))
            .collect()
    }

    pub fn filter_by_tag(&self, tag: &str) -> Vec<&Connection> {
        self.connections
            .iter()
            .filter(|c| c.matches_tag(tag))
            .collect()
    }

    pub fn filter_local(&self, local_ips: &[IpAddr]) -> Vec<&Connection> {
        self.connections
            .iter()
            .filter(|c| c.is_local_network(local_ips))
            .collect()
    }

    pub fn list_all_tags(&self) -> Vec<String> {
        let mut tags: Vec<String> = self
            .connections
            .iter()
            .flat_map(|c| c.tags.clone())
            .collect();
        tags.sort();
        tags.dedup();
        tags
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_conn(alias: &str, host: &str) -> Connection {
        Connection::new(alias.to_string(), host.to_string(), 22, "root".to_string())
    }

    fn make_conn_full(alias: &str, host: &str, tags: Vec<&str>, color: Option<&str>) -> Connection {
        let mut conn = Connection::new(alias.to_string(), host.to_string(), 22, "root".to_string());
        conn.tags = tags.into_iter().map(String::from).collect();
        conn.color = color.map(String::from);
        conn
    }

    #[test]
    fn test_connection_new() {
        let conn = Connection::new("test".into(), "1.2.3.4".into(), 22, "admin".into());
        assert_eq!(conn.alias, "test");
        assert_eq!(conn.host, "1.2.3.4");
        assert_eq!(conn.port, 22);
        assert_eq!(conn.user, "admin");
        assert!(conn.tags.is_empty());
        assert_eq!(conn.color, None);
        assert!(conn.last_connected.is_none());
    }

    #[test]
    fn test_builder_pattern() {
        let conn = make_conn_full("app", "10.0.0.1", vec!["web", "nginx"], Some("red"));
        assert_eq!(conn.tags, vec!["web", "nginx"]);
        assert_eq!(conn.color.as_deref(), Some("red"));
    }

    #[test]
    fn test_display_host() {
        let conn = Connection::new("db".into(), "192.168.1.100".into(), 3306, "admin".into());
        assert_eq!(conn.display_host(), "admin@192.168.1.100:3306");
    }

    #[test]
    fn test_matches_keyword_by_alias() {
        let conn = make_conn("prod-web-01", "10.0.0.1");
        assert!(conn.matches_keyword("prod"));
        assert!(conn.matches_keyword("WEB"));
        assert!(!conn.matches_keyword("staging"));
    }

    #[test]
    fn test_matches_keyword_by_host() {
        let conn = make_conn("app", "192.168.1.100");
        assert!(conn.matches_keyword("192.168"));
        assert!(conn.matches_keyword("1.100"));
    }

    #[test]
    fn test_matches_keyword_by_user() {
        let conn = Connection::new("app".into(), "1.1.1.1".into(), 22, "deployer".into());
        assert!(conn.matches_keyword("deploy"));
    }

    #[test]
    fn test_matches_keyword_by_tag() {
        let conn = make_conn_full("app", "1.1.1.1", vec!["nginx", "web"], None);
        assert!(conn.matches_keyword("nginx"));
        assert!(conn.matches_keyword("WEB"));
    }

    #[test]
    fn test_matches_tag() {
        let conn = make_conn_full("app", "1.1.1.1", vec!["web", "nginx"], None);
        assert!(conn.matches_tag("web"));
        assert!(conn.matches_tag("WEB"));
        assert!(conn.matches_tag("nginx"));
        assert!(!conn.matches_tag("mysql"));
    }

    #[test]
    fn test_is_local_network_same_subnet() {
        let conn = make_conn("app", "192.168.1.100");
        let local_ips: Vec<IpAddr> = vec!["192.168.1.50".parse().unwrap()];
        assert!(conn.is_local_network(&local_ips));
    }

    #[test]
    fn test_is_local_network_different_subnet() {
        let conn = make_conn("app", "10.0.0.100");
        let local_ips: Vec<IpAddr> = vec!["192.168.1.50".parse().unwrap()];
        assert!(!conn.is_local_network(&local_ips));
    }

    #[test]
    fn test_is_local_network_invalid_host() {
        let mut conn = make_conn("app", "example.com");
        conn.host = "example.com".into();
        let local_ips: Vec<IpAddr> = vec!["192.168.1.50".parse().unwrap()];
        assert!(!conn.is_local_network(&local_ips));
    }

    #[test]
    fn test_store_add_and_find() {
        let mut store = ConnectionStore::new();
        let conn = make_conn("app", "10.0.0.1");
        store.add(conn).unwrap();

        let found = store.find("app").unwrap();
        assert_eq!(found.alias, "app");
        assert_eq!(found.host, "10.0.0.1");
    }

    #[test]
    fn test_store_add_duplicate_alias() {
        let mut store = ConnectionStore::new();
        store.add(make_conn("app", "10.0.0.1")).unwrap();
        let result = store.add(make_conn("app", "10.0.0.2"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already exists"));
    }

    #[test]
    fn test_store_remove() {
        let mut store = ConnectionStore::new();
        store.add(make_conn("app", "10.0.0.1")).unwrap();
        let removed = store.remove("app").unwrap();
        assert_eq!(removed.alias, "app");
        assert!(store.find("app").is_none());
    }

    #[test]
    fn test_store_remove_not_found() {
        let mut store = ConnectionStore::new();
        let result = store.remove("nonexist");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_store_find_not_found() {
        let store = ConnectionStore::new();
        assert!(store.find("nothing").is_none());
    }

    #[test]
    fn test_store_find_mut() {
        let mut store = ConnectionStore::new();
        store.add(make_conn("app", "10.0.0.1")).unwrap();
        let conn = store.find_mut("app").unwrap();
        conn.host = "10.0.0.2".to_string();
        assert_eq!(store.find("app").unwrap().host, "10.0.0.2");
    }

    #[test]
    fn test_store_search() {
        let mut store = ConnectionStore::new();
        store.add(make_conn("prod-web-01", "192.168.1.100")).unwrap();
        store.add(make_conn("staging-db", "10.0.0.5")).unwrap();
        store.add(make_conn("dev-app", "172.16.0.10")).unwrap();

        let results = store.search("prod");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].alias, "prod-web-01");

        let results = store.search("192.168");
        assert_eq!(results.len(), 1);

        let results = store.search("nothing");
        assert!(results.is_empty());
    }

    #[test]
    fn test_store_filter_by_tag() {
        let mut store = ConnectionStore::new();
        store.add(make_conn_full("web1", "10.0.0.1", vec!["web", "nginx"], None)).unwrap();
        store.add(make_conn_full("db1", "10.0.0.2", vec!["mysql"], None)).unwrap();
        store.add(make_conn_full("web2", "10.0.0.3", vec!["web"], None)).unwrap();

        let results = store.filter_by_tag("web");
        assert_eq!(results.len(), 2);

        let results = store.filter_by_tag("nginx");
        assert_eq!(results.len(), 1);

        let results = store.filter_by_tag("redis");
        assert!(results.is_empty());
    }

    #[test]
    fn test_store_filter_local() {
        let mut store = ConnectionStore::new();
        store.add(make_conn("local1", "192.168.1.100")).unwrap();
        store.add(make_conn("remote1", "8.8.8.8")).unwrap();
        store.add(make_conn("local2", "192.168.1.200")).unwrap();

        let local_ips: Vec<IpAddr> = vec!["192.168.1.50".parse().unwrap()];
        let results = store.filter_local(&local_ips);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_store_list_all_tags() {
        let mut store = ConnectionStore::new();
        store.add(make_conn_full("a", "1.1.1.1", vec!["web", "nginx"], None)).unwrap();
        store.add(make_conn_full("b", "2.2.2.2", vec!["web", "mysql"], None)).unwrap();

        let tags = store.list_all_tags();
        assert_eq!(tags, vec!["mysql", "nginx", "web"]);
    }

    #[test]
    fn test_store_default() {
        let store = ConnectionStore::default();
        assert!(store.connections.is_empty());
    }

    #[test]
    fn test_store_multiple_local_ips() {
        let mut store = ConnectionStore::new();
        store.add(make_conn("a", "192.168.1.100")).unwrap();
        store.add(make_conn("b", "10.0.0.50")).unwrap();
        store.add(make_conn("c", "8.8.8.8")).unwrap();

        let local_ips: Vec<IpAddr> = vec![
            "192.168.1.50".parse().unwrap(),
            "10.0.0.10".parse().unwrap(),
        ];
        let results = store.filter_local(&local_ips);
        assert_eq!(results.len(), 2);
    }
}
