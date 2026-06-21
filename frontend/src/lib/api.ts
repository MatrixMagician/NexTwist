// Thin typed wrapper around Tauri `invoke`. This is the ONLY place the UI talks to the
// backend; it holds no business logic and never resolves paths — it just names the
// commands (defined in src-tauri/src/commands/*.rs) and mirrors their report types.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** A Steam game found by auto-detection (mirrors steam::DetectedGame). */
export interface DetectedGame {
  appid: number;
  name: string;
  library_path: string;
}

/** A managed game (mirrors core::Game). */
export interface Game {
  appid: number;
  name: string;
  install_dir: string;
  prefix: string;
  staging_dir: string;
}

/** A validated staged mod (mirrors extract::StagedMod); handed back to deploy(). */
export interface StagedMod {
  staging_root: string;
  files: string[];
}

/** Filesystem-safety warnings surfaced at deploy time (mirrors deploy::FsWarning). */
export type FsWarning = "CrossDevice" | "NotCasefolded";

/** Result of a deploy (mirrors deploy::DeployReport). */
export interface DeployReport {
  deployed: number;
  backed_up: number;
  methods: [string, string][];
  fs_warnings: FsWarning[];
  /** Resolved targets whose source file was missing at deploy time (WR-04): NOT deployed
   *  and surfaced so the UI can warn the deployment is incomplete. */
  skipped: string[];
}

/** Result of a purge (mirrors deploy::PurgeReport). */
export interface PurgeReport {
  removed: number;
  restored: number;
  orphans: string[];
}

/** Result of a verify pass (mirrors deploy::VerifyReport). */
export interface VerifyReport {
  missing: string[];
  changed: string[];
  orphans: string[];
  pristine: boolean;
}

/** A managed mod (mirrors core::ManagedMod). Lower rank = higher priority (1-based). */
export interface ManagedMod {
  id: number;
  name: string;
  staging_root: string;
  enabled: boolean;
  rank: number;
}

/** A file-level conflict (mirrors core::FileConflict). `providers`/`winner` are mod ids. */
export interface FileConflict {
  target_rel: string;
  providers: number[];
  winner: number;
}

/** A plugin's master/light/regular classification (mirrors core::PluginKind). */
export type PluginKind = "esm" | "esl" | "esp";

/** A plugin entry (mirrors core::Plugin). `order` is the zero-based load position. */
export interface PluginInfo {
  name: string;
  kind: PluginKind;
  enabled: boolean;
  order: number;
}

/** A LOOT sort proposal (mirrors loadorder::SortProposal). `proposed` writes nothing. */
export interface SortProposal {
  proposed: string[];
  warnings: string[];
}

/** A profile (mirrors core::Profile). Exactly one profile is active per game. */
export interface Profile {
  id: number;
  appid: number;
  name: string;
  active: boolean;
}

/**
 * Result of a profile switch (mirrors deploy::SwitchReport): the purge of the previous
 * deployment, the deploy of the target profile's winner set, and the written plugins.txt.
 */
export interface SwitchReport {
  purged: PurgeReport;
  deployed: DeployReport;
  plugins_txt: string;
}

/** The authenticated NexusMods user (mirrors nexus::UserInfo). */
export interface UserInfo {
  user_id: number;
  name: string;
  is_premium: boolean;
}

/** Persisted NexusMods provenance for a downloaded mod (mirrors core::NexusSource). */
export interface NexusSource {
  mod_id: number;
  nexus_mod_id: number;
  file_id: number;
  version: string;
  display_name: string;
}

/** The result of a completed download (mirrors commands::downloads::DownloadResult). */
export interface DownloadResult {
  mod_id: number;
  display_name: string;
  staging_root: string;
}

/** One row in the downloads list (UI-SPEC §B). Built/updated from progress events. */
export type DownloadState =
  | "queued"
  | "downloading"
  | "extracting"
  | "done"
  | "failed"
  // WR-01/WR-02: a transient, auto-recoverable rate-limit pause (NEXUS-05). NOT a
  // terminal failure — the row shows a paused state and the list shows the backoff notice.
  | "ratelimited";

/** The source coordinates needed to (re)start a download. */
export interface DownloadSource {
  appid: number;
  gameDomain: string;
  nexusModId: number;
  fileId: number;
  key?: string | null;
  expires?: string | null;
}

export interface DownloadItem {
  /** UI-assigned id; echoed back on every progress event. */
  id: string;
  /** Display name shown in the row (monospace). */
  name: string;
  /** Bytes downloaded so far. */
  downloaded: number;
  /** Total bytes (Content-Length) if known. */
  total: number | null;
  /** Current row state. */
  state: DownloadState;
  /** Verbatim failure reason when `state === "failed"`. */
  reason?: string;
  /** The source coordinates, so a Failed row's Retry can re-start the same download. */
  source: DownloadSource;
}

/** The `download://progress` event payload (mirrors commands::downloads::ProgressEvent). */
export interface DownloadProgress {
  id: string;
  downloaded: number;
  total: number | null;
  /** "downloading" | "extracting" | "done" | "failed" | "expired" | "ratelimited". */
  state: string;
  reason: string | null;
}

