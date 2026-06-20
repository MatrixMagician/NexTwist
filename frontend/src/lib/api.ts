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
