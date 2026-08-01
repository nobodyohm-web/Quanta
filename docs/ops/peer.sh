#!/usr/bin/env bash
# ==========================================================================
#  peer.sh — join the network from your own machine, find out whether it
#  actually worked, and pull a fix yourself.
#
#  Written for the second person in a two-machine test: someone who did not
#  write this code and should not have to read it to take part.
#
#  Nothing here lets anyone reach into your machine. `update` is a git pull
#  that YOU run, and it prints every incoming commit before applying it — a
#  currency node that could be told from elsewhere what code to run would be
#  a backdoor, not a convenience.
#
#  Usage:
#    ./peer.sh start     build if needed, then run the node in the background
#    ./peer.sh status    the one screen that says whether it is working
#    ./peer.sh logs      follow the node's log
#    ./peer.sh update    show, then apply, the commits published upstream
#    ./peer.sh report    write a shareable diagnostics file (secrets stripped)
#    ./peer.sh stop      stop the node
#
#  Environment overrides:
#    QUANTA_PEER_DIR         data dir        (default: ~/.quanta-peer)
#    QUANTA_RPC_ADDR         RPC bind        (default: 127.0.0.1:8645)
#    QUANTA_WALLET_PASSWORD  vault password  (else prompted, never written down)
# ==========================================================================
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="${QUANTA_REPO:-$(cd "$HERE/../.." && pwd)}"
BIN="$REPO/src-tauri/target/release/quanta-node"
DIR="${QUANTA_PEER_DIR:-$HOME/.quanta-peer}"
RPC="${QUANTA_RPC_ADDR:-127.0.0.1:8645}"
LOG="$DIR/node.log"
PIDFILE="$DIR/node.pid"
STAMP="$DIR/running.commit"

say()  { printf '\n\033[1m%s\033[0m\n' "$*"; }
ok()   { printf '\033[1;32m%s\033[0m\n' "$*"; }
warn() { printf '\033[1;33m%s\033[0m\n' "$*"; }
fail() { printf '\033[1;31m%s\033[0m\n' "$*"; }

# --- RPC ------------------------------------------------------------------
rpc() {
  # `params` is built separately on purpose: inlining a `${2:-\{\}}` default
  # inside the double-quoted payload leaves the backslashes in place, and the
  # node answers -32700 parse error to every argument-less call.
  local method="$1" params="${2-}"
  [ -n "$params" ] || params='{}'
  curl -s --max-time 5 -X POST "http://$RPC" \
    -H 'Content-Type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$method\",\"params\":$params}"
}
# Pull one scalar out of a flat JSON-RPC result without requiring jq.
field() { sed -n "s/.*\"$1\":\"\{0,1\}\([^,\"}]*\).*/\1/p" <<<"$2" | head -1; }

running() { [ -f "$PIDFILE" ] && kill -0 "$(cat "$PIDFILE" 2>/dev/null)" 2>/dev/null; }

# The branch this checkout follows. A clone made with `git clone` has one; a
# detached HEAD does not, so fall back to the published branch rather than
# failing with a message about `@{u}` that means nothing to a newcomer.
upstream_ref() {
  git -C "$REPO" rev-parse --abbrev-ref --symbolic-full-name '@{u}' 2>/dev/null || echo origin/main
}

git_head()  { git -C "$REPO" rev-parse --short HEAD 2>/dev/null || echo unknown; }
git_dirty() { [ -n "$(git -C "$REPO" status --porcelain 2>/dev/null)" ] && printf ' +local-changes'; }
# What is actually running, recorded at start and compared later. Uncommitted
# edits are part of the identity: "same commit" is not "same code".
build_id()  { printf '%s%s' "$(git_head)" "$(git_dirty)"; }
# The protocol version is read from source, not from the binary: after an
# update it tells us whether the other machine MUST update too. Peers whose
# TORUS_PROTOCOL_VERSION differs ignore each other by design (dispatch step
# ⑩), which on screen looks exactly like a discovery failure.
protocol_in_source() {
  grep -oE 'TORUS_PROTOCOL_VERSION: u8 = [0-9]+' \
    "$REPO/src-tauri/src/p2p/gossip.rs" 2>/dev/null | grep -oE '[0-9]+$'
}

