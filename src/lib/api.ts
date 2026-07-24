// ═══════════════════════════════════════════════════════════════════════════
//  api.ts — la SEULE porte vers le backend Tauri (couche IPC typée).
//
//  Règle projet (.claude/rules/frontend.md §12) : `invoke<ReturnType>` toujours
//  typé. Ici, on centralise ce contrat : un wrapper par commande, une interface
//  par forme de retour réellement consommée par le front. Plus AUCUN composant
//  n'importe `@tauri-apps/api/core` — ils appellent ces fonctions. (Seul
//  `diag.ts`, bas-niveau, garde son accès direct à l'internal Tauri.)
//
//  Les identifiants de commande (`get_node_status`, `torus://…`) restent inchangés
//  — ce sont des noms wire hérités, gelés pour la compatibilité réseau.
// ═══════════════════════════════════════════════════════════════════════════
import { invoke } from "@tauri-apps/api/core";

// ─── Formes de retour (champs = ceux que le front lit réellement) ───────────

/** `get_node_status` — présence + protocole du nœud local. */
export interface NodeStatus {
  peer_count: number;
  peer_id: string;
  is_online: boolean;
  protocol: string;
  /** Présent selon le build ; Dashboard le lit avec repli « Actif ». */
  mode?: string;
}

/** Un bloc résumé renvoyé dans `get_chain_overview`. */
export interface ChainBlock {
  index: number;
  hash: string;
  tx_count: number;
  minted_qta: number;
}

/** `get_chain_overview` — hauteur + blocs récents + offre prouvable. */
export interface ChainOverview {
  height: number;
  pending: number;
  blocks: ChainBlock[];
  max_supply_qta: number;
  total_mined_qta: number;
  total_burned_qta: number;
  total_supply_qta: number;
  pct_to_cap: number;
}

/** `get_finality_status` — gadget Casper-FFG vivant (époque, plancher, set). */
export interface FinalityStatus {
  height: number;
  finalized_floor: number;
  epoch: number;
  epoch_length: number;
  blocks_into_epoch: number;
  validators: number;
  total_staked: number;
  my_stake: number;
  i_am_validator: boolean;
}

/** Une entrée de déverrouillage (unbonding) du portefeuille. */
export interface UnbondingEntry {
  amount: number;
  unlock_height: number;
  blocks_remaining: number;
}

/** `get_wallet_overview` — vérité on-chain du portefeuille. */
export interface WalletOverview {
  address: string; // hex canonique on-chain (clé ledger, from/to)
  address_bech32: string; // forme publique `qta1…` (partage/QR/envoi)
  height: number;
  spendable: number;
  staked: number;
  unbonding: number;
  unbonding_entries: UnbondingEntry[];
  pending_stake: number;
  pending_unstake: number;
  earned: number;
  min_validator_stake: number;
  unbonding_period_blocks: number;
}

/** `get_recent_txs` — mouvements récents (ring buffer backend). */
export interface LedgerTx {
  id: string;
  from: string;
  to: string;
  amount: number;
  tx_type: string;
  timestamp: string;
  hash: string;
}

/** `get_economy_stats` — émission réelle décroissante (même fonction que le minage). */
export interface EconomyStats {
  emission_per_hour: number;
}

/** `get_my_reputation` — contribution + identité (hors chemin de sécurité). */
export interface Reputation {
  atn_earned: number;
  trust_score: number;
  uptime_minutes: number;
  public_key: string;
  joined_at: string;
}

/** `get_node_mode` — mode du nœud (distinct de NodeStatus, cf. Profile). */
export interface NodeMode {
  mode: string;
}

/** `get_peer_metrics` — métriques par pair (NET-9/NET-10). */
export interface PeerMetric {
  public_key: string;
  display_name: string | null;
  country: string;
  last_rtt_ms: number | null;
  smoothed_rtt_ms: number | null;
  bytes_in: number;
  messages_in: number;
  pings_sent: number;
  pongs_received: number;
  loss_ratio: number;
  uptime_secs: number;
  quality_score: number | null;
  last_seen_secs_ago: number;
}

/** Un agrégat (« gros bloc ») de `get_chain_history`. */
export interface HistoryBucket {
  from: number;
  to: number;
  count: number;
  minted_qta: number;
  tx_count: number;
}

/** Un bloc récent individuel de `get_chain_history`. */
export interface HistoryRecent {
  index: number;
  hash: string;
  minted_qta: number;
  tx_count: number;
}

/** `get_chain_history` — histoire complète (agrégats + récents). */
export interface ChainHistory {
  height: number;
  bucket_size: number;
  buckets: HistoryBucket[];
  recent: HistoryRecent[];
}

/** `biometric_status` — disponibilité + activation Touch ID. */
export interface BiometricStatus {
  supported: boolean;
  enabled: boolean;
}

/** Retour d'un déverrouillage / création / restauration d'identité. */
export interface Identity {
  public_key_hex: string;
}

/** `verify_connection` — résolution d'un proche par pseudo + code. */
export interface ConnectionVerification {
  username: string;
  pk: string;
  connection_code: string;
}

/** Une primitive dans `get_security_audit`. */
export interface SecurityAuditPrimitive {
  name: string;
  standard: string;
}

