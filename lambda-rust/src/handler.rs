use lambda_http::{Body, Error, Request, Response};
use tracing::instrument;

use crate::routes::RouteConfig;
use crate::sqs::{PublishRequest, Publisher};

fn header_str(req: &Request, name: &str) -> Option<String> {
    req.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

fn json_response(status: u16, body: serde_json::Value) -> Result<Response<Body>, Error> {
    Ok(Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::Text(body.to_string()))?)
}

#[instrument(skip(req, routes, publisher), fields(route_key = tracing::field::Empty))]
pub async fn handle(
    req: Request,
    routes: &RouteConfig,
    publisher: &dyn Publisher,
) -> Result<Response<Body>, Error> {
    let route_key = match header_str(&req, "x-route-key") {
        Some(k) => k,
        None => {
            tracing::warn!("request rejected: missing X-Route-Key header");
            return json_response(400, serde_json::json!({"error": "X-Route-Key ausente"}));
        }
    };
    tracing::Span::current().record("route_key", route_key.as_str());

    let Some(target) = routes.find_route(&route_key) else {
        tracing::warn!(route_key, "route not found");
        return json_response(
            404,
            serde_json::json!({"error": format!("rota não encontrada: {route_key}")}),
        );
    };

    let trace_id = header_str(&req, "x-trace-id");
    let correlation_id = header_str(&req, "x-correlation-id");

    let body = match req.into_body() {
        Body::Text(s) => s,
        Body::Binary(b) => match String::from_utf8(b) {
            Ok(s) => s,
            Err(_) => {
                return json_response(
                    400,
                    serde_json::json!({"error": "corpo deve ser texto UTF-8"}),
                )
            }
        },
        Body::Empty => String::new(),
    };

    // SQS exige body não vazio; validamos que é JSON para pegar erro cedo.
    let instance: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!(route_key, "request rejected: invalid JSON body");
            return json_response(
                400,
                serde_json::json!({"error": "corpo da requisição não é JSON válido"}),
            );
        }
    };

    // Contrato da rota: JSON Schema declarado no routes.json.
    if let Err(messages) = target.validate(&instance) {
        tracing::warn!(route_key, errors = %messages, "schema validation failed");
        return json_response(400, serde_json::json!({"error": messages}));
    }

    let message_id = match publisher
        .publish(PublishRequest {
            queue_name: target.queue_name.clone(),
            body,
            route_key,
            trace_id,
            correlation_id,
        })
        .await
    {
        Ok(id) => id,
        Err(e) => {
            tracing::error!(error = %e, "failed to publish to SQS");
            return json_response(500, serde_json::json!({"error": "erro ao publicar no SQS"}));
        }
    };

    tracing::info!(message_id, "message published successfully");

    json_response(202, serde_json::json!({"message_id": message_id}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    // ── Publishers mock ───────────────────────────────────────────────────────

    struct OkPublisher;

    #[async_trait]
    impl Publisher for OkPublisher {
        async fn publish(&self, _req: PublishRequest) -> anyhow::Result<String> {
            Ok("msg-123".into())
        }
    }

    struct ErrPublisher;

    #[async_trait]
    impl Publisher for ErrPublisher {
        async fn publish(&self, _req: PublishRequest) -> anyhow::Result<String> {
            anyhow::bail!("erro simulado do SQS")
        }
    }

    /// Captura o PublishRequest recebido para inspeção nos testes.
    struct SpyPublisher {
        received: Mutex<Option<PublishRequest>>,
    }

    #[async_trait]
    impl Publisher for SpyPublisher {
        async fn publish(&self, req: PublishRequest) -> anyhow::Result<String> {
            *self.received.lock().unwrap() = Some(req);
            Ok("msg-spy".into())
        }
    }

    fn routes() -> RouteConfig {
        RouteConfig::load().expect("routes.json embutido deve ser válido")
    }

    fn valid_body() -> String {
        r#"{"order_id":"ORD-123","customer_name":"Daniel","total":199.9}"#.into()
    }

    fn valid_request() -> Request {
        http::Request::builder()
            .header("x-route-key", "order-created")
            .body(Body::Text(valid_body()))
            .unwrap()
    }

    // ── 400 ──────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_sem_route_key_retorna_400() {
        let req = http::Request::builder().body(Body::Empty).unwrap();
        let resp = handle(req, &routes(), &OkPublisher).await.unwrap();
        assert_eq!(resp.status(), 400);
    }

    #[tokio::test]
    async fn test_body_nao_json_retorna_400() {
        let req = http::Request::builder()
            .header("x-route-key", "order-created")
            .body(Body::Text("isto nao e json".into()))
            .unwrap();
        let resp = handle(req, &routes(), &OkPublisher).await.unwrap();
        assert_eq!(resp.status(), 400);
    }

    #[tokio::test]
    async fn test_body_vazio_retorna_400() {
        let req = http::Request::builder()
            .header("x-route-key", "order-created")
            .body(Body::Empty)
            .unwrap();
        let resp = handle(req, &routes(), &OkPublisher).await.unwrap();
        assert_eq!(resp.status(), 400);
    }

    #[tokio::test]
    async fn test_body_fora_do_schema_retorna_400() {
        let req = http::Request::builder()
            .header("x-route-key", "order-created")
            .body(Body::Text(r#"{"order_id":"ORD-123"}"#.into()))
            .unwrap();
        let resp = handle(req, &routes(), &OkPublisher).await.unwrap();
        assert_eq!(resp.status(), 400);

        if let Body::Text(body) = resp.into_body() {
            assert!(body.contains("customer_name"));
        } else {
            panic!("esperava Body::Text");
        }
    }

    // ── 404 ──────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_rota_desconhecida_retorna_404() {
        let req = http::Request::builder()
            .header("x-route-key", "rota-que-nao-existe")
            .body(Body::Text("{}".into()))
            .unwrap();
        let resp = handle(req, &routes(), &OkPublisher).await.unwrap();
        assert_eq!(resp.status(), 404);
    }

    // ── 202 ──────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_body_valido_retorna_202() {
        let resp = handle(valid_request(), &routes(), &OkPublisher).await.unwrap();
        assert_eq!(resp.status(), 202);
    }

    #[tokio::test]
    async fn test_202_contem_message_id() {
        let resp = handle(valid_request(), &routes(), &OkPublisher).await.unwrap();
        if let Body::Text(body) = resp.into_body() {
            assert!(body.contains("msg-123"));
        } else {
            panic!("esperava Body::Text");
        }
    }

    // ── Propagação para o publisher ──────────────────────────────────────────

    #[tokio::test]
    async fn test_headers_e_fila_propagados_ao_publisher() {
        let spy = SpyPublisher {
            received: Mutex::new(None),
        };
        let req = http::Request::builder()
            .header("x-route-key", "order-created")
            .header("x-trace-id", "trace-123")
            .header("x-correlation-id", "corr-789")
            .body(Body::Text(valid_body()))
            .unwrap();

        handle(req, &routes(), &spy).await.unwrap();

        let received = spy.received.lock().unwrap().take().unwrap();
        assert_eq!(received.route_key, "order-created");
        assert_eq!(received.queue_name, "orders-queue-rust-sample");
        assert_eq!(received.trace_id.as_deref(), Some("trace-123"));
        assert_eq!(received.correlation_id.as_deref(), Some("corr-789"));
    }

    // ── Erro do publisher ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_erro_do_publisher_retorna_500() {
        let resp = handle(valid_request(), &routes(), &ErrPublisher).await.unwrap();
        assert_eq!(resp.status(), 500);

        if let Body::Text(body) = resp.into_body() {
            assert!(body.contains("erro ao publicar no SQS"));
        } else {
            panic!("esperava Body::Text");
        }
    }
}
