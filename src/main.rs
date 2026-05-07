use axum::{routing::get, Json, Router};
use category::handler::CategoryHandler;
use category::manager::biz::CategoryBiz;
use category::manager::client::BudgetClient;
use category::manager::repository::CategoryRepository;
use category::pb::service::category::category_service_server::CategoryServiceServer;
use std::{net::SocketAddr, sync::Arc};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let rust_log = std::env::var("RUST_LOG").ok();
    philand_logging::init("category", rust_log.as_deref().or(Some("category=debug")));

    let app_info = philand_application::from_env_with_prefix("CATEGORY_APP");
    tracing::info!("starting {}", app_info.user_agent());

    let database_url =
        std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL not set"))?;
    let grpc_host = std::env::var("GRPC_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let grpc_port: u16 = std::env::var("GRPC_PORT")
        .unwrap_or_else(|_| "50104".to_string())
        .parse()?;
    let http_host = std::env::var("HTTP_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let http_port: u16 = std::env::var("HTTP_PORT")
        .unwrap_or_else(|_| "9104".to_string())
        .parse()?;
    let budget_url =
        std::env::var("BUDGET_GRPC_URL").unwrap_or_else(|_| "http://127.0.0.1:50103".to_string());

    let repo = CategoryRepository::new(&database_url)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to init repository: {e}"))?;
    tracing::info!("Storage initialized");

    let budget_client = BudgetClient::connect(&budget_url)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to budget gRPC: {e}"))?;
    tracing::info!("Budget gRPC client connected to {}", budget_url);

    let biz = Arc::new(CategoryBiz::new(repo, budget_client));
    let grpc_handler = CategoryHandler::new(biz);

    let grpc_addr: SocketAddr = format!("{grpc_host}:{grpc_port}").parse()?;
    let grpc_server = tonic::transport::Server::builder()
        .add_service(CategoryServiceServer::new(grpc_handler))
        .serve(grpc_addr);
    tracing::info!("gRPC server listening on {}", grpc_addr);

    let http_addr: SocketAddr = format!("{http_host}:{http_port}").parse()?;
    let http_app = Router::new().route("/health", get(health_check));
    let http_listener = tokio::net::TcpListener::bind(http_addr).await?;
    tracing::info!("HTTP server listening on {}", http_addr);

    tokio::select! {
        res = grpc_server => { if let Err(e) = res { tracing::error!("gRPC error: {}", e); } }
        res = axum::serve(http_listener, http_app) => { if let Err(e) = res { tracing::error!("HTTP error: {}", e); } }
    }
    Ok(())
}

async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "service": "category" }))
}
