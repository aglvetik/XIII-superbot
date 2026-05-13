#!/usr/bin/env bash
set -euo pipefail

SERVICE_NAME="xiii-superbot.service"
APP_DIR="${APP_DIR:-/opt/XIII/xiii-superbot}"
SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}"

echo "Building XIII Superbot release binary..."
cargo build --release

echo "Installing binary into ${APP_DIR}..."
install -d "${APP_DIR}/data/service-status"
install -m 0755 target/release/xiii-superbot "${APP_DIR}/xiii-superbot"

echo "Installing example systemd unit at ${SERVICE_FILE}..."
install -m 0644 scripts/linux/xiii-superbot.service.example "${SERVICE_FILE}"

echo "Reloading systemd daemon..."
systemctl daemon-reload

echo "Installed ${SERVICE_NAME}."
echo "Before starting it, place .env.production and state files under ${APP_DIR}, refresh the latest legacy DBs into their configured /opt paths, stop old services, and capture service status files."
echo "Recommended read-only check first: ${APP_DIR}/xiii-superbot production-preflight --env-file ${APP_DIR}/.env.production"
echo "Start only after dry-run verification: systemctl enable --now ${SERVICE_NAME}"