/** The `nxm://arrival` event payload (mirrors commands::nexus::NxmArrival). Secret-free:
 *  the UI download id plus the non-secret download coordinates (domain/mod/file) so a
 *  Retry of this row can re-issue a premium download. The key/expires redemption secrets
 *  are NEVER emitted. */
export interface NxmArrival {
  id: string;
  /** snake_case crosses the IPC boundary (matches the Rust Serialize field). */
  game_domain: string;
  mod_id: number;
  file_id: number;
}

/** The `nxm://expired` event payload (mirrors commands::nexus::NxmExpired). Secret-free:
 *  a human-readable reason for the Warning notice (never the link/key/code). */
export interface NxmExpired {
  reason: string;
}

// --- FOMOD guided installer (FOMOD-01/FOMOD-02). 1:1 mirrors of commands/fomod.rs. ---

/** The 5 FOMOD selection-group types (mirrors fomod::GroupType, PascalCase over IPC). */
export type GroupType =
  | "SelectExactlyOne"
  | "SelectAtMostOne"
  | "SelectAtLeastOne"
  | "SelectAll"
  | "SelectAny";

/** The 5-state FOMOD option type (mirrors fomod::PluginType, PascalCase over IPC). */
export type PluginType =
  | "Required"
  | "Optional"
  | "Recommended"
  | "NotUsable"
  | "CouldBeUsable";

/** One selectable option (mirrors commands::fomod::OptionProjection). */
export interface FomodOption {
  name: string;
  description: string;
  /** Archive-relative image path, if the author supplied one (bounded ≤96px in the UI). */
  image: string | null;
  /** The authored default/static type-state; the live type after choices comes from resolve. */
  default_type: PluginType;
  /** `[flag, value]` pairs this option sets when selected; the wizard accumulates these
   *  into the flag set it passes to resolveFomod so the engine re-evaluates conditions live. */
  flags: [string, string][];
}

/** One option group within a step (mirrors commands::fomod::GroupProjection). */
export interface FomodGroup {
  name: string;
  group_type: GroupType;
  options: FomodOption[];
}

/** One wizard install step (mirrors commands::fomod::StepProjection). */
export interface FomodStep {
  name: string;
  /** Whether this step carries a `<visible>` condition (it may be skipped live). */
  conditional: boolean;
  groups: FomodGroup[];
}

/** The parsed FOMOD module projected for the wizard (mirrors commands::fomod::FomodProjection). */
export interface FomodProjection {
  module_name: string;
  steps: FomodStep[];
}

/** The user's wizard choices crossing the IPC boundary (mirrors commands::fomod::SelectionDto).
 *  `chosen` is a list of `[step, group, option]` identities; `flags` a list of `[name, value]`. */
export interface FomodSelection {
  chosen: [string, string, string][];
  flags: [string, string][];
}

/** The dry-run conflict classification (mirrors commands::fomod::ConflictClass). */
export type ConflictClass = "none" | "resolvable" | "blocking";

/** One row of the resolved dry-run plan (mirrors commands::fomod::PlanEntry). */
export interface FomodPlanEntry {
  src: string;
  dest: string;
  priority: number;
}

/** The dry-run preview shown BEFORE any staging write (mirrors commands::fomod::ResolvePreview). */
export interface FomodResolvePreview {
  plan: FomodPlanEntry[];
  classification: ConflictClass;
  /** Destinations contested by equal-priority sources (the blocking set). */
  blocking: string[];
}

/** The result of a confirmed apply (mirrors commands::fomod::ApplyResult). */
export interface FomodApplyResult {
  mod_id: number;
  name: string;
  staging_root: string;
  files: number;
}

/** Parse a mod archive's `fomod/ModuleConfig.xml` into the wizard projection.
 *  Rejects (throws the verbatim reason) for a non-FOMOD / malformed archive. */
export const parseFomod = (appid: number, archive: string): Promise<FomodProjection> =>
  invoke("parse_fomod", { appid, archive });

/** The PURE dry-run resolve: turn a selection into the file-install plan + conflict
 *  classification WITHOUT writing anything (the FOMOD-02 dry-run-before-apply gate). */
export const resolveFomod = (
  appid: number,
  archive: string,
  selection: FomodSelection,
): Promise<FomodResolvePreview> =>
  invoke("resolve_fomod", { appid, archive, selection });

/** Apply a confirmed (non-blocking) FOMOD install: stage the validated archive and record
 *  it as an ordinary ManagedMod. Throws on a blocking selection (server-side gate). */
export const applyFomod = (
  appid: number,
  archive: string,
  name: string,
  selection: FomodSelection,
): Promise<FomodApplyResult> =>
  invoke("apply_fomod", { appid, archive, name, selection });

export const detectGames = (): Promise<DetectedGame[]> => invoke("detect_games");

export const addGame = (appid: number): Promise<Game> => invoke("add_game", { appid });

export const addGameByFolder = (path: string, appid: number): Promise<Game> =>
  invoke("add_game_by_folder", { path, appid });

