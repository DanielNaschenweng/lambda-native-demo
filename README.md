# Quarkus Native na prática: como eliminamos o cold start e igualamos o Rust em produção

Repositório de código da palestra apresentada no **TDC 2026**.

Aqui você encontra as duas implementações demonstradas no palco, lado a lado:

| Projeto | Linguagem / Runtime | O que faz |
|---|---|---|
| [`lambda-quarkus-native/`](lambda-quarkus-native/) | Java 21 + Quarkus 3 compilado com GraalVM (binário nativo) | Gateway de ingestão HTTP com validação de contrato via JSON Schema antes de publicar no SQS |
| [`lambda-rust/`](lambda-rust/) | Rust + `lambda_http` + AWS SDK | A mesma função de ingestão, usada como baseline de performance |

O objetivo é permitir que você clone, rode localmente e reproduza a comparação de cold start e tempo de resposta apresentada na palestra.

---

## Sobre o Projeto

### O problema: cold start no AWS Lambda

Toda função Lambda que fica um tempo sem receber tráfego é desligada. Na próxima invocação, a AWS precisa provisionar um novo ambiente de execução do zero: baixar o artefato, iniciar o runtime e inicializar o seu código. Esse é o **cold start**, e ele aparece direto na latência do cliente.

Para Java na JVM tradicional, a conta é pesada: subir a JVM, carregar classes, inicializar o framework e fazer JIT warm-up custa **de 3 a 6 segundos** e exige 512 MB+ de memória. Em APIs voltadas ao usuário, isso é inaceitável. A recomendação usual vira "reescreva em Go ou Rust", o que significa abrir mão de todo o ecossistema e da experiência do time em Java.

### A solução demonstrada

Este repositório mostra o caminho do meio: **compilar o Java para um binário nativo com GraalVM via Quarkus**. O resultado:

| | Java na JVM | Quarkus Nativo (GraalVM) | Rust |
|---|---|---|---|
| Cold start | ~3-6 s | ~200-400 ms | ~50-150 ms |
| Memória mínima | 512 MB+ | 128 MB | 128 MB |
| Artefato | jar + runtime da JVM | binário único | binário único |

O binário nativo elimina a JVM da equação: o metadado de classes, a injeção de dependência e boa parte da inicialização são resolvidos **em tempo de build**, não em tempo de execução. Com isso, o Java passa a competir na mesma faixa do Rust, mantendo a stack e o conhecimento do time.

A função Rust está aqui de propósito: ela é o baseline honesto da comparação. Você roda as duas, mede as duas e tira suas conclusões.

### O que as funções fazem

As duas implementam o mesmo gateway de ingestão serverless (versão simplificada de um gateway real que roda em produção):

```
                     +--------------------------------------------+
 POST /              |  AWS Lambda (binário nativo)               |
 X-Route-Key: xxx -->|  1. Resolve a rota pelo header             |--> SQS orders-queue
 X-Trace-Id: yyy     |  2. Valida o payload (JSON Schema)*        |--> SQS webhooks-queue
                     |  3. Publica com atributos de trace         |
                     +--------------------------------------------+
                                                    * na versão Quarkus
```

- O header `X-Route-Key` decide para qual fila SQS a mensagem vai.
- Na versão Quarkus, rotas com `schema` declarado em [`routes.json`](lambda-quarkus-native/src/main/resources/routes.json) validam o payload **antes** de enfileirar: mensagem inválida recebe `400` na hora, sem DLQ para limpar depois.
- Os headers `X-Trace-Id` e `X-Correlation-Id` viram *message attributes* no SQS, preservando o rastreamento ponta a ponta.
- Resposta `202 Accepted`: o processamento é assíncrono a partir daí.

---

## Arquitetura e Stack Tecnológico

**Padrão arquitetural**: gateway de ingestão assíncrono, uma peça comum em arquiteturas de microserviços. A Lambda desacopla a recepção HTTP do processamento: valida, roteia e enfileira. Quem consome as filas escala no seu próprio ritmo.

**Stack:**

