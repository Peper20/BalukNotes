#!/usr/bin/env bash
# Тестовое окружение: сервер на хранилище-фикстуре tests/vault со своими
# данными (настройки, кэш) в tests/.data — отдельно от data/ пользователя.
#
#   tools/test-env.sh               # http://127.0.0.1:8432, отладочная сборка
#   tools/test-env.sh --fresh       # сначала очистить tests/.data (настройки по умолчанию, пустой кэш)
#   tools/test-env.sh --release     # релизная сборка (встроенные клиент и библиотека)
#   PORT=8500 tools/test-env.sh
#
# Остановить — Ctrl+C или `kill $(pgrep -x notes)`.
set -euo pipefail
cd "$(dirname "$0")/.."

port="${PORT:-8432}"
profile=()
for arg in "$@"; do
  case "$arg" in
    --fresh) rm -rf tests/.data ;;   # кэш и настройки тестов — восстанавливать нечего
    --release) profile=(--release) ;;
    *) echo "неизвестный параметр: $arg" >&2; exit 2 ;;
  esac
done
mkdir -p tests/.data

echo "тестовое окружение: http://127.0.0.1:$port  (хранилище tests/vault, данные tests/.data)"
exec cargo run -q "${profile[@]}" -p notes-cli -- \
  --data tests/.data --vault tests/vault serve --addr "127.0.0.1:$port"
