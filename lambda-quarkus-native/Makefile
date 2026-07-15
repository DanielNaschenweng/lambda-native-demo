.PHONY: build build-native dev dev-valid dev-invalid dev-unknown \
        localstack-up localstack-down logs \
        invoke-valid invoke-invalid invoke-unknown consume-orders \
        test clean help build-IngressFunction

# ─── Build ────────────────────────────────────────────────────────────────────

build:
	./mvnw clean package -DskipTests

# O template.yaml declara a Lambda como arm64; o build roda num container
# arm64 (via QEMU em hosts x86_64, lento porém funcional) para o binário
# bater com a arquitetura do runtime.
build-native:
	./mvnw clean package -Pnative -DskipTests \
		-Dquarkus.native.container-build=true \
		-Dquarkus.native.builder-image=quay.io/quarkus/ubi-quarkus-mandrel-builder-image:jdk-21 \
		-Dquarkus.native.container-runtime-options=--platform=linux/arm64

# ─── Dev mode (demo rápida, sem build nativo) ─────────────────────────────────
# Sobe o mock event server do Quarkus em localhost:8080 apontando pro LocalStack

dev:
	QUARKUS_SQS_ENDPOINT_OVERRIDE=http://localhost:4566 \
	AWS_REGION=us-east-1 AWS_ACCESS_KEY_ID=test AWS_SECRET_ACCESS_KEY=test \
	./mvnw quarkus:dev

# Com `make dev` rodando em outro terminal:

dev-valid:
	curl -s -X POST http://localhost:8080 -d @events/order-valid.json | jq .

dev-invalid:
	curl -s -X POST http://localhost:8080 -d @events/order-invalid.json | jq .

dev-unknown:
	curl -s -X POST http://localhost:8080 -d @events/route-unknown.json | jq .

# ─── SAM build target (chamado por `sam build` com BuildMethod: makefile) ─────

build-IngressFunction:
	unzip -o target/function.zip -d $(ARTIFACTS_DIR)

# ─── LocalStack ───────────────────────────────────────────────────────────────

localstack-up:
	docker compose -f docker-compose.localstack.yml up -d
	@echo "Aguardando LocalStack ficar pronto..."
	@sleep 5

localstack-down:
	docker compose -f docker-compose.localstack.yml down

logs:
	docker logs localstack-sqs -f

# ─── Invocações locais (sam local invoke + LocalStack) ────────────────────────
# host.docker.internal permite que o container do SAM enxergue o LocalStack

SAM_ENV = --env-vars events/env-local.json

invoke-valid:
	sam build
	sam local invoke IngressFunction --event events/order-valid.json $(SAM_ENV)

invoke-invalid:
	sam build
	sam local invoke IngressFunction --event events/order-invalid.json $(SAM_ENV)

invoke-unknown:
	sam build
	sam local invoke IngressFunction --event events/route-unknown.json $(SAM_ENV)

# Lê as mensagens enfileiradas para provar que chegaram no SQS
consume-orders:
	docker exec localstack-sqs awslocal sqs receive-message \
		--queue-url http://localhost:4566/000000000000/orders-queue \
		--message-attribute-names All --max-number-of-messages 10

# ─── Qualidade ────────────────────────────────────────────────────────────────

test:
	./mvnw test

clean:
	./mvnw clean

# ─── Help ─────────────────────────────────────────────────────────────────────

help:
	@echo "Targets disponíveis:"
	@echo "  build            Build JVM (target/function.zip)"
	@echo "  build-native     Build nativo via container (Mandrel, sem GraalVM local)"
	@echo "  localstack-up    Sobe LocalStack com as filas SQS criadas"
	@echo "  localstack-down  Derruba o LocalStack"
	@echo "  dev              Dev mode (JVM) com mock event server em :8080"
	@echo "  dev-valid        [dev rodando] pedido válido -> 202 + mensagem no SQS"
	@echo "  dev-invalid      [dev rodando] payload viola o JSON Schema -> 400"
	@echo "  dev-unknown      [dev rodando] route-key inexistente -> 404"
	@echo "  invoke-valid     sam local invoke com pedido válido (requer build-native)"
	@echo "  invoke-invalid   sam local invoke com payload inválido (requer build-native)"
	@echo "  invoke-unknown   sam local invoke com rota inexistente (requer build-native)"
	@echo "  consume-orders   Lê as mensagens da fila orders-queue no LocalStack"
	@echo "  test             Roda os testes"
	@echo "  clean            Limpa artefatos de build"
