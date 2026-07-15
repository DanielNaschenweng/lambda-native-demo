#!/usr/bin/env bash
# Uso: ./run.sh [rust|java]  (padrão: rust)
TARGET="${1:-rust}"

if [[ "$TARGET" != "rust" && "$TARGET" != "java" ]]; then
  echo "Uso: $0 [rust|java]" >&2
  exit 1
fi

sudo k6 run load-test.js -e TARGET="$TARGET" --out json="result_${TARGET}.json"
