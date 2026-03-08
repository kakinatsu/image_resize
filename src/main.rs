mod cleanup;
mod config;
mod db;
mod error;
mod fetch;
mod image_processing;
mod r2;
mod ui;
mod upload;

use std::{env, io, net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{
    Json, Router,
    body::Body,
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, get_service, post},
};
use config::Config;
use serde::Serialize;
use tokio::runtime::Builder;
use tower_http::services::ServeDir;
use tracing::{error, info};

#[derive(Clone)]
pub(crate) struct AppState {
    sqlite_path: PathBuf,
    public_base_url: String,
    max_upload_file_bytes: usize,
    index_html: Arc<str>,
    app_asset_route: Arc<str>,
    app_script: Arc<[u8]>,
    r2: r2::R2Client,
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "image_resize=info,tower_http=info".into()),
        )
        .init();

    let command = match AppCommand::from_args() {
        Ok(command) => command,
        Err(err) => {
            error!("{err}");
            std::process::exit(1);
        }
    };

    let runtime = match Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(err) => {
            error!("failed to create tokio runtime: {err}");
            std::process::exit(1);
        }
    };

    let result = runtime.block_on(run(command));
    match result {
        Ok(()) if matches!(command, AppCommand::Cleanup) => std::process::exit(0),
        Ok(()) => {}
        Err(err) => {
            error!("{err}");
            std::process::exit(1);
        }
    }
}

async fn run(command: AppCommand) -> Result<(), AppError> {
    let config = Config::from_env().map_err(AppError::Config)?;
    db::initialize_database(&config.sqlite_path).map_err(AppError::Database)?;

    match command {
        AppCommand::Serve => {
            let r2 = r2::R2Client::new(&config).map_err(AppError::R2Config)?;
            serve(config, r2).await
        }
        AppCommand::Cleanup => cleanup::run_cleanup(&config)
            .await
            .map(|_| ())
            .map_err(AppError::Cleanup),
    }
}

async fn serve(config: Config, r2: r2::R2Client) -> Result<(), AppError> {
    let ui_assets = ui::load(&config.static_dir).map_err(AppError::UiAssets)?;
    let state = AppState {
        sqlite_path: config.sqlite_path.clone(),
        public_base_url: config.public_base_url.clone(),
        max_upload_file_bytes: config.upload_max_file_bytes,
        index_html: Arc::<str>::from(ui_assets.index_html),
        app_asset_route: Arc::<str>::from(ui_assets.app_asset_route),
        app_script: Arc::<[u8]>::from(ui_assets.app_script),
        r2,
    };
    let app_asset_route = state.app_asset_route.to_string();

    let app = build_router(state, config.static_dir.clone());
    let listener = tokio::net::TcpListener::bind(config.app_addr)
        .await
        .map_err(|source| AppError::Bind {
            addr: config.app_addr,
            source,
        })?;

    info!("listening on http://{}", config.app_addr);
    info!("serving static files from {}", config.static_dir.display());
    info!("using sqlite database {}", config.sqlite_path.display());
    info!(
        "configured upload size limit: {} bytes",
        config.upload_max_file_bytes
    );
    info!("fingerprinted app asset path: {}", app_asset_route);

    axum::serve(listener, app).await.map_err(AppError::Serve)?;

    Ok(())
}

fn build_router(state: AppState, static_dir: PathBuf) -> Router {
    let static_service = get_service(ServeDir::new(static_dir));
    let app_asset_route = state.app_asset_route.to_string();

    Router::new()
        .route("/", get(index))
        .route("/index.html", get(index))
        .route("/app.js", get(legacy_app_script))
        .route(&app_asset_route, get(fingerprinted_app_script))
        .route("/healthz", get(healthz))
        .route("/api/settings", get(settings))
        .route(
            "/api/images",
            post(upload::upload_image)
                .layer(upload::upload_body_limit(state.max_upload_file_bytes)),
        )
        .route("/i/:id", get(fetch::get_image))
        .fallback_service(static_service)
        .with_state(state)
}

async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn index(axum::extract::State(state): axum::extract::State<AppState>) -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-store, no-cache, must-revalidate")],
        Html(state.index_html.to_string()),
    )
}

async fn fingerprinted_app_script(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Response {
    javascript_response(&state.app_script, "public, max-age=31536000, immutable")
}

async fn legacy_app_script(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Response {
    javascript_response(&state.app_script, "no-store, no-cache, must-revalidate")
}

fn javascript_response(script: &[u8], cache_control: &'static str) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/javascript; charset=utf-8")
        .header(header::CACHE_CONTROL, cache_control)
        .body(Body::from(script.to_vec()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

async fn settings(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Json<SettingsResponse> {
    Json(SettingsResponse {
        upload: UploadSettingsResponse {
            max_file_bytes: state.max_upload_file_bytes,
        },
    })
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Serialize)]
struct SettingsResponse {
    upload: UploadSettingsResponse,
}

#[derive(Serialize)]
struct UploadSettingsResponse {
    max_file_bytes: usize,
}

#[derive(Clone, Copy, Debug)]
enum AppCommand {
    Serve,
    Cleanup,
}

impl AppCommand {
    fn from_args() -> Result<Self, AppCommandError> {
        let mut args = env::args().skip(1);

        match (args.next(), args.next()) {
            (None, None) => Ok(Self::Serve),
            (Some(command), None) if command == "cleanup" => Ok(Self::Cleanup),
            (Some(command), _) => Err(AppCommandError::Unknown(command)),
            (None, Some(_)) => unreachable!(),
        }
    }
}

#[derive(Debug)]
enum AppError {
    Config(config::ConfigError),
    Database(db::DbInitError),
    UiAssets(ui::UiAssetError),
    R2Config(r2::R2ConfigError),
    Cleanup(cleanup::CleanupError),
    Bind { addr: SocketAddr, source: io::Error },
    Serve(io::Error),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(err) => write!(f, "{err}"),
            Self::Database(err) => write!(f, "{err}"),
            Self::UiAssets(err) => write!(f, "{err}"),
            Self::R2Config(err) => write!(f, "{err}"),
            Self::Cleanup(err) => write!(f, "{err}"),
            Self::Bind { addr, source } => write!(f, "failed to bind {addr}: {source}"),
            Self::Serve(err) => write!(f, "server error: {err}"),
        }
    }
}

impl std::error::Error for AppError {}

#[derive(Debug)]
enum AppCommandError {
    Unknown(String),
}

impl std::fmt::Display for AppCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown(command) => write!(
                f,
                "unknown command `{command}`; supported commands are `cleanup` or no command"
            ),
        }
    }
}

impl std::error::Error for AppCommandError {}