- **Java 21** com **Quarkus 3.26** (`quarkus-amazon-lambda`, `quarkus-arc` para CDI)
- **GraalVM / Mandrel** para compilação nativa (via container, sem instalação local)
- **Rust** (edição 2021) com `lambda_http`, `tokio` e `aws-sdk-sqs`
- **AWS Lambda** com runtime customizado `provided.al2023`
- **Amazon SQS** como broker de mensagens
- **Amazon API Gateway (HTTP API)** como front door
- **AWS SAM** para infraestrutura como código e invocação local
- **LocalStack** para emular o SQS na máquina local
- **JSON Schema (Draft-07)** para validação de contrato na borda (`networknt/json-schema-validator`)

Detalhes de cada implementação:

```
lambda-quarkus-native/
├── src/main/java/br/com/oeratech/ingress/
│   ├── IngressLambda.java        # Handler: HTTP -> extrai headers -> delega
│   ├── IngressService.java       # Orquestra: rota -> validação -> publicação
│   ├── RouteConfigService.java   # Carrega routes.json no startup
│   └── SqsPublisher.java         # Publica no SQS (cache de queue URL + pré-aquecimento)
├── src/main/resources/routes.json # Mapa: X-Route-Key -> fila + schema opcional
├── template.yaml                  # SAM: HTTP API + Lambda (128 MB) + 2 filas SQS
└── Makefile                       # Todos os comandos da demo

lambda-rust/
├── src/
│   ├── main.rs                   # Bootstrap: cliente SQS criado 1x no cold start
│   ├── handler.rs                # Valida headers e JSON, publica, responde 202
│   └── sqs.rs                    # Trait Publisher + implementação SQS
└── template.yaml                 # SAM: HTTP API + Lambda (128 MB, arm64) + fila SQS
```

> Nota de transparência: o template do Quarkus usa `x86_64` e o do Rust usa `arm64`. Para uma comparação rigorosa, alinhe a arquitetura nos dois `template.yaml` antes de medir.

---

## Pré-requisitos