# --- prerequisites --------------------------------------------------------
check_prereqs() {
  local missing=0
  command -v cargo >/dev/null || {
    fail "cargo (Rust) not found."
    echo "  Install it once:  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    missing=1
  }
  command -v git >/dev/null  || { fail "git not found."; missing=1; }
  command -v curl >/dev/null || { fail "curl not found."; missing=1; }
  [ "$missing" -eq 0 ] || exit 1
}

# The password unlocks the encrypted vault holding your keys. It is never
# written to disk here — that would make the encryption pointless — so an
# unattended restart needs it exported in your shell instead.
ensure_password() {
  [ -n "${QUANTA_WALLET_PASSWORD:-}" ] && return 0
  if [ ! -t 0 ]; then
    fail "No vault password and no terminal to ask on."
    echo "  export QUANTA_WALLET_PASSWORD='…' and re-run."
    exit 1
  fi
  printf 'Vault password (8+ chars; same one every time, never stored): '
  read -rs QUANTA_WALLET_PASSWORD; echo
  [ ${#QUANTA_WALLET_PASSWORD} -ge 8 ] || { fail "Too short — 8 characters minimum."; exit 1; }
  export QUANTA_WALLET_PASSWORD
}

build() {
  say "Building quanta-node (incremental — a no-op when already current)…"
  # The frontend is not needed: quanta-node is headless, so no npm, no Node.
  cargo build --release --manifest-path "$REPO/src-tauri/Cargo.toml" --bin quanta-node || exit 1
  [ -x "$BIN" ] || { fail "Build produced no binary at $BIN"; exit 1; }
}

# --- start ----------------------------------------------------------------
cmd_start() {
  if running; then
    warn "Already running (pid $(cat "$PIDFILE")). Showing status instead."
    cmd_status
    return 0
  fi
  check_prereqs
  ensure_password
  build
  mkdir -p "$DIR"

  say "Starting the node (data dir $DIR)…"
  # A plain RUST_LOG=info drowns the log: iroh's transport logs one line per UDP
  # packet, so ~97% of the file is `poll_send` and the last 300 lines of a report
  # contain nothing about Quanta at all. Keep our own logs verbose, keep iroh's
  # dial/discovery decisions, drop its per-packet chatter.
  local filter="${RUST_LOG:-quanta_lib=debug,iroh=info,iroh::socket=warn,iroh_gossip=info,tracing::span=error}"
  # The password goes through the environment, never through argv, so it does
  # not show up in `ps`.
  RUST_LOG="$filter" nohup "$BIN" --data-dir "$DIR" --rpc-addr "$RPC" --mine >> "$LOG" 2>&1 &
  echo $! > "$PIDFILE"
  build_id > "$STAMP"

  # Wait on `node_id`, not on `height`: the RPC listener answers before the
  # Iroh endpoint has bound, so an early getinfo returns a valid body with an
  # empty node_id.
  local id=""
  for _ in $(seq 1 90); do
    running || { fail "The node exited during start-up. Last lines of $LOG:"; tail -20 "$LOG"; rm -f "$PIDFILE"; exit 1; }
    id=$(field node_id "$(rpc getinfo)")
    [ -n "$id" ] && break
    sleep 1
  done
  [ -n "$id" ] || { fail "No network identity within 90 s — see $LOG"; exit 1; }

  ok "Running."
  cmd_status
  echo
  echo "  Explorer  : http://$RPC/   (open it in a browser)"
  echo "  Follow it : $0 logs"
}

# --- status ---------------------------------------------------------------
cmd_status() {
  if ! running; then
    fail "Node: STOPPED"
    echo "  Start it with:  $0 start"
    [ -f "$LOG" ] && { echo "  Last lines of $LOG:"; tail -5 "$LOG" | sed 's/^/    /'; }
    return 1
  fi

  local info h peers fin ver proto addr nid tip bal
  info=$(rpc getinfo)
  [ -n "$info" ] || { fail "Node is up but the RPC on $RPC did not answer."; return 1; }

  ver=$(field version "$info");            proto=$(field protocol_version "$info")
  h=$(field height "$info");               fin=$(field finalized_height "$info")
  peers=$(field peers "$info");            addr=$(field address "$info")
  nid=$(field node_id "$info")
  # `head -1`, not `tail -1`: the RPC emits JSON objects with keys in alphabetical
  # order, so the block's own "hash" comes before "transactions" — and every tx
  # carries a "hash" of its own. Taking the last match compares the tip's last
  # transaction instead of the tip, which happens to agree most of the time and
  # is wrong exactly when it matters.
  [ "${h:-0}" -gt 0 ] 2>/dev/null \
    && tip=$(rpc getblock "{\"height\":$((h - 1))}" | grep -o '"hash":"[0-9a-f]*"' | head -1 | cut -d'"' -f4)
  [ -n "$addr" ] && bal=$(field spendable_uqta "$(rpc getbalance "{\"address\":\"$addr\"}")")

  say "Quanta peer — RUNNING (pid $(cat "$PIDFILE"))"
  echo "  build      : $(cat "$STAMP" 2>/dev/null || echo '?')   v${ver:-?}, protocol ${proto:-?}"
  # Two different identities, and confusing them wastes an evening: the wallet
  # address is where money goes, the node id is what a peer dials.
  echo "  wallet     : ${addr:-?}"
  echo "  node id    : ${nid:-?}"
  echo "  height     : ${h:-?}    finalized: ${fin:-?}"
  echo "  balance    : $(fmt_qta "${bal:-0}")"
  if [ "${peers:-0}" -gt 0 ] 2>/dev/null; then
    ok   "  peers      : $peers          <- you are talking to someone"
  else
    warn "  peers      : 0          <- nobody yet; give it a few minutes, then see
               docs/ops/RUN-WITH-A-FRIEND.md (\"quand ça ne marche pas\")"
  fi
  [ -n "${tip:-}" ] && {
    echo
    echo "  >>> SEND THIS LINE to the other machine — identical = converged:"
    echo "      TIP@$h = $tip"
  }

  # Two stale-build traps, both silent if unreported.
  local head; head="$(build_id)"
  [ "$(cat "$STAMP" 2>/dev/null)" = "$head" ] || \
    warn "
  This checkout is at $head but the running node was built from $(cat "$STAMP" 2>/dev/null || echo '?').
  Restart to run the code you have:  $0 stop && $0 start"
  if git -C "$REPO" fetch --quiet 2>/dev/null; then
    local behind
    behind=$(git -C "$REPO" rev-list --count "HEAD..$(upstream_ref)" 2>/dev/null || echo 0)
    [ "${behind:-0}" -gt 0 ] 2>/dev/null && \
      warn "
  $behind new commit(s) published upstream.  Get them:  $0 update"
  fi
  return 0
}

# Integer µQTA → QTA with six decimals. No float anywhere: the whole protocol
# counts in µQTA precisely so amounts never drift (Rust rule #6).
fmt_qta() {
  local u="${1:-0}"
  [ -n "$u" ] || u=0
  printf '%s.%06d QTA\n' "$((u / 1000000))" "$((u % 1000000))"
}

# --- update ---------------------------------------------------------------
cmd_update() {
  git -C "$REPO" rev-parse --git-dir >/dev/null 2>&1 || {
    fail "$REPO is not a git clone — update needs one."; exit 1; }
  if [ -n "$(git -C "$REPO" status --porcelain)" ]; then
    fail "You have local changes; refusing to overwrite them."
    git -C "$REPO" status --short | sed 's/^/    /'
    exit 1
  fi
  say "Fetching…"
  git -C "$REPO" fetch --quiet || { fail "Fetch failed — offline?"; exit 1; }

  local before_proto after_proto behind up was_running=no
  before_proto=$(protocol_in_source)
  up="$(upstream_ref)"
  behind=$(git -C "$REPO" rev-list --count "HEAD..$up" 2>/dev/null || echo 0)
  running && was_running=yes

  if [ "${behind:-0}" -eq 0 ] 2>/dev/null; then
    ok "Already on the latest published commit ($(git_head))."
    # Nothing new, and the running node was built from exactly this — done.
    [ "$was_running" = yes ] && [ "$(cat "$STAMP" 2>/dev/null)" = "$(build_id)" ] && return 0
  else
    say "$behind new commit(s). Read them — they are about to run on your machine:"
    git -C "$REPO" log --oneline --no-decorate "HEAD..$up" | sed 's/^/    /'
    # Fast-forward only: if the histories ever diverge, stop and say so rather
    # than inventing a merge on someone else's machine.
    git -C "$REPO" merge --ff-only --quiet "$up" || { fail "Cannot fast-forward — histories diverged."; exit 1; }
    ok "Now at $(git_head)."
  fi

  after_proto=$(protocol_in_source)
  cmd_stop >/dev/null 2>&1
  build
  if [ "$was_running" = yes ]; then
    cmd_start
  else
    ok "Built. Start it with:  $0 start"
  fi

  if [ -n "$before_proto" ] && [ -n "$after_proto" ] && [ "$before_proto" != "$after_proto" ]; then
    echo
    warn "PROTOCOL CHANGED: $before_proto -> $after_proto."
    warn "Every machine must update, or you will silently stop seeing each other:"
    warn "a peer on another protocol version is ignored on purpose, and that looks"
    warn "exactly like a network failure. Tell the other side to run '$0 update' too."
  fi
}

# --- report ---------------------------------------------------------------
# Everything an outsider needs to diagnose a problem, and nothing that would
# cost you money if you pasted it in a chat.
cmd_report() {
  mkdir -p "$DIR"
  local out cookie=""
  out="$DIR/report-$(date +%Y%m%d-%H%M%S).txt"
  [ -f "$DIR/.cookie" ] && cookie=$(cat "$DIR/.cookie")

  # Literal (not regex) redaction via bash substitution, so a password full of
  # punctuation cannot slip through a sed metacharacter.
  redact() {
    local line
    while IFS= read -r line; do
      [ -n "${QUANTA_WALLET_PASSWORD:-}" ] && line="${line//$QUANTA_WALLET_PASSWORD/<redacted:password>}"
      [ -n "$cookie" ] && line="${line//$cookie/<redacted:rpc-cookie>}"
      printf '%s\n' "$line"
    done
  }

  {
    echo "=== quanta peer report ==="
    echo "date        : $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    echo "machine     : $(uname -srm)"
    echo "commit      : $(git_head)$(git_dirty)"
    echo "built from  : $(cat "$STAMP" 2>/dev/null || echo '?')"
    echo "protocol    : $(protocol_in_source) (source)"
    echo "rustc       : $(rustc --version 2>/dev/null || echo absent)"
    echo "node        : $(running && echo "running (pid $(cat "$PIDFILE"))" || echo stopped)"
    echo
    echo "=== getinfo ==="        ; rpc getinfo
    echo; echo "=== getfinalityinfo ===" ; rpc getfinalityinfo
    echo; echo "=== getvalidators ==="   ; rpc getvalidators
    echo; echo "=== getmempool ==="      ; rpc getmempool
    echo; echo "=== last 300 log lines ==="
    tail -300 "$LOG" 2>/dev/null
  } | redact > "$out"

  ok "Report written: $out"
  echo "  No keys, no password, no RPC cookie — those are stripped. It does"
  echo "  contain IP addresses (yours on the LAN, and your peers'), which is"
  echo "  usually the point: that is where connection problems show up."
}

cmd_logs() { [ -f "$LOG" ] || { fail "No log yet at $LOG"; exit 1; }; tail -f "$LOG"; }

cmd_stop() {
  running || { warn "Not running."; rm -f "$PIDFILE"; return 0; }
  local pid; pid=$(cat "$PIDFILE")
  kill "$pid" 2>/dev/null
  for _ in $(seq 1 20); do kill -0 "$pid" 2>/dev/null || break; sleep 1; done
  kill -0 "$pid" 2>/dev/null && kill -9 "$pid" 2>/dev/null
  rm -f "$PIDFILE"
  ok "Stopped."
}

case "${1:-status}" in
  start)  cmd_start  ;;
  status) cmd_status ;;
  logs)   cmd_logs   ;;
  update) cmd_update ;;
  report) cmd_report ;;
  stop)   cmd_stop   ;;
  *)
    sed -n '2,26p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
    exit 1
    ;;
esac
