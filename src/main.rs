mod config;

use config::Config;
use goldentooth_mcp::tools::version::GoldentoothMcp;

use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder,
    service::TowerToHyperService,
};
use rmcp::transport::streamable_http_server::{
    StreamableHttpService, session::local::LocalSessionManager,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env();

    if config.dev_enabled {
        tracing::info!("Starting dev server on {}", config.dev_addr);
        run_dev_server(config.dev_addr).await?;
    } else {
        tracing::info!("Dev server disabled");
        tokio::signal::ctrl_c().await?;
    }

    Ok(())
}

async fn run_dev_server(addr: std::net::SocketAddr) -> anyhow::Result<()> {
    let service = TowerToHyperService::new(StreamableHttpService::new(
        || Ok(GoldentoothMcp::new()),
        LocalSessionManager::default().into(),
        Default::default(),
    ));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Dev MCP server listening on {}", addr);

    loop {
        let io = tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Shutting down dev server");
                break;
            }
            accept = listener.accept() => {
                TokioIo::new(accept?.0)
            }
        };
        let service = service.clone();
        tokio::spawn(async move {
            if let Err(e) = Builder::new(TokioExecutor::default())
                .serve_connection(io, service)
                .await
            {
                tracing::error!("Connection error: {}", e);
            }
        });
    }

    Ok(())
}
