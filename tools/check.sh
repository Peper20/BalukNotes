#!/usr/bin/env bash
# Полная проверка проекта одной командой — перед коммитом и PR.
#
#   tools/check.sh           # всё: Rust, хранилище-фикстура, типы API, клиент, e2e
#   tools/check.sh --fast    # без сборки клиента и e2e (минута-две)
#   tools/check.sh --rust    # только Rust: test, clippy, fmt, копии крейтов, notes check, заготовки заметок, типы API
#   tools/check.sh --app     # только клиент: check, Vitest, сборка, e2e
#
# Шаги идут все, даже если какой-то упал (кроме зависимых: без сборки
# клиента e2e не запускается). В конце — таблица «шаг → ок/упал, время»;
# вывод упавшего шага — в tests/.data/check/<шаг>.log (хвост печатается).
# Код возврата ненулевой, если упал хоть один шаг.
set -uo pipefail
cd "$(dirname "$0")/.."

rust=1 app=1 fast=0
for arg in "$@"; do
  case "$arg" in
    --fast) fast=1 ;;
    --rust) app=0 ;;
    --app) rust=0 ;;
    -h|--help) sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "неизвестный параметр: $arg (есть --fast, --rust, --app)" >&2; exit 2 ;;
  esac
done
if [[ $rust == 0 && $app == 0 ]]; then
  echo "--rust и --app вместе — это просто tools/check.sh" >&2
  exit 2
fi

logs=tests/.data/check
rm -rf "$logs"
mkdir -p "$logs"

names=() results=() times=()
failed=0

# step <имя> <команда…>: выполнить, записать итог и время.
step() {
  local name="$1"; shift
  local log="$logs/$name.log" start=$SECONDS
  echo "▶ $name: $*"
  if "$@" >"$log" 2>&1; then
    results+=("ок")
  else
    results+=("упал")
    failed=1
    echo "  ✗ $name упал — хвост $log:"
    tail -n 25 "$log" | sed 's/^/    /'
  fi
  names+=("$name")
  times+=("$((SECONDS - start))")
}

# skip <имя> <причина>
skip() {
  names+=("$1") results+=("пропущен: $2") times+=("-")
}

# notes check на tests/vault: ровно намеренные проблемы. Ожидаемый итог —
# строка «Итог: `…`» в tests/vault/README.md (её же сверяет тест ядра).
vault_check() {
  local expected actual
  expected=$(sed -n 's/^Итог: `\(.*\)`$/\1/p' tests/vault/README.md)
  if [[ -z $expected ]]; then
    echo "в tests/vault/README.md нет строки «Итог: \`…\`»"
    return 1
  fi
  # Код выхода 1 — ожидаем: в фикстуре есть намеренные ошибки.
  actual=$(cargo run -q -p notes-cli -- --data tests/.data --vault tests/vault check | tee /dev/stderr | tail -n 1)
  echo "ожидается: $expected"
  echo "получено:  $actual"
  [[ $actual == "$expected" ]]
}

# Типы API выгружены из Rust и закоммичены: после выгрузки нет изменений.
api_types() {
  local dir=app/src/lib/api/types
  (cd app && npm run -s types) || return 1
  local changed
  changed=$(git status --porcelain -- "$dir")
  if [[ -n $changed ]]; then
    echo "типы API устарели — выгрузите (cd app && npm run types) и закоммитьте:"
    echo "$changed"
    git --no-pager diff --stat -- "$dir"
    return 1
  fi
}

# Копии чужих крейтов с правками (vendor/README.md) — их собственные тесты.
vendor_tests() {
  CARGO_TARGET_DIR=target/vendor cargo test -q --manifest-path vendor/comemo/Cargo.toml --features testing || return 1
  rm -f vendor/comemo/Cargo.lock
}

# Навык /new-note: заготовки `notes new` (заметка и книга), примеры
# skills/new-note/examples и блок «Частые вызовы» из SKILL.md (дописан в
# заготовку заметки) собираются во временном хранилище без ошибок,
# предупреждений и битых ссылок.
new_note() {
  local dir=tests/.data/new-note notes=(cargo run -q -p notes-cli -- --data tests/.data/new-note)
  rm -rf "$dir"
  "${notes[@]}" new --tag проверка "Математика/Заметка" || return 1
  "${notes[@]}" new --book --lang en --title 'C++ [1] #x $y$ // - z' "Книга" || return 1
  local vault=$dir/vault
  cp -r skills/new-note/examples "$vault/examples"
  awk '/^## Частые вызовы/ {on = 1} on && /^```$/ {exit} on == 2 {print} on && /^```typst/ {on = 2}' \
    skills/new-note/SKILL.md >>"$vault/Математика/Заметка.typ"
  # цель ссылки #see из блока
  printf '#import "/_baluk/lib.typ": *\n#show: note.with(title: [Производная])\n' >"$vault/Математика/Производная.typ"
  local actual
  actual=$("${notes[@]}" check | tee /dev/stderr | tail -n 1)
  [[ $actual == "заметок: 9, ошибок: 0, предупреждений: 0, битых ссылок: 0" ]]
}

in_app() { (cd app && "$@"); }

if [[ $rust == 1 ]]; then
  step cargo-test cargo test --workspace
  step clippy cargo clippy --workspace --all-targets -- -D warnings
  step fmt cargo fmt --check
  step vendor vendor_tests
  step notes-check vault_check
  step new-note new_note
  step api-types api_types
fi

if [[ $app == 1 ]]; then
  if [[ ! -d app/node_modules ]]; then
    step npm-ci in_app npm ci
  fi
  step svelte-check in_app npm run -s check
  step vitest in_app npm test
  if [[ $fast == 1 ]]; then
    skip build "--fast"
    skip e2e "--fast"
  else
    step build in_app npm run -s build
    if [[ ${results[-1]} == "ок" ]]; then
      step e2e in_app npm run -s e2e
    else
      skip e2e "клиент не собрался"
    fi
  fi
fi

# printf выравнивает по байтам, а не по буквам — дополняем сами.
cell() {
  local LC_ALL=C.UTF-8 s="$1" width="$2"
  printf '%s%*s' "$s" $((width - ${#s})) ''
}
echo
cell "шаг" 14; cell "итог" 22; echo "время, с"
for i in "${!names[@]}"; do
  cell "${names[$i]}" 14; cell "${results[$i]}" 22; echo "${times[$i]}"
done
echo
if [[ $failed == 0 ]]; then
  echo "проверка пройдена ($SECONDS с)"
else
  echo "ПРОВЕРКА НЕ ПРОЙДЕНА ($SECONDS с): логи — $logs/"
fi
exit "$failed"
