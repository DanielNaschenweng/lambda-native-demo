mod handler;
mod routes;
mod sqs;

use lambda_http::{run, service_fn};
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // JSON estruturado compatível com CloudWatch Logs Insights
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    fmt()
        .json()
        .with_env_filter(filter)
        .with_current_span(true)
        .init();

    // Rotas carregadas e schemas compilados uma única vez no cold start,
    // como o RouteConfigService faz no @PostConstruct do Quarkus.
    let routes = routes::RouteConfig::load().map_err(|e| {
        tracing::error!(error = %e, "failed to load routes.json on cold start");
        e
    })?;

    // Cliente SQS criado uma única vez no cold start e reutilizado
    // em todas as invocações — a primeira requisição não paga o setup.
    let publisher = sqs::SqsPublisher::new().await;
    publisher.pre_warm(routes.queue_names()).await;

    tracing::info!("lambda-rust started");

    run(service_fn(|req| handler::handle(req, &routes, &publisher))).await?;

    Ok(())
}