| Ferramenta | Versão | Para quê |
|---|---|---|
| [Java (JDK)](https://adoptium.net/) | 21+ | Build e dev mode do projeto Quarkus |
| [Docker](https://docs.docker.com/get-docker/) | 24+ | Build nativo em container (Mandrel) e LocalStack |
| [AWS SAM CLI](https://docs.aws.amazon.com/serverless-application-model/latest/developerguide/install-sam-cli.html) | 1.100+ | Build, invocação local e deploy |
| [AWS CLI](https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html) | 2.x | Credenciais e consultas às filas/logs na AWS |
| [Rust](https://rustup.rs/) | 1.75+ | Build do projeto Rust |
| [cargo-lambda](https://www.cargo-lambda.info/guide/installation.html) | 1.x | Build cruzado do binário Lambda em Rust |
| [k6](https://k6.io/docs/get-started/installation/) | 0.50+ | Teste de carga contra os endpoints publicados |
| `make`, `curl`, `jq` | - | Automação e testes dos endpoints |

> **Você não precisa instalar o GraalVM.** O build nativo do Quarkus roda dentro de um container Mandrel; basta ter Docker.

Verifique o ambiente:

```bash
java -version        # 21+
docker --version
sam --version
aws --version
cargo --version
cargo lambda --version
```

---

## Passo a Passo para Execução

### 1. Clone o repositório

```bash
git clone <URL_DO_REPOSITORIO>
cd Lambda_Quarkus
```

### 2. Quarkus: caminho rápido (dev mode na JVM, sem build nativo)

Ideal para entender o comportamento da aplicação antes de compilar o binário.

```bash
cd lambda-quarkus-native

# Terminal 1: sobe o LocalStack já com as filas SQS criadas
make localstack-up

# Terminal 1: dev mode com mock event server em localhost:8080
make dev
```

Em um segundo terminal, dispare os três cenários da demo:

```bash
make dev-valid      # 202 -> mensagem enfileirada no SQS
make dev-invalid    # 400 -> erros do JSON Schema na resposta
make dev-unknown    # 404 -> rota não registrada

# Prova real: lê as mensagens que chegaram na orders-queue
make consume-orders
```

### 3. Quarkus: caminho completo (binário nativo + SAM)

```bash
cd lambda-quarkus-native

make localstack-up

# Build nativo em container Mandrel (5-10 min na primeira vez)
make build-native

# Invoca o binário nativo localmente via SAM
make invoke-valid    # 202 e mensagem na fila
make invoke-invalid  # 400 com os erros de validação
make invoke-unknown  # 404
make consume-orders
```

O [`template.yaml`](lambda-quarkus-native/template.yaml) usa o runtime `provided.al2023`, que espera o binário nativo (`bootstrap` dentro do `function.zip`). Os targets `invoke-*` só funcionam após `make build-native`.

### 4. Rust: build e testes

```bash
cd lambda-rust

# Testes unitários (handler com publishers mock)
cargo test

# Build do binário Lambda (release, otimizado para tamanho)
cargo lambda build --release
```

---

## Deploy e Métricas

### Deploy das duas funções na AWS

Cada projeto tem seu próprio stack SAM, independente:

```bash
# Função Quarkus Nativo
cd lambda-quarkus-native
make build-native
sam build
sam deploy --guided --stack-name lambda-quarkus-native-demo
```

```bash
# Função Rust
cd lambda-rust
sam build
sam deploy --guided --stack-name lambda-rust-demo
```

No `--guided`, aceite os padrões e confirme a criação de recursos IAM. Ao final de cada deploy, anote o output `ApiUrl`. Os templates criam as filas SQS, a HTTP API e as funções com permissão `sqs:SendMessage` restrita às filas da demo.

### Smoke test dos endpoints

```bash
export QUARKUS_URL="<ApiUrl do stack lambda-quarkus-native-demo>"
export RUST_URL="<ApiUrl do stack lambda-rust-demo>"

curl -X POST "$QUARKUS_URL" \
  -H 'Content-Type: application/json' \
  -H 'X-Route-Key: order-created' \
  -H 'X-Trace-Id: trace-001' \
  -d '{"order_id":"TDC-2026-001","customer_name":"Daniel","total":149.90}'

curl -X POST "$RUST_URL" \
  -H 'Content-Type: application/json' \
  -H 'X-Route-Key: order-created' \
  -H 'X-Trace-Id: trace-001' \
  -d '{"order_id":"TDC-2026-001","customer_name":"Daniel","total":149.90}'
```

Ambos devem responder `202` com o `message_id` da mensagem publicada no SQS.

### Medindo o cold start

Um cold start só acontece em ambiente de execução novo. Para forçar um, altere qualquer configuração da função (isso descarta os ambientes quentes) e invoque em seguida:

```bash
# Força cold start na função Quarkus
aws lambda update-function-configuration \
  --function-name lambda-quarkus-native-demo \
  --environment "Variables={DISABLE_SIGNAL_HANDLERS=true,FORCE_COLD=$(date +%s)}"
aws lambda wait function-updated --function-name lambda-quarkus-native-demo

curl -s -o /dev/null -w "Quarkus total: %{time_total}s\n" -X POST "$QUARKUS_URL" \
  -H 'X-Route-Key: order-created' \
  -d '{"order_id":"TDC-2026-001","customer_name":"Daniel","total":149.90}'
```

```bash
# Força cold start na função Rust
aws lambda update-function-configuration \
  --function-name lambda-rust-ingress \
  --environment "Variables={QUEUE_URL=<QueueUrl do stack>,RUST_LOG=info,FORCE_COLD=$(date +%s)}"
aws lambda wait function-updated --function-name lambda-rust-ingress

curl -s -o /dev/null -w "Rust total: %{time_total}s\n" -X POST "$RUST_URL" \
  -H 'X-Route-Key: order-created' \
  -d '{"order_id":"TDC-2026-001","customer_name":"Daniel","total":149.90}'
```

Repita o ciclo algumas vezes (5 a 10 amostras) para ter números confiáveis. Requisições seguintes, sem alterar a configuração, mostram a latência **warm**.

### Teste de carga (k6)

O script [`test/load-test.js`](test/load-test.js) reproduz a metodologia da palestra: **100 usuários simultâneos por 10 minutos**, sem rampa, contra qualquer uma das duas funções. Ajuste as URLs no objeto `URLS` do script para os `ApiUrl` dos seus stacks e rode:

```bash
k6 run -e TARGET=java test/load-test.js
k6 run -e TARGET=rust test/load-test.js
```

> **Importante:** os números que você vai obter refletem a *sua* conta, região e carga — a metodologia é a mesma da palestra, mas os valores absolutos não são comparáveis com os de produção apresentados no palco.

#### Atenção: limite de concorrência da conta AWS

Contas AWS novas frequentemente vêm com o limite de **apenas 10 execuções concorrentes** de Lambda (o padrão de 1.000 precisa ser solicitado). Com 100 usuários simultâneos, as requisições que excedem o limite são rejeitadas **antes de chegar à função**: o API Gateway (HTTP API) devolve `503 Service Unavailable`, enquanto o painel do Lambda segue mostrando **100% de sucesso** — requisição com throttle não vira invocação e só aparece na métrica `Throttles` do CloudWatch.

Verifique o limite da sua conta:

```bash
aws lambda get-account-settings \
  --query 'AccountLimit.ConcurrentExecutions'
```

Se o valor for baixo, solicite o aumento (pedidos até o padrão de 1.000 costumam ser aprovados automaticamente em minutos ou poucas horas):

```bash
aws service-quotas request-service-quota-increase \
  --service-code lambda --quota-code L-B99A9384 \
  --desired-value 1000
```

Enquanto o aumento não é aprovado, o próprio script já ameniza o problema: a pausa entre iterações tem *jitter* (`sleep(0.5 + Math.random())`) para espalhar as requisições em vez de dispará-las em rajadas sincronizadas de 100. Mesmo assim, espere alguns `503` no primeiro segundo do teste — os 100 usuários virtuais iniciam juntos.

### Comparando no CloudWatch

A medida oficial do cold start é a `Init Duration` na linha `REPORT` dos logs. Consulta pronta para o **CloudWatch Logs Insights** (selecione os log groups `/aws/lambda/lambda-quarkus-native-demo` e `/aws/lambda/lambda-rust-ingress`):

```
filter @type = "REPORT"
| parse @message /Init Duration: (?<init_ms>[\d.]+)/
| stats count(*) as invocacoes,
        avg(init_ms) as init_medio_ms,
        max(init_ms) as init_max_ms,
        avg(@duration) as duracao_media_ms,
        avg(@maxMemoryUsed / 1024 / 1024) as memoria_mb
  by @log
```

O que observar:

- **`Init Duration`**: o cold start propriamente dito. É aqui que o Quarkus Nativo e o Rust jogam no mesmo campeonato, e a JVM tradicional não.
- **`Duration`**: a latência warm de cada invocação.
- **`Max Memory Used`**: as duas funções rodam confortavelmente nos 128 MB configurados.

### Limpeza

Para não deixar custo residual na conta:

```bash
sam delete --stack-name lambda-quarkus-native-demo
sam delete --stack-name lambda-rust-demo
```

---

## Dúvidas e Contato

Se você assistiu à palestra e chegou até aqui: obrigado! Esse repositório existe exatamente para isso, para você quebrar, medir e tirar suas próprias conclusões.

- Encontrou um problema ou tem uma sugestão? Abra uma [issue](../../issues).
- Reproduziu os testes e chegou a números diferentes? Quero saber! Abra uma issue com o seu cenário (região, arquitetura, memória).
- Quer continuar a conversa sobre Java nativo, serverless e arquitetura?

**Vamos nos conectar:**

- LinkedIn: [SEU_LINKEDIN_AQUI]
- GitHub: [SEU_GITHUB_AQUI]
- E-mail: [SEU_EMAIL_AQUI]

Bons builds, e que seus cold starts sejam sempre de milissegundos. 🚀
