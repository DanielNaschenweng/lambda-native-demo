#!/usr/bin/env bash
#
# Deploy das duas demos (TDC 2026) na AWS via SAM:
#
#   ./deploy.sh          # deploya os dois projetos
#   ./deploy.sh rust     # apenas lambda-rust
#   ./deploy.sh java     # apenas lambda-quarkus-native
#
# Pré-requisitos:
#   - AWS CLI com credenciais configuradas (região sa-east-1 nos samconfig.toml)
#   - SAM CLI
#   - Rust: cargo-lambda (https://www.cargo-lambda.info)
#   - Java: Docker rodando (build nativo usa a builder image do Mandrel)

set -euo pipefail

BASE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUST_DIR="$BASE_DIR/lambda-rust"
JAVA_DIR="$BASE_DIR/lambda-quarkus-native"

TARGET="${1:-all}"

# ─── Helpers ──────────────────────────────────────────────────────────────────

log() {
    echo ""
    echo "═══════════════════════════════════════════════════════════════"
    echo "  $1"
    echo "═══════════════════════════════════════════════════════════════"
}

require() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "ERRO: '$1' não encontrado no PATH. $2" >&2
        exit 1
    }
}

show_outputs() {
    local dir="$1" stack="$2"
    echo ""
    echo "Outputs do stack $stack:"
    (cd "$dir" && sam list stack-outputs --stack-name "$stack" --output json) \
        | jq -r '.[] | "  \(.OutputKey): \(.OutputValue)"' \
        || echo "  (não foi possível listar os outputs)"
}

# ─── Deploys ──────────────────────────────────────────────────────────────────

deploy_rust() {
    log "lambda-rust: build + deploy"
    require cargo-lambda "Instale com: cargo install cargo-lambda"

    cd "$RUST_DIR"
    # --beta-features: o build method rust-cargolambda é beta no SAM CLI
    # e sem a flag o build para num prompt interativo de confirmação.
    sam build --beta-features
    sam deploy --no-confirm-changeset --no-fail-on-empty-changeset

    show_outputs "$RUST_DIR" "lambda-rust-demo"
}

deploy_java() {
    log "lambda-quarkus-native: build nativo + deploy"
    require docker "Necessário para o build nativo via container (Mandrel)"

    cd "$JAVA_DIR"
    make build-native
    sam build
    sam deploy --no-confirm-changeset --no-fail-on-empty-changeset

    show_outputs "$JAVA_DIR" "lambda-quarkus-native-demo"
}

# ─── Main ─────────────────────────────────────────────────────────────────────

require sam "Instale o AWS SAM CLI: https://docs.aws.amazon.com/serverless-application-model/"
require jq "Instale com: sudo apt install jq"

case "$TARGET" in
    rust) deploy_rust ;;
    java) deploy_java ;;
    all)
        deploy_rust
        deploy_java
        ;;
    *)
        echo "Uso: $0 [rust|java|all]" >&2
        exit 1
        ;;
esac

log "Deploy concluído"
