//! Configuration read from environment variables.

use std::net::SocketAddr;

/// Shortest `API_TOKEN` accepted, so a placeholder like `changeme` cannot go live.
pub const MIN_API_TOKEN_LEN: usize = 32;

#[derive(Clone, Debug)]
pub struct Config {
    /// `DATABASE_URL`: PostgreSQL connection string. Required.
    pub database_url: String,
    /// `API_TOKEN`: bearer token clients must send. Required, at least `MIN_API_TOKEN_LEN` bytes.
    pub api_token: String,
    /// `BIND_ADDR`: address to listen on. Defaults to `127.0.0.1:8080`.
    pub bind_addr: SocketAddr,
    /// `DB_MAX_CONNECTIONS`: connection pool size. Defaults to 5.
    pub db_max_connections: u32,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Builds a config from any key lookup, so tests do not touch the process environment.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let database_url = get("DATABASE_URL").ok_or("DATABASE_URL is not set")?;

        let api_token = get("API_TOKEN").ok_or("API_TOKEN is not set")?;
        if api_token.len() < MIN_API_TOKEN_LEN {
            return Err(format!(
                "API_TOKEN must be at least {MIN_API_TOKEN_LEN} characters"
            ));
        }

        let bind_addr = get("BIND_ADDR")
            .unwrap_or_else(|| "127.0.0.1:8080".into())
            .parse()
            .map_err(|e| format!("BIND_ADDR is not a valid socket address: {e}"))?;

        let db_max_connections = match get("DB_MAX_CONNECTIONS") {
            Some(v) => v
                .parse()
                .ok()
                .filter(|n| *n > 0)
                .ok_or("DB_MAX_CONNECTIONS must be a positive integer")?,
            None => 5,
        };

        Ok(Self {
            database_url,
            api_token,
            bind_addr,
            db_max_connections,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn config(vars: &[(&str, &str)]) -> Result<Config, String> {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Config::from_lookup(|k| vars.get(k).cloned())
    }

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn defaults_apply() {
        let c = config(&[("DATABASE_URL", "postgres://x"), ("API_TOKEN", TOKEN)]).unwrap();
        assert_eq!(c.bind_addr, "127.0.0.1:8080".parse().unwrap());
        assert_eq!(c.db_max_connections, 5);
    }

    #[test]
    fn required_and_invalid_values_are_rejected() {
        assert!(config(&[("API_TOKEN", TOKEN)]).is_err());
        assert!(config(&[("DATABASE_URL", "postgres://x")]).is_err());
        assert!(config(&[("DATABASE_URL", "postgres://x"), ("API_TOKEN", "short")]).is_err());
        assert!(config(&[
            ("DATABASE_URL", "postgres://x"),
            ("API_TOKEN", TOKEN),
            ("BIND_ADDR", "nope")
        ])
        .is_err());
        assert!(config(&[
            ("DATABASE_URL", "postgres://x"),
            ("API_TOKEN", TOKEN),
            ("DB_MAX_CONNECTIONS", "0")
        ])
        .is_err());
    }
}
