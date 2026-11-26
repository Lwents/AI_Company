#!/usr/bin/env bash
# One-time server preparation (Ubuntu 24.04), run as root:
#   HOST=1.2.3.4 CERT=1.2.3.4 DEPLOY_PUBKEY="ssh-ed25519 AAAA…" bash setup-server.sh
# With the UI on Cloudflare Pages also set UI_ORIGIN=https://name.pages.dev API_HOST=example.com
# Afterwards: put secrets in /etc/ai-company/env (see env.example) and push to main.
set -euo pipefail

HOST="${HOST:?set HOST (domain or IP)}"
CERT="${CERT:-$HOST}"
APP=/opt/ai-company
HERE="$(cd "$(dirname "$0")" && pwd)"

# 1. A dedicated, unprivileged user: the AI employees' commands run as this user.
id aico >/dev/null 2>&1 || useradd --create-home --shell /bin/bash aico
install -d -o aico -g aico -m 750 "$APP" "$APP/web" "$APP/data" "$APP/workspace" "$APP/skills" "$APP/incoming"

# 2. Deploy key (GitHub Actions logs in as aico with it).
if [ -n "${DEPLOY_PUBKEY:-}" ]; then
    install -d -o aico -g aico -m 700 /home/aico/.ssh
    touch /home/aico/.ssh/authorized_keys
    grep -qxF "$DEPLOY_PUBKEY" /home/aico/.ssh/authorized_keys || echo "$DEPLOY_PUBKEY" >> /home/aico/.ssh/authorized_keys
    chown aico:aico /home/aico/.ssh/authorized_keys
    chmod 600 /home/aico/.ssh/authorized_keys
fi

# 3. aico may restart only this service (the service itself runs with NoNewPrivileges, so agents cannot).
cat > /etc/sudoers.d/ai-company <<'EOF'
aico ALL=(root) NOPASSWD: /usr/bin/systemctl restart ai-company, /usr/bin/systemctl is-active ai-company
EOF
chmod 440 /etc/sudoers.d/ai-company
visudo -cf /etc/sudoers.d/ai-company

# 4. Secrets file, readable by root only.
install -d -m 700 /etc/ai-company
[ -f /etc/ai-company/env ] || install -m 600 "$HERE/env.example" /etc/ai-company/env

# 5. Browser for the designer / tester (Playwright MCP) and its system libraries.
npx -y playwright@latest install-deps chromium >/dev/null
sudo -u aico -H bash -lc 'cd ~ && npx -y -p @playwright/mcp@0.0.84 playwright install chromium' >/dev/null || \
    sudo -u aico -H bash -lc 'cd ~ && npx -y playwright install chromium' >/dev/null

# 6. Service + nginx.
install -m 644 "$HERE/ai-company.service" /etc/systemd/system/ai-company.service
systemctl daemon-reload
systemctl enable ai-company >/dev/null
sed -e "s/__HOST__/$HOST/g" -e "s/__CERT__/$CERT/g" -e "s#__UI_ORIGIN__#${UI_ORIGIN:-}#g" "$HERE/nginx-ai-company.conf" > /etc/nginx/sites-available/ai-company
# Optional: the same UI/API on a host name (API_HOST, its own certificate), for the Cloudflare Pages proxy.
if [ -n "${API_HOST:-}" ]; then
    sed -e "s/__API_HOST__/$API_HOST/g" "$HERE/nginx-ai-company-api.conf" >> /etc/nginx/sites-available/ai-company
fi
ln -sf /etc/nginx/sites-available/ai-company /etc/nginx/sites-enabled/ai-company
nginx -t
systemctl reload nginx
echo "OK. Edit /etc/ai-company/env, then deploy (git push)."
