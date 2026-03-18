use std::net::SocketAddr;

pub struct Config {
    pub dev_addr: SocketAddr,
    pub dev_enabled: bool,
}

impl Config {
    pub fn from_env() -> Self {
        let dev_port: u16 = std::env::var("GOLDENTOOTH_MCP_DEV_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080);

        let dev_enabled: bool = std::env::var("GOLDENTOOTH_MCP_DEV")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(true);

        Self {
            dev_addr: SocketAddr::from(([0, 0, 0, 0], dev_port)),
            dev_enabled,
        }
    }
}
