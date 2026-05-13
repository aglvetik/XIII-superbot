#!/usr/bin/env bash
set -euo pipefail

ROOT="/opt/XIII/xiii-superbot"
ENV_FILE=""
FAILURES=0
WARNS=0

usage() {
  cat <<'EOF'
Usage:
  verify-production-layout.sh [--root <path>] [--env-file <path>] [--help]

Purpose:
  Read-only VPS layout validation before cutover.

Behavior:
  - Does NOT stop, start, restart, enable, or disable services.
  - Does NOT modify legacy DB files.
  - Does NOT delete anything.
  - Reads only the path-related variables needed from the env file.
  - Does NOT print secrets or full env contents.

Options:
  --root <path>      Superbot root directory. Default: /opt/XIII/xiii-superbot
  --env-file <path>  Production env file. Default: <root>/.env.production
  --help             Show this help text.
EOF
}

ok() {
  printf '[OK] %s\n' "$*"
}

warn() {
  printf '[WARN] %s\n' "$*"
  WARNS=$((WARNS + 1))
}

fail() {
  printf '[FAIL] %s\n' "$*"
  FAILURES=$((FAILURES + 1))
}

check_file() {
  local label="$1"
  local path="$2"
  if [[ -f "$path" ]]; then
    ok "${label} exists: ${path}"
  else
    fail "${label} missing: ${path}"
  fi
}

check_dir() {
  local label="$1"
  local path="$2"
  if [[ -d "$path" ]]; then
    ok "${label} exists: ${path}"
  else
    fail "${label} missing: ${path}"
  fi
}

read_env_path() {
  local key="$1"
  node - "$ENV_FILE" "$key" <<'EOF'
const fs = require('fs');
const envFile = process.argv[2];
const key = process.argv[3];
const text = fs.readFileSync(envFile, 'utf8');
for (const line of text.split(/\r?\n/)) {
  const trimmed = line.trim();
  if (!trimmed || trimmed.startsWith('#')) continue;
  const eq = trimmed.indexOf('=');
  if (eq < 0) continue;
  const name = trimmed.slice(0, eq).trim();
  if (name !== key) continue;
  const value = trimmed.slice(eq + 1).trim();
  process.stdout.write(value);
  process.exit(0);
}
process.exit(1);
EOF
}

validate_production_path_shape() {
  local label="$1"
  local path="$2"
  if [[ -z "$path" ]]; then
    fail "${label} is empty"
    return
  fi
  if [[ "$path" == *"XIII_BOTS_FULL_COPY"* ]]; then
    fail "${label} points at XIII_BOTS_FULL_COPY; production env must use real VPS paths"
    return
  fi
  if [[ "$path" =~ ^[A-Za-z]:[\\/].* ]]; then
    fail "${label} looks like a Windows path: ${path}"
    return
  fi
  if [[ "$path" != /* ]]; then
    fail "${label} must be an absolute Linux path: ${path}"
    return
  fi
  ok "${label} looks production-shaped: ${path}"
}

check_legacy_path() {
  local key="$1"
  local kind="$2"
  local value=""
  if ! value="$(read_env_path "$key")"; then
    fail "${key} is missing from ${ENV_FILE}"
    return
  fi
  validate_production_path_shape "$key" "$value"
  if [[ "$kind" == "dir" ]]; then
    check_dir "$key" "$value"
  else
    check_file "$key" "$value"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --root)
      if [[ $# -lt 2 ]]; then
        fail "--root requires a path"
        exit 2
      fi
      ROOT="$2"
      shift 2
      ;;
    --env-file)
      if [[ $# -lt 2 ]]; then
        fail "--env-file requires a path"
        exit 2
      fi
      ENV_FILE="$2"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument: $1"
      usage
      exit 2
      ;;
  esac
done

if [[ -z "$ENV_FILE" ]]; then
  ENV_FILE="${ROOT}/.env.production"
fi

printf 'XIII Superbot Production Layout Check\n'
printf 'Mode: READ ONLY / NO WRITES\n'
printf 'Root: %s\n' "$ROOT"
printf 'Env file: %s\n' "$ENV_FILE"
printf 'Service changes: DISABLED\n'
printf 'Legacy DB writes: DISABLED\n'
printf 'Google calls: DISABLED\n'
printf '\n'

check_dir "root directory" "$ROOT"
check_file "env file" "$ENV_FILE"
check_dir "data directory" "${ROOT}/data"
check_dir "service-status directory" "${ROOT}/data/service-status"

if [[ -e "${ROOT}/.env.local" ]]; then
  warn ".env.local exists under ${ROOT}; it is not required on the VPS and should stay private/offline"
else
  ok ".env.local is not required on the VPS"
fi

check_legacy_path "LEGACY_TICKET_DB_PATH" "file"
check_legacy_path "LEGACY_VOICE_DB_PATH" "file"
check_legacy_path "LEGACY_RECRUIT_DB_PATH" "file"
check_legacy_path "LEGACY_VACATION_DB_PATH" "file"
check_legacy_path "LEGACY_DISCIPLINE_DB_PATH" "file"
check_legacy_path "LEGACY_TEMP_VOICE_DB_PATH" "file"
check_legacy_path "LEGACY_CLANLIST_DATA_DIR" "dir"

printf '\nSummary: WARN=%s FAIL=%s\n' "$WARNS" "$FAILURES"
if [[ "$FAILURES" -gt 0 ]]; then
  exit 2
fi
