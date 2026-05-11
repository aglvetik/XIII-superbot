#!/usr/bin/env bash
set -euo pipefail

SERVICE_NAME="xiii-superbot-clanlist.service"
APP_DIR="${APP_DIR:-/opt/XIII/xiii-superbot}"
SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}"

echo "Building XIII Superbot release binary..."
cargo build --release

echo "Installing binary into ${APP_DIR}..."
install -d "${APP_DIR}/data"
install -m 0755 target/release/xiii-superbot "${APP_DIR}/xiii-superbot"

echo "Installing example systemd unit at ${SERVICE_FILE}..."
install -m 0644 scripts/linux/xiii-superbot-clanlist.service.example "${SERVICE_FILE}"

echo "Reloading systemd daemon..."
systemctl daemon-reload

echo "Installed ${SERVICE_NAME}."
echo "Before starting it, ensure:"
echo "  1. ${APP_DIR}/.env exists and contains the production unified env."
echo "  2. ${APP_DIR}/data/clanlist_panel_state.json exists."
echo "  3. The old xiii-clanlist service is stopped."
echo "Then run: systemctl enable --now ${SERVICE_NAME}"