/** `get_security_audit` — inventaire crypto affiché dans l'aide. */
export interface SecurityAudit {
  signing?: SecurityAuditPrimitive;
  symmetric?: SecurityAuditPrimitive;
  kdf?: SecurityAuditPrimitive;
  hashing?: SecurityAuditPrimitive;
  /** Identité de transport du nœud (NodeId Iroh) — le seul primitif classique
   *  restant, affiché tel quel plutôt que dissimulé (M3, audit 2026-07-25). */
  transport_auth?: SecurityAuditPrimitive;
}

// ─── Données vivantes (lues en boucle par les stores / composants) ──────────

export function getNodeStatus(): Promise<NodeStatus> {
  return invoke<NodeStatus>("get_node_status");
}
export function getChainOverview(limit: number): Promise<ChainOverview> {
  return invoke<ChainOverview>("get_chain_overview", { limit });
}
export function getFinalityStatus(): Promise<FinalityStatus> {
  return invoke<FinalityStatus>("get_finality_status");
}
export function getWalletOverview(): Promise<WalletOverview> {
  return invoke<WalletOverview>("get_wallet_overview");
}
export function getRecentTxs(): Promise<LedgerTx[]> {
  return invoke<LedgerTx[]>("get_recent_txs");
}
export function getEconomyStats(): Promise<EconomyStats> {
  return invoke<EconomyStats>("get_economy_stats");
}
export function getMyReputation(): Promise<Reputation> {
  return invoke<Reputation>("get_my_reputation");
}
export function getNodeMode(): Promise<NodeMode> {
  return invoke<NodeMode>("get_node_mode");
}
export function getPeerMetrics(): Promise<PeerMetric[]> {
  return invoke<PeerMetric[]>("get_peer_metrics");
}
export function getChainHistory(): Promise<ChainHistory> {
  return invoke<ChainHistory>("get_chain_history");
}
export function getMyUsername(): Promise<string | null> {
  return invoke<string | null>("get_my_username");
}
export function getMyConnectionCode(): Promise<string> {
  return invoke<string>("get_my_connection_code");
}
export function getReceiveAddress(): Promise<string> {
  return invoke<string>("get_receive_address");
}
export function getDisplayName(): Promise<string | null> {
  return invoke<string | null>("get_display_name");
}

// ─── Actions ponctuelles (envoi, staking, réseau, identité) ─────────────────

export function resolveUsername(username: string): Promise<string | null> {
  return invoke<string | null>("resolve_username", { username });
}
export function validateAddress(address: string): Promise<boolean> {
  return invoke<boolean>("validate_address", { address });
}
export function ledgerTransfer(to: string, amount: number): Promise<void> {
  return invoke<void>("ledger_transfer", { to, amount });
}
export function ledgerStake(amount: number): Promise<void> {
  return invoke<void>("ledger_stake", { amount });
}
export function ledgerUnstake(amount: number): Promise<void> {
  return invoke<void>("ledger_unstake", { amount });
}
export function connectPeer(peerId: string): Promise<void> {
  return invoke<void>("connect_peer", { peerId });
}
export function setDisplayName(name: string | null): Promise<string | null> {
  return invoke<string | null>("set_display_name", { name });
}
export function verifyConnection(username: string, code: string): Promise<ConnectionVerification> {
  return invoke<ConnectionVerification>("verify_connection", { username, code });
}
export function isUsernameAvailable(username: string): Promise<boolean> {
  return invoke<boolean>("is_username_available", { username });
}
export function claimUsername(username: string): Promise<void> {
  return invoke<void>("claim_username", { username });
}

// ─── Identité / sécurité (boot, déverrouillage, récupération, biométrie) ─────

export function checkIdentity(): Promise<boolean> {
  return invoke<boolean>("check_identity");
}
export function wasGuardianReload(): Promise<boolean> {
  return invoke<boolean>("was_guardian_reload");
}
export function getPublicKey(): Promise<string> {
  return invoke<string>("get_public_key");
}
export function createIdentity(displayName: string, password: string): Promise<Identity> {
  return invoke<Identity>("create_identity", { displayName, password });
}
export function unlockIdentity(password: string): Promise<Identity> {
  return invoke<Identity>("unlock_identity", { password });
}
export function restoreFromPhrase(
  mnemonic: string,
  displayName: string,
  password: string,
): Promise<Identity> {
  return invoke<Identity>("restore_from_phrase", { mnemonic, displayName, password });
}
export function getRecoveryPhrase(): Promise<string> {
  return invoke<string>("get_recovery_phrase");
}
export function getRecoveryKey(): Promise<string> {
  return invoke<string>("get_recovery_key");
}
export function biometricStatus(): Promise<BiometricStatus> {
  return invoke<BiometricStatus>("biometric_status");
}
export function enableBiometricUnlock(password: string): Promise<void> {
  return invoke<void>("enable_biometric_unlock", { password });
}
export function disableBiometricUnlock(): Promise<void> {
  return invoke<void>("disable_biometric_unlock");
}
export function unlockBiometric(): Promise<Identity> {
  return invoke<Identity>("unlock_biometric");
}
export function getSecurityAudit(): Promise<SecurityAudit> {
  return invoke<SecurityAudit>("get_security_audit");
}
