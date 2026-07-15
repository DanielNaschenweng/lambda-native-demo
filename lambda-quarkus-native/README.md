# Lambda Quarkus Nativo — Demo TDC

Gateway de ingestão serverless em **Java + Quarkus Nativo (GraalVM)**: recebe requisições HTTP pelo API Gateway, valida o contrato com JSON Schema e enfileira no **Amazon SQS**.

Versão simplificada, para demonstração, de um gateway de ingestão real que roda em produção.

## Arquitetura

```
                    ┌──────────────────────────────────────────┐
                    │  AWS Lambda (binário nativo GraalVM)     │
POST /              │                                          │
X-Route-Key: xxx ──>│  1. Resolve rota (routes.json)           │──> SQS orders-queue
                    │  2. Valida payload (JSON Schema)         │──> SQS webhooks-queue
                    │  3. Publica com atributos de trace       │
                    └──────────────────────────────────────────┘
```

- O header `X-Route-Key` decide para qual fila a mensagem vai.
- Rotas com `schema` no [routes.json](src/main/resources/routes.json) validam o payload **antes** de enfileirar: mensagem inválida nem entra na fila (400 na cara do cliente, sem DLQ pra limpar depois).
- Headers `X-Trace-Id` e `X-Correlation-Id` viram *message attributes* no SQS, preservando o rastreamento ponta a ponta.
- Resposta `202 Accepted`: o processamento é assíncrono a partir daqui.

## Por que nativo?

| | JVM | Nativo (GraalVM) |
|---|---|---|
| Cold start | ~3-6 s | ~200-400 ms |
| Memória mínima | 512 MB+ | 128 MB |
| Tamanho do artefato | ~15 MB + runtime | binário único |

Em Lambda, cold start e memória são a conta. O binário nativo faz o Java competir de igual pra igual com Go e Rust nesse cenário.

## Pré-requisitos

- Java 21
- Docker (para o build nativo em container e o LocalStack)
- [AWS SAM CLI](https://docs.aws.amazon.com/serverless-application-model/latest/developerguide/install-sam-cli.html)

## Rodando a demo local

### Caminho rápido: dev mode (JVM, sem build nativo)

```bash
# Terminal 1: LocalStack já com as filas SQS criadas
make localstack-up

# Terminal 1: dev mode com mock event server em localhost:8080
make dev

# Terminal 2: os três cenários da demo
make dev-valid      # 202 -> mensagem enfileirada
make dev-invalid    # 400 -> erros do JSON Schema na resposta
make dev-unknown    # 404 -> rota não registrada

# Prova real: lê as mensagens que chegaram na orders-queue
make consume-orders
```

### Caminho completo: binário nativo + SAM

```bash
make localstack-up

# Build nativo (usa container Mandrel, não precisa de GraalVM local)
make build-native

make invoke-valid    # 202 e mensagem na fila
make invoke-invalid  # 400 com os erros de validação
make invoke-unknown  # 404
make consume-orders
```

O [template.yaml](template.yaml) usa runtime `provided.al2023`, que espera o binário nativo (`bootstrap` dentro do `function.zip`). Os targets `invoke-*` só funcionam após `make build-native`.

## Deploy na AWS

```bash
make build-native
sam build
sam deploy --guided
```

O template cria as duas filas, a HTTP API e a função com permissão de `sqs:SendMessage` restrita às filas da demo.

```bash
# Teste no endpoint publicado
curl -X POST "$API_URL" \
  -H 'Content-Type: application/json' \
  -H 'X-Route-Key: order-created' \
  -H 'X-Trace-Id: trace-001' \
  -d '{"order_id":"TDC-2026-001","customer_name":"Daniel","total":149.90}'
```

## Estrutura

```
src/main/java/br/com/oeratech/ingress/
├── IngressLambda.java        # Handler: HTTP -> extrai headers -> delega
├── IngressService.java       # Orquestra: rota -> validação -> publicação
├── RouteConfigService.java   # Carrega routes.json no startup
├── SqsPublisher.java         # Publica no SQS (cache de queue URL + pré-aquecimento)
├── dto/                      # RouteDTO (JSON) e RouteTarget (resolvido)
└── exception/                # BadRequest, NotFound, Broker
src/main/resources/
├── application.properties
└── routes.json               # Mapa: X-Route-Key -> fila + schema opcional
```

