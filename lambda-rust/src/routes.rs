use anyhow::Context;
use jsonschema::Validator;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

/// routes.json embutido no binário em tempo de compilação — equivalente ao
/// resource no classpath do Quarkus. Parseado uma única vez no cold start.
const ROUTES_JSON: &str = include_str!("../routes.json");

#[derive(Deserialize)]
struct RouteDto {
    header_value: String,
    queue_name: String,
    #[serde(default)]
    schema: Option<serde_json::Value>,
}

/// Destino de uma rota: fila SQS + JSON Schema compilado (opcional).
pub struct RouteTarget {
    pub queue_name: String,
    schema: Option<Validator>,
}

impl RouteTarget {
    /// Valida o body contra o JSON Schema da rota, se ela tiver um.
    /// Retorna todas as violações encontradas, separadas por vírgula.
    pub fn validate(&self, instance: &serde_json::Value) -> Result<(), String> {
        let Some(schema) = &self.schema else {
            return Ok(());
        };

        let errors: Vec<String> = schema
            .iter_errors(instance)
            .map(|e| {
                let path = e.instance_path.to_string();
                if path.is_empty() {
                    e.to_string()
                } else {
                    format!("{path}: {e}")
                }
            })
            .collect();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join(", "))
        }
    }
}

/// Mapa de rotas carregado do routes.json, indexado pelo valor do
/// header X-Route-Key.
pub struct RouteConfig {
    routes: HashMap<String, RouteTarget>,
}

impl RouteConfig {
    pub fn load() -> anyhow::Result<Self> {
        Self::from_json(ROUTES_JSON)
    }

    fn from_json(json: &str) -> anyhow::Result<Self> {
        let dtos: Vec<RouteDto> =
            serde_json::from_str(json).context("routes.json inválido")?;

        let mut routes = HashMap::new();
        for dto in dtos {
            let schema = dto
                .schema
                .as_ref()
                .map(|s| {
                    jsonschema::validator_for(s).map_err(|e| {
                        anyhow::anyhow!(
                            "schema inválido para rota {}: {e}",
                            dto.header_value
                        )
                    })
                })
                .transpose()?;

            tracing::info!(
                route = dto.header_value,
                queue = dto.queue_name,
                has_schema = schema.is_some(),
                "route registered"
            );

            routes.insert(
                dto.header_value,
                RouteTarget {
                    queue_name: dto.queue_name,
                    schema,
                },
            );
        }

        tracing::info!(total = routes.len(), "routes loaded");
        Ok(Self { routes })
    }

    pub fn find_route(&self, header_value: &str) -> Option<&RouteTarget> {
        self.routes.get(header_value)
    }

    /// Nomes únicos de fila, para o pre-warm das URLs no cold start.
    pub fn queue_names(&self) -> HashSet<&str> {
        self.routes
            .values()
            .map(|t| t.queue_name.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Usa o routes.json real do projeto: os testes também servem de
    // verificação do contrato embarcado no binário.
    fn config() -> RouteConfig {
        RouteConfig::load().expect("routes.json embutido deve ser válido")
    }

    #[test]
    fn test_carrega_rotas_do_arquivo() {
        let config = config();
        assert!(config.find_route("order-created").is_some());
        assert!(config.find_route("nao-existe").is_none());
    }

    #[test]
    fn test_queue_names() {
        let config = config();
        assert!(config.queue_names().contains("orders-queue-rust-sample"));
    }

    #[test]
    fn test_rota_sem_schema_aceita_qualquer_json() {
        let json = r#"[{"header_value": "generic-webhook", "queue_name": "webhooks-queue"}]"#;
        let config = RouteConfig::from_json(json).unwrap();
        let route = config.find_route("generic-webhook").unwrap();
        let body = serde_json::json!({"qualquer": "coisa"});
        assert!(route.validate(&body).is_ok());
    }

    #[test]
    fn test_schema_aceita_body_valido() {
        let config = config();
        let route = config.find_route("order-created").unwrap();
        let body = serde_json::json!({
            "order_id": "ORD-123",
            "customer_name": "Daniel",
            "total": 199.9
        });
        assert!(route.validate(&body).is_ok());
    }

    #[test]
    fn test_schema_rejeita_campo_obrigatorio_ausente() {
        let config = config();
        let route = config.find_route("order-created").unwrap();
        let body = serde_json::json!({"order_id": "ORD-123"});
        let err = route.validate(&body).unwrap_err();
        assert!(err.contains("customer_name"));
        assert!(err.contains("total"));
    }

    #[test]
    fn test_schema_rejeita_total_negativo() {
        let config = config();
        let route = config.find_route("order-created").unwrap();
        let body = serde_json::json!({
            "order_id": "ORD-123",
            "customer_name": "Daniel",
            "total": -1
        });
        assert!(route.validate(&body).is_err());
    }

    #[test]
    fn test_schema_rejeita_propriedade_extra() {
        let config = config();
        let route = config.find_route("order-created").unwrap();
        let body = serde_json::json!({
            "order_id": "ORD-123",
            "customer_name": "Daniel",
            "total": 10,
            "extra": true
        });
        assert!(route.validate(&body).is_err());
    }

    #[test]
    fn test_json_invalido_falha_no_load() {
        assert!(RouteConfig::from_json("not json").is_err());
    }

    #[test]
    fn test_schema_invalido_falha_no_load() {
        let json = r#"[{
            "header_value": "x",
            "queue_name": "q",
            "schema": {"type": "tipo-que-nao-existe"}
        }]"#;
        assert!(RouteConfig::from_json(json).is_err());
    }
}
