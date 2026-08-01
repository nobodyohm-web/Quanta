#!/usr/bin/env bash
# ==========================================================================
#  two-machines.sh — the test only two physical machines can settle.
#
#  Run this UNCHANGED on each machine, on two DIFFERENT networks (ideally one
#  on home Wi-Fi, the other on a 4G hotspot): two distinct NATs, no local route
#  between them. Two daemons on one host share a public IP and punch through
#  nothing — which is exactly why that cheaper test proves less.
#
#  What it exercises, and nothing else can:
#    (1) RDV-1  — find each other over the public mainline DHT, no server,
#                 no ticket pasted by hand
#    (2) NAT    — establish direct QUIC (hole punching), or fall back to relay
#    (3) v9     — converge on the SAME tip, and share the block reward
#    (4) RDV-0  — find each other again after a restart, same NodeId
#
#  Usage:  ./two-machines.sh A      (on the first machine)
#          ./two-machines.sh B      (on the second)
#
#  Nothing is exposed: the RPC listener stays bound to 127.0.0.1.
#
#  Environment overrides:
#    QUANTA_REPO             repo root            (default: this script's ../..)
#    QUANTA_TEST_DIR         node data dir        (default: ~/.quanta-2machines-<TAG>)
#    QUANTA_WALLET_PASSWORD  vault password       (default: derived from <TAG>)
#    QUANTA_RPC_ADDR         RPC bind address     (default: 127.0.0.1:8650)
# ==========================================================================
set -uo pipefail

TAG="${1:-A}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="${QUANTA_REPO:-$(cd "$HERE/../.." && pwd)}"
BIN="$REPO/src-tauri/target/release/quanta-node"
DIR="${QUANTA_TEST_DIR:-$HOME/.quanta-2machines-$TAG}"
RPC="${QUANTA_RPC_ADDR:-127.0.0.1:8650}"
LOG="$DIR/node.log"

# A DISTINCT password per machine gives two distinct identities, hence two
# payable addresses — which is what makes the reward split observable at all.
export QUANTA_WALLET_PASSWORD="${QUANTA_WALLET_PASSWORD:-quanta-test-$TAG-change-me}"

say()  { printf '\n\033[1m%s\033[0m\n' "$*"; }
fail() { printf '\n\033[1;31m%s\033[0m\n' "$*"; }
ok()   { printf '\n\033[1;32m%s\033[0m\n' "$*"; }

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
# Pull one scalar field out of a flat JSON-RPC result without requiring jq.
field() { sed -n "s/.*\"$1\":\"\{0,1\}\([^,\"}]*\).*/\1/p" <<<"$2" | head -1; }

# Always build: cargo is incremental, so this is a no-op when up to date, and it
# rules out the trap of running a stale binary from an older protocol version —
# TORUS_PROTOCOL_VERSION has broken nine times, and mismatched peers ignore each
# other by design, which looks exactly like a discovery failure.
say "Building quanta-node (no-op if already current)…"
cargo build --release --manifest-path "$REPO/src-tauri/Cargo.toml" --bin quanta-node || exit 1
[ -x "$BIN" ] || { fail "Build produced no binary at $BIN"; exit 1; }

mkdir -p "$DIR"
say "Machine $TAG — starting node (data-dir $DIR)"
RUST_LOG=info "$BIN" --data-dir "$DIR" --rpc-addr "$RPC" --mine > "$LOG" 2>&1 &
NODE_PID=$!
trap 'kill $NODE_PID 2>/dev/null' EXIT

# --- identity -------------------------------------------------------------
# Wait on `node_id`, not on `height`: the RPC listener answers before the Iroh
# endpoint has bound, so an early `getinfo` returns a valid body with an empty
# node_id. Gating on the field we actually need removes the race.
NODEID=""
for _ in $(seq 1 60); do
  NODEID=$(field node_id "$(rpc getinfo)")
  [ -n "$NODEID" ] && break
  sleep 1
done
[ -n "$NODEID" ] || { fail "Node never published a NodeId within 60 s — see $LOG"; exit 1; }
ADDR=$(field address "$(rpc getinfo)")
echo "  NodeId  : ${NODEID:-?}"
echo "  Address : ${ADDR:-?}"

# --- (1) discovery: THE test. No server, no hand-pasted ticket. -----------
say "Waiting for DHT discovery (up to 6 min)…"
FOUND=""
for i in $(seq 1 72); do
  PEERS=$(field peers "$(rpc getinfo)")
  if [ "${PEERS:-0}" -gt 0 ] 2>/dev/null; then FOUND=yes; break; fi
  [ $((i % 6)) -eq 0 ] && echo "  … ${i}0 s, still alone"
  sleep 5
done
if [ -z "$FOUND" ]; then
  fail "(1) FAILED — no peer found in 6 min."
  echo "  Check $LOG for 'RDV-1', 'Harvested', 'pkarr'."
  echo "  Most common cause: outbound UDP blocked (corporate network, VPN)."
  exit 1
