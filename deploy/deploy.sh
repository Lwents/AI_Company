#!/usr/bin/env bash
# Installs a release built by GitHub Actions and restarts the service.
# Runs on the server as user `aico`:  deploy.sh /opt/ai-company/incoming/release.tgz
# Keeps data/ and workspace/ untouched; rolls back if the new version does not come up.
set -euo pipefail

APP=/opt/ai-company
PKG="${1:?usage: deploy.sh <release.tgz>}"
NEW=$(mktemp -d "$APP/incoming/new.XXXXXX")
trap 'rm -rf "$NEW"' EXIT

tar -xzf "$PKG" -C "$NEW"
test -x "$NEW/ai-company" || { echo "release has no binary"; exit 1; }

# Keep the previous version for rollback.
[ -f "$APP/ai-company" ] && cp -a "$APP/ai-company" "$APP/ai-company.prev"
[ -d "$APP/web/dist" ] && rm -rf "$APP/web/dist.prev" && cp -a "$APP/web/dist" "$APP/web/dist.prev"

mkdir -p "$APP/web" "$APP/skills"
install -m 755 "$NEW/ai-company" "$APP/ai-company.new"
mv -f "$APP/ai-company.new" "$APP/ai-company"
rsync -a --delete "$NEW/web/dist/" "$APP/web/dist/"
rsync -a --delete "$NEW/skills/" "$APP/skills/"
install -m 644 "$NEW/mcp.json" "$APP/mcp.json"

# The service saves state and stops on SIGTERM; interrupted projects resume on start (RESUME_ON_START=1).
sudo -n /usr/bin/systemctl restart ai-company

ok=0
for _ in $(seq 1 20); do
    sleep 1
    if curl -fsS -o /dev/null http://127.0.0.1:18787/login; then ok=1; break; fi
done

if [ "$ok" != 1 ]; then
    echo "New version did not start: rolling back"
    if [ -f "$APP/ai-company.prev" ]; then
        cp -a "$APP/ai-company.prev" "$APP/ai-company"
        [ -d "$APP/web/dist.prev" ] && rsync -a --delete "$APP/web/dist.prev/" "$APP/web/dist/"
        sudo -n /usr/bin/systemctl restart ai-company
    fi
    exit 1
fi
rm -f "$PKG"
echo "Deployed $(date -u +%FT%TZ)"
