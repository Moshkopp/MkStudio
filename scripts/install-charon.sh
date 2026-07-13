#!/usr/bin/env bash
set -euo pipefail

SERVICE_NAME="charon"
SERVICE_USER="charon"
SERVICE_GROUP="charon"
INSTALL_PATH="/usr/local/bin/charon"
ENV_PATH="/etc/charon/charon.env"
UNIT_PATH="/etc/systemd/system/charon.service"
DATA_DIR="/var/lib/charon"
BIND="0.0.0.0:3737"
START_SERVICE=1

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
BINARY="${REPO_ROOT}/target/release/charon"

usage() {
    cat <<'EOF'
Installiert Charon als systemd-Dienst (Debian/Proxmox).

Aufruf:
  sudo ./scripts/install-charon.sh [Optionen]

Optionen:
  --binary PATH    Charon-Binary (Standard: target/release/charon)
  --bind ADDR      Bind-Adresse (Standard: 0.0.0.0:3737)
  --data-dir PATH  Persistente Daten (Standard: /var/lib/charon)
  --no-start       Dienst installieren, aber noch nicht starten
  -h, --help       Diese Hilfe anzeigen

Das Script verändert keine Firewallregeln.
EOF
}

die() {
    echo "Fehler: $*" >&2
    exit 1
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --binary)
            [[ $# -ge 2 ]] || die "--binary benötigt einen Pfad"
            BINARY="$2"
            shift 2
            ;;
        --bind)
            [[ $# -ge 2 ]] || die "--bind benötigt eine Adresse"
            BIND="$2"
            shift 2
            ;;
        --data-dir)
            [[ $# -ge 2 ]] || die "--data-dir benötigt einen Pfad"
            DATA_DIR="$2"
            shift 2
            ;;
        --no-start)
            START_SERVICE=0
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            die "unbekannte Option '$1'"
            ;;
    esac
done

[[ ${EUID} -eq 0 ]] || die "bitte als root oder mit sudo ausführen"
command -v systemctl >/dev/null || die "systemd/systemctl wurde nicht gefunden"
command -v install >/dev/null || die "das Programm 'install' wurde nicht gefunden"
command -v getent >/dev/null || die "das Programm 'getent' wurde nicht gefunden"
command -v groupadd >/dev/null || die "das Programm 'groupadd' wurde nicht gefunden"
command -v useradd >/dev/null || die "das Programm 'useradd' wurde nicht gefunden"
[[ -f "${BINARY}" ]] || die "Binary nicht gefunden: ${BINARY}"
[[ -x "${BINARY}" ]] || die "Binary ist nicht ausführbar: ${BINARY}"
[[ "${DATA_DIR}" = /* ]] || die "--data-dir muss ein absoluter Pfad sein"
[[ "${DATA_DIR}" != *[[:space:]]* ]] || die "--data-dir darf keine Leerzeichen enthalten"
[[ "${BIND}" != *[[:space:]]* && "${BIND}" == *:* ]] \
    || die "ungültige Bind-Adresse: ${BIND}"

ALLOW_NETWORK=1
case "${BIND}" in
    127.*|'[::1]:'*) ALLOW_NETWORK=0 ;;
esac

if ! getent group "${SERVICE_GROUP}" >/dev/null; then
    groupadd --system "${SERVICE_GROUP}"
fi
if ! id --user "${SERVICE_USER}" >/dev/null 2>&1; then
    useradd \
        --system \
        --gid "${SERVICE_GROUP}" \
        --home-dir "${DATA_DIR}" \
        --no-create-home \
        --shell /usr/sbin/nologin \
        "${SERVICE_USER}"
fi

install -d -m 0750 -o "${SERVICE_USER}" -g "${SERVICE_GROUP}" "${DATA_DIR}"
install -d -m 0750 -o root -g "${SERVICE_GROUP}" "$(dirname -- "${ENV_PATH}")"
install -m 0755 -o root -g root "${BINARY}" "${INSTALL_PATH}"

ENV_TMP="$(mktemp)"
UNIT_TMP="$(mktemp)"
trap 'rm -f "${ENV_TMP}" "${UNIT_TMP}"' EXIT

cat >"${ENV_TMP}" <<EOF
CHARON_BIND=${BIND}
CHARON_ALLOW_NETWORK=${ALLOW_NETWORK}
CHARON_DATA_DIR=${DATA_DIR}
EOF

cat >"${UNIT_TMP}" <<EOF
[Unit]
Description=LuxiFer Charon coordination service
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=${SERVICE_USER}
Group=${SERVICE_GROUP}
EnvironmentFile=${ENV_PATH}
ExecStart=${INSTALL_PATH}
Restart=on-failure
RestartSec=2
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=${DATA_DIR}

[Install]
WantedBy=multi-user.target
EOF

install -m 0640 -o root -g "${SERVICE_GROUP}" "${ENV_TMP}" "${ENV_PATH}"
install -m 0644 -o root -g root "${UNIT_TMP}" "${UNIT_PATH}"

systemctl daemon-reload
systemctl enable "${SERVICE_NAME}.service"
if [[ ${START_SERVICE} -eq 1 ]]; then
    systemctl restart "${SERVICE_NAME}.service"
fi

echo "Charon wurde installiert."
echo "  Dienst:  ${SERVICE_NAME}.service"
echo "  Adresse:  ${BIND}"
echo "  Daten:    ${DATA_DIR}"
if [[ ${START_SERVICE} -eq 1 ]]; then
    systemctl --no-pager --full status "${SERVICE_NAME}.service" || true
else
    echo "  Start:    systemctl start ${SERVICE_NAME}.service"
fi
echo "Hinweis: Port ${BIND##*:}/tcp muss separat auf das interne Netz begrenzt werden."
