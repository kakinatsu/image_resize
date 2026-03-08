use std::{env, net::SocketAddr, path::PathBuf};

pub struct Config {
    pub app_addr: SocketAddr,
    pub public_base_url: String,
    pub sqlite_path: PathBuf,
    pub static_dir: PathBuf,
    pub r2_endpoint: String,
    pub r2_bucket: String,
    pub r2_access_key_id: String,
    pub r2_secret_access_key: String,
    pub r2_region: String,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let app_addr = env::var("APP_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:3000".to_owned())
            .parse()
            .map_err(ConfigError::InvalidSocketAddr)?;

        let public_base_url =
            env::var("PUBLIC_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".to_owned());

        let sqlite_path = env::var("SQLITE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("data/image_upload.db"));

        let static_dir = env::var("STATIC_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("static"));

        let r2_endpoint = required_var("R2_ENDPOINT")?;
        let r2_bucket = required_var("R2_BUCKET")?;
        let r2_access_key_id = required_var("R2_ACCESS_KEY_ID")?;
        let r2_secret_access_key = required_var("R2_SECRET_ACCESS_KEY")?;
        let r2_region = env::var("R2_REGION").unwrap_or_else(|_| "auto".to_owned());

        Ok(Self {
            app_addr,
            public_base_url,
            sqlite_path,
            static_dir,
            r2_endpoint,
            r2_bucket,
            r2_access_key_id,
            r2_secret_access_key,
            r2_region,
        })
    }
}

fn required_var(key: &'static str) -> Result<String, ConfigError> {
    match env::var(key) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(ConfigError::MissingEnv(key)),
    }
}

#[derive(Debug)]
pub enum ConfigError {
    InvalidSocketAddr(std::net::AddrParseError),
    MissingEnv(&'static str),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSocketAddr(err) => write!(f, "invalid APP_ADDR: {err}"),
            Self::MissingEnv(key) => write!(f, "missing required environment variable {key}"),
        }
    }
}

impl std::error::Error for ConfigError {}
