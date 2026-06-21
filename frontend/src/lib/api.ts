// Thin typed wrapper around Tauri `invoke`. This is the ONLY place the UI talks to the
// backend; it holds no business logic and never resolves paths — it just names the
// commands (defined in src-tauri/src/commands/*.rs) and mirrors their report types.

import { invoke } from "@tauri-apps/api/core";

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
