#!/usr/bin/env bash
# Two voice peers on this machine in one tmux window: listener on the left, dialer on the
# right. Extra args (e.g. --clean none) go to both. Use headphones or the peers echo each other.
# Re-running replaces the session; ctrl-b d detaches, `tmux kill-session -t voice` ends it.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
bin="$root/target/release/voice"
log=$(mktemp -t voice-listen.XXXXXX)
extra="$*"

cargo build -q --release -p voice --manifest-path "$root/Cargo.toml"
tmux kill-session -t voice 2>/dev/null || true

tmux new-session -d -s voice -x 200 -y 50 "$bin listen $extra 2>&1 | tee -i $log"
# Keep a pane readable after its peer hangs up (ctrl-c would otherwise close it).
tmux set-option -t voice remain-on-exit on
for _ in $(seq 1 50); do grep -q "voice dial" "$log" && break; sleep 0.2; done
dial=$(grep -o "voice dial .*" "$log") || { echo "listener never printed a dial line:"; cat "$log"; exit 1; }

tmux split-window -h -t voice "$bin ${dial#voice } $extra"
tmux select-layout -t voice even-horizontal

if [[ -t 0 ]]; then
    if [[ -n "${TMUX:-}" ]]; then tmux switch-client -t voice; else tmux attach -t voice; fi
fi
