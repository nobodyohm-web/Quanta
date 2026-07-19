// Cross-view intents — one view can ask another view's panel to open
// pre-filled (Contacts → Wallet send). Consumed exactly once by the target
// view's mount effect, so no stale intent survives a second navigation.
let sendTo = $state<string | null>(null);

/** Ask the Wallet to open its Send panel pre-filled with `to` (an @pseudo or address). */
export function requestSend(to: string): void {
  sendTo = to;
}

/** Consume the pending send intent (returns it once, then clears it). */
export function takeSendIntent(): string | null {
  const v = sendTo;
  sendTo = null;
  return v;
}

/** Whether a send intent is currently pending (without consuming it). */
export function hasSendIntent(): boolean {
  return sendTo !== null;
}
