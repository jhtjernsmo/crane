#!/usr/bin/env bash
# Ships the committed source to the server and (re)starts the demo API.
#   DEPLOY_HOST=user@host [DEPLOY_SSH_KEY=~/.ssh/key] deploy/demo-api/deploy.sh
set -euo pipefail

host="${DEPLOY_HOST:?set DEPLOY_HOST=user@host}"
ssh_args=(-o BatchMode=yes)
[[ -n "${DEPLOY_SSH_KEY:-}" ]] && ssh_args+=(-i "$DEPLOY_SSH_KEY")

cd "$(git rev-parse --show-toplevel)"
git archive --format=tar HEAD Cargo.toml Cargo.lock crates src-tauri deploy/demo-api |
	ssh "${ssh_args[@]}" "$host" '
		set -e
		rm -rf ~/crane-demo.new && mkdir ~/crane-demo.new
		tar -x -C ~/crane-demo.new
		rm -rf ~/crane-demo && mv ~/crane-demo.new ~/crane-demo
		cd ~/crane-demo/deploy/demo-api
		docker compose up -d --build --remove-orphans
		docker image prune -f >/dev/null
		docker compose ps
	'
