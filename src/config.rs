use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    pub api_key: Option<String>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([127, 0, 0, 1], 7700)),
            data_dir: PathBuf::from("."),
            api_key: None,
        }
    }
}

impl RuntimeConfig {
    pub fn from_env_and_args() -> Result<Self, String> {
        let mut config = Self::default();

        if let Ok(bind) = env::var("TYPOJET_BIND") {
            config.bind = parse_bind(&bind)?;
        }
        if let Ok(data_dir) = env::var("TYPOJET_DATA_DIR") {
            config.data_dir = PathBuf::from(data_dir);
        }
        if let Ok(api_key) = env::var("TYPOJET_API_KEY") {
            if !api_key.trim().is_empty() {
                config.api_key = Some(api_key);
            }
        }

        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--bind" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "missing value for --bind".to_string())?;
                    config.bind = parse_bind(&value)?;
                }
                "--data-dir" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "missing value for --data-dir".to_string())?;
                    config.data_dir = PathBuf::from(value);
                }
                "--api-key" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "missing value for --api-key".to_string())?;
                    if !value.trim().is_empty() {
                        config.api_key = Some(value);
                    }
                }
                "--help" | "-h" => {
                    return Err(help_text());
                }
                other => {
                    return Err(format!("unknown argument: {other}\n\n{}", help_text()));
                }
            }
        }

        Ok(config)
    }
}

fn parse_bind(value: &str) -> Result<SocketAddr, String> {
    value
        .parse::<SocketAddr>()
        .map_err(|_| format!("invalid bind address `{value}`"))
}

fn help_text() -> String {
    "typojet\n\nUSAGE:\n    typojet [--bind ADDR] [--data-dir PATH] [--api-key TOKEN]\n\nOPTIONS:\n    --bind ADDR      Bind address, e.g. 127.0.0.1:7700\n    --data-dir PATH  Directory that stores data/indexes.json\n    --api-key TOKEN  Optional bearer token for write and management routes\n\nENVIRONMENT:\n    TYPOJET_BIND\n    TYPOJET_DATA_DIR\n    TYPOJET_API_KEY"
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_current_behavior() {
        let config = RuntimeConfig::default();
        assert_eq!(config.bind, SocketAddr::from(([127, 0, 0, 1], 7700)));
        assert_eq!(config.data_dir, PathBuf::from("."));
        assert_eq!(config.api_key, None);
    }
}