export const listGames = (): Promise<Game[]> => invoke("list_games");

export const installArchive = (appid: number, archive: string): Promise<StagedMod> =>
  invoke("install_archive", { appid, archive });

export const deploy = (appid: number, staged: StagedMod): Promise<DeployReport> =>
  invoke("deploy", { appid, staged });

export const purge = (appid: number): Promise<PurgeReport> => invoke("purge", { appid });

export const verify = (appid: number): Promise<VerifyReport> => invoke("verify", { appid });

export const listMods = (appid: number): Promise<ManagedMod[]> =>
  invoke("list_mods", { appid });

export const listConflicts = (appid: number): Promise<FileConflict[]> =>
  invoke("list_conflicts", { appid });

export const setModRank = (appid: number, modId: number, rank: number): Promise<boolean> =>
  invoke("set_mod_rank", { appid, modId, rank });

export const deployWinnerSet = (appid: number): Promise<DeployReport> =>
  invoke("deploy_winner_set", { appid });

export const listPlugins = (appid: number): Promise<PluginInfo[]> =>
  invoke("list_plugins", { appid });

export const setPluginEnabled = (
  appid: number,
  name: string,
  enabled: boolean,
): Promise<void> => invoke("set_plugin_enabled", { appid, name, enabled });

export const savePluginOrder = (appid: number, order: PluginInfo[]): Promise<string> =>
  invoke("save_plugin_order", { appid, order });

export const sortWithLoot = (appid: number): Promise<SortProposal> =>
  invoke("sort_with_loot", { appid });

export const listProfiles = (appid: number): Promise<Profile[]> =>
  invoke("list_profiles", { appid });

export const createProfile = (appid: number, name: string): Promise<Profile> =>
  invoke("create_profile", { appid, name });

export const switchProfile = (appid: number, profileId: number): Promise<SwitchReport> =>
  invoke("switch_profile", { appid, profileId });

export const deleteProfile = (appid: number, profileId: number): Promise<boolean> =>
  invoke("delete_profile", { appid, profileId });

// --- NexusMods auth (NEXUS-01/02). Tokens never cross this boundary; only UserInfo. ---

/** Log in with a manual NexusMods personal API key (the works-today fallback). */
export const loginWithApiKey = (key: string): Promise<UserInfo> =>
  invoke("login_with_api_key", { key });

/** Begin the OAuth2+PKCE login; opens the system browser and returns the authorize URL. */
export const loginOAuthStart = (): Promise<string> => invoke("login_oauth_start");

/** Log out: clears the keyring entry + in-memory token. */
export const logout = (): Promise<void> => invoke("logout");

/** The currently logged-in user, or null if logged out. */
export const accountInfo = (): Promise<UserInfo | null> => invoke("account_info");

// --- NexusMods downloads (NEXUS-03/05/06). Streams server-side; the UI drives entirely
//     off async `download://progress` events so it never freezes. ---

/**
 * Start an in-app download of a NexusMods file and stage it as an ordinary ManagedMod.
 * `key`/`expires` are omitted for a Premium direct download and supplied for a free-user
 * `nxm://` redemption. Resolves with the staged mod once extraction + provenance persist.
 */
export const startDownload = (args: {
  id: string;
  appid: number;
  gameDomain: string;
  nexusModId: number;
  fileId: number;
  key?: string | null;
  expires?: string | null;
}): Promise<DownloadResult> =>
  invoke("start_download", {
    id: args.id,
    appid: args.appid,
    gameDomain: args.gameDomain,
    nexusModId: args.nexusModId,
    fileId: args.fileId,
    key: args.key ?? null,
    expires: args.expires ?? null,
  });

/** Cancel an in-flight download by id (idempotent). */
export const cancelDownload = (id: string): Promise<void> =>
  invoke("cancel_download", { id });

/**
 * Subscribe to per-item download progress events. Returns the Tauri unlisten fn. This is
 * the one event-bridge primitive (the rest of api.ts is request/response invoke).
 */
export const onDownloadProgress = (
  handler: (p: DownloadProgress) => void,
): Promise<UnlistenFn> =>
  listen<DownloadProgress>("download://progress", (e) => handler(e.payload));

/**
 * Subscribe to `nxm://` deep-link arrivals (NXM-01). The shell emits this when a website
 * "Mod Manager Download" link is routed to the running app (a new downloads row begins).
 * The UI shows the non-blocking "Download started from NexusMods" toast.
 */
export const onNxmArrival = (
  handler: (a: NxmArrival) => void,
): Promise<UnlistenFn> =>
  listen<NxmArrival>("nxm://arrival", (e) => handler(e.payload));

/**
 * Subscribe to expired/invalid `nxm://` link notices (UI-SPEC §C.3). The shell emits this
 * for a malformed/expired/unredeemable link so the UI shows the Warning notice instead of
 * a stuck Failed download row.
 */
export const onNxmExpired = (
  handler: (x: NxmExpired) => void,
): Promise<UnlistenFn> =>
  listen<NxmExpired>("nxm://expired", (e) => handler(e.payload));
