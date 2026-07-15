use anyhow::Context;
use async_trait::async_trait;
use aws_sdk_sqs::types::MessageAttributeValue;
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::instrument;

// ── Tipos públicos ────────────────────────────────────────────────────────────

pub struct PublishRequest {
    pub queue_name: String,
    pub body: String,
    pub route_key: String,
    pub trace_id: Option<String>,
    pub correlation_id: Option<String>,
}

// ── Helpers puros (testáveis sem AWS) ─────────────────────────────────────────

/// Monta os message attributes da mensagem SQS.
/// O route key sempre vai junto; trace e correlation apenas se presentes.
pub(crate) fn build_message_attributes(
    route_key: &str,
    trace_id: Option<&str>,
    correlation_id: Option<&str>,
) -> HashMap<String, MessageAttributeValue> {
    let attr = |v: &str| {
        MessageAttributeValue::builder()
            .data_type("String")
            .string_value(v)
            .build()
            .expect("String attribute sempre é válido")
    };

    let mut attrs = HashMap::new();
    attrs.insert("x-route-key".to_string(), attr(route_key));
    if let Some(v) = trace_id {
        attrs.insert("x-trace-id".to_string(), attr(v));
    }
    if let Some(v) = correlation_id {
        attrs.insert("x-correlation-id".to_string(), attr(v));
    }
    attrs
}

// ── Trait Publisher (permite mock nos testes) ─────────────────────────────────

#[async_trait]
pub trait Publisher: Send + Sync {
    /// Publica a mensagem e retorna o message_id atribuído pelo SQS.
    async fn publish(&self, req: PublishRequest) -> anyhow::Result<String>;
}

// ── Implementação real ────────────────────────────────────────────────────────

pub struct SqsPublisher {
    client: aws_sdk_sqs::Client,
    /// Cache de nome da fila -> URL, para não chamar GetQueueUrl a cada request.
    queue_url_cache: RwLock<HashMap<String, String>>,
}

impl SqsPublisher {
    pub async fn new() -> Self {
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;

        Self {
            client: aws_sdk_sqs::Client::new(&config),
            queue_url_cache: RwLock::new(HashMap::new()),
        }
    }

    /// Pré-resolve as URLs das filas durante o cold start.
    ///
    /// Sem isso, a primeira invocação de cada rota paga o custo de uma
    /// chamada GetQueueUrl antes do SendMessage. Falha aqui é não-fatal:
    /// a URL será resolvida novamente na primeira request.
    pub async fn pre_warm<'a>(&self, queue_names: impl IntoIterator<Item = &'a str>) {
        for name in queue_names {
            match self.resolve_queue_url(name).await {
                Ok(_) => tracing::info!(queue = name, "queue URL pre-warmed"),
                Err(e) => tracing::warn!(
                    queue = name,
                    error = %e,
                    "queue pre-warm failed (will retry on first request)"
                ),
            }
        }
    }

    async fn resolve_queue_url(&self, queue_name: &str) -> anyhow::Result<String> {
        if let Some(url) = self.queue_url_cache.read().await.get(queue_name) {
            return Ok(url.clone());
        }

        let url = self
            .client
            .get_queue_url()
            .queue_name(queue_name)
            .send()
            .await
            .with_context(|| format!("falha ao resolver URL da fila {queue_name}"))?
            .queue_url()
            .context("GetQueueUrl não retornou URL")?
            .to_string();

        self.queue_url_cache
            .write()
            .await
            .insert(queue_name.to_string(), url.clone());

        Ok(url)
    }
}

#[async_trait]
impl Publisher for SqsPublisher {
    #[instrument(skip(self, req), fields(route_key = req.route_key, queue = req.queue_name))]
    async fn publish(&self, req: PublishRequest) -> anyhow::Result<String> {
        let queue_url = self.resolve_queue_url(&req.queue_name).await?;

        let result = self
            .client
            .send_message()
            .queue_url(&queue_url)
            .message_body(req.body)
            .set_message_attributes(Some(build_message_attributes(
                &req.route_key,
                req.trace_id.as_deref(),
                req.correlation_id.as_deref(),
            )))
            .send()
            .await;

        let output = match result {
            Ok(o) => o,
            Err(e) => {
                // Invalida o cache: a fila pode ter sido recriada com outra URL.
                self.queue_url_cache.write().await.remove(&req.queue_name);
                return Err(e).context("falha ao enviar mensagem ao SQS");
            }
        };

        output
            .message_id()
            .map(str::to_string)
            .context("SQS não retornou message_id")
    }
}

// ── Testes ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attributes_todos_os_campos() {
        let attrs =
            build_message_attributes("order-created", Some("trace-abc"), Some("corr-ghi"));
        assert!(attrs.contains_key("x-route-key"));
        assert!(attrs.contains_key("x-trace-id"));
        assert!(attrs.contains_key("x-correlation-id"));
    }

    #[test]
    fn test_attributes_sem_opcionais() {
        let attrs = build_message_attributes("events", None, None);
        assert_eq!(attrs.len(), 1);
        assert!(attrs.contains_key("x-route-key"));
    }

    #[test]
    fn test_attributes_route_key_preservado() {
        let attrs = build_message_attributes("order-created", None, None);
        let attr = attrs.get("x-route-key").unwrap();
        assert_eq!(attr.string_value(), Some("order-created"));
        assert_eq!(attr.data_type(), "String");
    }
}
