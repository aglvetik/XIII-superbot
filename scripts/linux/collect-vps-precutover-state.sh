#!/usr/bin/env bash
set -euo pipefail

ROOT="/opt/XIII/xiii-superbot"
DRY_RUN=0

SERVICES=(
  "xiii-clanlist.service"
  "temp-voice-bot.service"
  "xiii-vacation-bot.service"
  "xiii-discipline-bot.service"
  "xiii-recruit-bot.service"
  "xiii-voice-activity-bot.service"
  "xiii-ticketbot.service"
)

usage() {
  cat <<'EOF'
Usage:
  collect-vps-precutover-state.sh [--dry-run] [--root <path>] [--help]

Purpose:
  Collect read-only old-service status snapshots before cutover.

Behavior:
  - Does NOT stop, start, restart, enable, or disable services.
  - Does NOT modify legacy DB files.
  - Does NOT delete anything.
  - Creates <root>/data/service-status if missing unless --dry-run is used.
  - Captures systemctl status / is-active / is-enabled output for the known old services.

Options:
  --dry-run      Print planned actions without writing files.
  --root <path>  Superbot root directory. Default: /opt/XIII/xiii-superbot
  --help         Show this help text.
EOF
}

log() {
  printf '%s\n' "$*"
}

run_capture() {
  local service="$1"
  local output_dir="$2"
  local status_file="${output_dir}/${service}.status.txt"
  local active_file="${output_dir}/${service}.is-active.txt"
  local enabled_file="${output_dir}/${service}.is-enabled.txt"

  if [[ "$DRY_RUN" -eq 1 ]]; then
    log "[DRY-RUN] systemctl status ${service} --no-pager > ${status_file}"
    log "[DRY-RUN] systemctl is-active ${service} > ${active_file}"
    log "[DRY-RUN] systemctl is-enabled ${service} > ${enabled_file}"
    return 0
  fi

  systemctl status "${service}" --no-pager >"${status_file}" 2>&1 || true
  systemctl is-active "${service}" >"${active_file}" 2>&1 || true
  systemctl is-enabled "${service}" >"${enabled_file}" 2>&1 || true
  log "[OK] captured ${service} -> ${output_dir}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --root)
      if [[ $# -lt 2 ]]; then
        log "[FAIL] --root requires a path"
        exit 2
      fi
      ROOT="$2"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      log "[FAIL] unknown argument: $1"
      usage
      exit 2
      ;;
  esac
done

if ! command -v systemctl >/dev/null 2>&1; then
  log "[FAIL] systemctl is required on the VPS for this script"
  exit 2
fi

OUTPUT_DIR="${ROOT}/data/service-status"

log "XIII Superbot VPS Pre-Cutover State Collection"
log "Mode: $( [[ "$DRY_RUN" -eq 1 ]] && printf 'DRY RUN / NO WRITES' || printf 'SAFE WRITE OF STATUS FILES ONLY' )"
log "Root: ${ROOT}"
log "Output dir: ${OUTPUT_DIR}"
log "Service changes: DISABLED"
log "Legacy DB writes: DISABLED"
log

if [[ "$DRY_RUN" -eq 1 ]]; then
  log "[DRY-RUN] mkdir -p ${OUTPUT_DIR}"
else
  mkdir -p "${OUTPUT_DIR}"
  log "[OK] ensured ${OUTPUT_DIR}"
fi

for service in "${SERVICES[@]}"; do
  run_capture "${service}" "${OUTPUT_DIR}"
done

log
log "Next commands:"
log "  ./scripts/linux/verify-production-layout.sh --root ${ROOT} --env-file ${ROOT}/.env.production"
log "  cargo run -- production-preflight --env-file ${ROOT}/.env.production"
log
log "This script did not stop/start/restart any services and did not modify any legacy DB files."