fi
ok "(1) DISCOVERY — peer found with no server involved"

# --- (2)+(3) let the chain grow, then read convergence and the split ------
say "Producing blocks (~8 min for four — one block every 2 min)…"
for i in $(seq 1 120); do
  H=$(field height "$(rpc getinfo)")
  [ "${H:-0}" -ge 5 ] 2>/dev/null && break
  [ $((i % 4)) -eq 0 ] && echo "  … height ${H:-0}"
  sleep 5
done

INFO=$(rpc getinfo)
H=$(field height "$INFO")
PEERS=$(field peers "$INFO")
FINAL=$(field finalized_height "$INFO")
# `head -1`, not `tail -1`: the RPC emits JSON objects with keys in alphabetical
# order, so the block's own "hash" comes before "transactions" — and every tx
# carries a "hash" of its own. Taking the last match compares the tip's last
# transaction instead of the tip, which happens to agree most of the time and is
# wrong exactly when it matters (an empty tip block falls back to the real hash,
# so the two machines would then be comparing two different things).
TIP=$(rpc getblock "{\"height\":$((H - 1))}" | grep -o '"hash":"[0-9a-f]*"' | head -1 | cut -d'"' -f4)

# Distinct reward recipients over the last five blocks — REWARD-SHARE-1 live.
FROM=$((H > 5 ? H - 5 : 0))
PAYEES=$(for h in $(seq "$FROM" $((H - 1))); do
           rpc getblock "{\"height\":$h}" | grep -o '"to":"[0-9a-f]*"'
         done | sort -u | wc -l | tr -d ' ')

say "VERDICT — machine $TAG"
echo "  height           : $H"
echo "  finalized floor  : $FINAL"
echo "  live peers       : $PEERS"
echo "  tip hash         : $TIP"
echo "  reward payees    : $PAYEES distinct addresses paid over 5 blocks"
echo
echo "  >>> COMPARE THIS LINE with the other machine:"
echo "      TIP@$H = $TIP"
echo
echo "  Expected: identical height and tip on both sides (convergence),"
echo "            and >= 2 distinct payees (REWARD-SHARE-1 in the wild)."

# --- (4) survive a restart (RDV-0) ---------------------------------------
# Two independent witnesses of the same promise: the 32-byte `node_key` file on
# disk, and the EndpointId the RPC reports. Checking both catches the case where
# the key survives but the node fails to load it — which is precisely how the
# old `getinfo.node_id` (a per-boot random value, fixed in v3.15.1) managed to
# report "identity lost" while `node_key` sat untouched.
say "Restart test — stopping the node, then bringing it back…"
KEYCOPY="$DIR/node_key.before-restart"
cp "$DIR/node_key" "$KEYCOPY" 2>/dev/null || fail "no node_key to compare — RDV-0 never persisted one"

kill $NODE_PID 2>/dev/null; wait $NODE_PID 2>/dev/null
sleep 3
RUST_LOG=info "$BIN" --data-dir "$DIR" --rpc-addr "$RPC" --mine >> "$LOG" 2>&1 &
NODE_PID=$!
T0=$(date +%s)

# The RPC answers before the endpoint binds, so read the identity only once the
# node has one — otherwise an empty string reads as "identity changed".
NODEID2=""
for _ in $(seq 1 60); do
  NODEID2=$(field node_id "$(rpc getinfo)")
  [ -n "$NODEID2" ] && break
  sleep 1
done

BACK=""
for _ in $(seq 1 90); do
  P=$(field peers "$(rpc getinfo)")
  if [ "${P:-0}" -gt 0 ] 2>/dev/null; then BACK=$(( $(date +%s) - T0 )); break; fi
  sleep 2
done
if cmp -s "$KEYCOPY" "$DIR/node_key" && [ "$NODEID2" = "$NODEID" ]; then
  KEYMSG="network identity preserved"
elif ! cmp -s "$KEYCOPY" "$DIR/node_key"; then
  fail "(4) FAILED — node_key changed across restart"
  echo "  RDV-0 is broken: every previously issued ticket is now stale."
  KEYMSG="IDENTITY LOST"
else
  fail "(4) FAILED — node_key survived but the node reports a different EndpointId"
  echo "  $NODEID -> $NODEID2 — the key is being persisted but not loaded."
  KEYMSG="IDENTITY MISMATCH"
fi
rm -f "$KEYCOPY"

if [ -n "$BACK" ]; then
  ok "(4) RESTART — peer refound in ${BACK}s, $KEYMSG"
else
  fail "(4) peer not refound within 3 min ($KEYMSG)"
  echo "  Note: if BOTH machines restart at the same moment, each is waiting on the"
  echo "  other's next DHT republish — stagger the restarts and re-run this step."
fi

say "Done. Full log: $LOG"
