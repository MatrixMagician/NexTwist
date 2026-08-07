<script lang="ts">
  // Functional-minimal Svelte 5 UI driving the full detect -> install -> deploy ->
  // purge round-trip. Visual polish is deferred; every action goes
  // through lib/api.ts and the UI holds NO business logic / path resolution.
  import * as api from "$lib/api";
  import { fmtBytes, pct } from "$lib/format";
  import * as fomodLogic from "$lib/fomod";
  import * as order from "$lib/plugins";
  import type {
    DetectedGame,
    Game,
    StagedMod,
    DeployReport,
    PurgeReport,
    VerifyReport,
    ManagedMod,
    FileConflict,
    PluginInfo,
    SortProposal,
    ReconcileState,
    Profile,
    SwitchReport,
    UserInfo,
    DownloadItem,
    DownloadProgress,
    FomodProjection,
    FomodResolvePreview,
    ResolveReport,
    DownloadCollectionReport,
    StarfieldStatus,
    IniActivationPreview,
  } from "$lib/api";

  // Starfield's Steam AppID — the one game with a CE2 first-launch gate + drift notice.
  const STARFIELD_APPID = 1716740;

  // Supported Bethesda AppIDs (display only; the backend enforces the allow-list).
  const SUPPORTED = [
    { appid: 489830, name: "Skyrim Special Edition" },
    { appid: 377160, name: "Fallout 4" },
    { appid: STARFIELD_APPID, name: "Starfield" },
  ];

  let detected = $state<DetectedGame[]>([]);
  let managed = $state<Game[]>([]);
  let selectedAppid = $state<number | null>(null);

  let folderPath = $state("");
  let folderAppid = $state<number>(SUPPORTED[0].appid);
  let archivePath = $state("");

  let staged = $state<StagedMod | null>(null);
  let deployReport = $state<DeployReport | null>(null);
  let purgeReport = $state<PurgeReport | null>(null);
  let verifyReport = $state<VerifyReport | null>(null);

  // Conflict view state. `mods` is the priority list (rank-ascending);
  // `conflicts` is the file-level conflict table. `deployedSig` captures the winner/
  // priority signature that was last deployed, so we can show the pending-vs-deployed
  // banner when the current set differs.
  let mods = $state<ManagedMod[]>([]);
  let conflicts = $state<FileConflict[]>([]);
  let deployedSig = $state<string | null>(null);

  // Plugin manager state. `plugins` is the editable, ordered list (the
  // backend returns it merged scan + per-profile state). `sortProposal` holds a LOOT
  // proposal awaiting review; it is applied into `plugins` only on explicit confirm.
  let plugins = $state<PluginInfo[]>([]);
  let sortProposal = $state<SortProposal | null>(null);
  let mastersFirstError = $state<string | null>(null);
  // Engine-classified on-launch reconciliation verdict (Starfield only). The UI
  // renders this verdict; it never derives InSync/Drift from a raw plugins.txt diff.
  let reconcileState = $state<ReconcileState | null>(null);

  // Profile state. `profiles` is the per-game selector source; the active
  // profile is marked with the Accent indicator. Switching and deleting are disk-mutating
  // and confirmation-gated: selecting a target opens a modal, and ONLY on confirm
  // does the safe engine run (purge old → deploy new → write plugins.txt → mark active).
  let profiles = $state<Profile[]>([]);
  let newProfileName = $state("");
  let switchTarget = $state<Profile | null>(null); // pending confirm-to-switch
  let deleteTarget = $state<Profile | null>(null); // pending confirm-to-delete
  let switchReport = $state<SwitchReport | null>(null);

  // Account panel state. `userInfo` drives logged-in vs logged-out; the
  // refresh token / API key NEVER reaches the UI (only UserInfo does). `noKeyring` is
  // set when a login attempt hits the no-Secret-Service hard-fail — it blocks
  // login behind the destructive banner. `apiKeyReveal` toggles the key-paste fallback.
  let userInfo = $state<UserInfo | null>(null);
  let noKeyring = $state(false);
  let apiKeyReveal = $state(false);
  let apiKeyInput = $state("");
  let confirmLogout = $state(false);
  const loggedIn = $derived(userInfo !== null);

  // Downloads list state. `downloads` is the per-item list, driven entirely
  // by async `download://progress` events so the UI never freezes (criterion #4).
  // `rateLimited` toggles the Warning notice above the list when the client backs off.
  // `expiredLink` carries the §C.3 "link expired" Warning (not a Failed download row).
  let downloads = $state<DownloadItem[]>([]);
  let rateLimited = $state(false);
  let expiredLink = $state<string | null>(null);
  // `nxmToast` carries the §C.1 "Download started from NexusMods" Success toast, shown
  // (non-blocking, auto-dismissing) when an nxm:// deep-link arrival fires.
  let nxmToast = $state<string | null>(null);
  let nxmToastTimer: ReturnType<typeof setTimeout> | null = null;

  // --- FOMOD guided installer state. The wizard opens in the existing
  //     .overlay/.modal pattern when an archive contains fomod/ModuleConfig.xml. ALL FOMOD
  //     logic lives in crates/fomod (via api.ts); the UI only renders the projection,
  //     accumulates the user's choices/flags, and drives the dry-run gate before apply. ---
  let fomodArchive = $state<string | null>(null); // the archive being guided-installed
  let fomodProj = $state<FomodProjection | null>(null); // the parsed wizard AST
  let fomodStepIdx = $state(0); // index into the VISIBLE steps
  // Chosen option identities, keyed by a JSON-encoded [step, group, option] triple. A Set drives reactive
  // membership checks; the Svelte 5 rune re-renders on reassignment.
  let fomodChosen = $state<Set<string>>(new Set());
  let fomodPreview = $state<FomodResolvePreview | null>(null); // dry-run gate result
  let fomodApplying = $state(false);
  // The malformed-FOMOD fallback offer (§A.8): the verbatim reason + plain-mod fallback.
  let fomodFallback = $state<string | null>(null);

  // --- Collections state. The Premium gate, the
  //     resolve-before-download HARD GATE (`collResolve`), the bulk-download outcome
  //     (`collDownload`, including manual steps + per-mod failures), and the installed
  //     Collection's deploy/uninstall reports. ALL logic lives in the backend adapters;
  //     the UI renders the reports + drives the two non-skippable consequence gates. ---
  let collSlug = $state("");
  let collRevision = $state<number>(1);
  let collManifest = $state(""); // the fetched collection.json (paste/live-fetch seam)
  let collResolve = $state<ResolveReport | null>(null); // the HARD GATE report
  let collDownload = $state<DownloadCollectionReport | null>(null); // bulk-download outcome
  let collDeployReport = $state<SwitchReport | null>(null);
  let collPurgeReport = $state<PurgeReport | null>(null);
  let collUninstallTarget = $state<{ id: number; name: string } | null>(null); // confirm
  let manualStepsDismissed = $state(false);

  // The per-mod Collection download rows are the existing `downloads` entries whose id is
  // prefixed `collection-` (they flow through the SAME `download://progress` stream).
  const collDownloadRows = $derived(downloads.filter((d) => d.id.startsWith("collection-")));
  const collResolveCounts = $derived.by(() => {
    const m = collResolve?.mods ?? [];
    return {
      available: m.filter((x) => x.status === "Available").length,
      archived: m.filter((x) => x.status === "Archived").length,
      unavailable: m.filter((x) => x.status === "Unavailable").length,
      manual: m.filter((x) => x.status === "Manual").length,
    };
  });
  const isPremium = $derived(userInfo?.is_premium === true);

  let busy = $state(false);
  let error = $state<string | null>(null);
  let status = $state<string | null>(null);

  const selectedGame = $derived(managed.find((g) => g.appid === selectedAppid) ?? null);

  // Starfield CE2 detection status. Fetched only when the selected game is
  // Starfield; the backend forwards the headless aggregate verbatim. `null` = not loaded /
  // not Starfield. All logic is in the engine — the UI only presents the typed state.
  let starfield = $state<StarfieldStatus | null>(null);
  const isStarfield = $derived(selectedGame?.appid === STARFIELD_APPID);
  // FirstLaunchPending gates load-order/INI management. Ready enables it.
  const starfieldPending = $derived(
    starfield !== null && "FirstLaunchPending" in starfield.ce2_state,
  );
  // The resolved (Ready) or expected (pending) CE2 config path, for the guidance line.
  const ce2Path = $derived(
    starfield === null
      ? null
      : "FirstLaunchPending" in starfield.ce2_state
        ? starfield.ce2_state.FirstLaunchPending
        : starfield.ce2_state.Ready,
  );

  // StarfieldCustom.ini loose-file activation. `iniPreview` = the last
  // read-only preview (null = not loaded). Within this surface's two-command engine API
  // (preview + apply), activation state is read off the preview: will_create => the INI is
  // absent (not active); a non-null conflict => a pre-existing user value blocks
  // auto-activation (surface 3); will_edit with no conflict => the INI is active. All the
  // real reversibility/format logic lives in the engine — the UI only presents the preview.
  let iniPreview = $state<IniActivationPreview | null>(null);
  let iniModalOpen = $state(false);
  const iniActive = $derived(
    iniPreview !== null && iniPreview.will_edit && iniPreview.conflict === null,
  );
  const iniConflict = $derived(iniPreview?.conflict ?? null);

  // Enabled mods in priority order — the input the resolver folds over.
  const enabledMods = $derived(mods.filter((m) => m.enabled));

  // The current winner/priority signature: enabled mod ids in rank order. When this
  // differs from what was last deployed, priority changes are PENDING on disk.
  const currentSig = $derived(enabledMods.map((m) => m.id).join(","));
  const pending = $derived(deployedSig !== null && deployedSig !== currentSig);
  // Before any deploy this session we cannot prove the on-disk state, so treat an
  // un-deployed selection with conflicts as pending too (so Deploy is offered).
  const showPending = $derived(deployedSig === null ? mods.length > 0 : pending);

  const modName = (id: number): string =>
    mods.find((m) => m.id === id)?.name ?? `mod #${id}`;

  async function run<T>(
    label: string,
    fn: () => Promise<T>,
    quiet = false,
  ): Promise<T | undefined> {
    busy = true;
    error = null;
    status = null;
    try {
      const result = await fn();
      if (!quiet) status = `${label} ok`;
      return result;
    } catch (e) {
      error = `${label} failed: ${String(e)}`;
      return undefined;
    } finally {
      busy = false;
    }
  }

  /// Refresh the managed-game list.
  ///
  /// `quiet` suppresses the success banner: `run` reports the outcome of something the
  /// USER asked for, and the on-mount load is not that. Announcing "List games ok" to
  /// someone who has just opened the app is debug chatter, and it trains them to ignore
  /// the banner that matters when a real action succeeds or fails. Errors still surface.
  async function refreshManaged(quiet = false) {
    const games = await run("List games", api.listGames, quiet);
    if (games) managed = games;
  }

  // Fetch Starfield's CE2 status. Called on selecting Starfield and on the
  // explicit "Re-check" button — never on focus/mount, so there are no surprise reads.
  async function loadStarfieldStatus() {
    if (selectedAppid !== STARFIELD_APPID) return;
    const s = await run("Re-check Starfield", () => api.starfieldStatus(selectedAppid!));
    if (s) {
      starfield = s;
      // Once CE2 is Ready (config folder exists), read the loose-file activation state.
      if (!("FirstLaunchPending" in s.ce2_state)) await loadIniPreview();
    }
  }

  // --- StarfieldCustom.ini loose-file activation ---

  // Read-only: preview what activation would do (writes nothing). Drives the state line +
  // conflict box; called when CE2 is Ready and after every apply.
  async function loadIniPreview() {
    if (selectedAppid !== STARFIELD_APPID) return;
    const p = await run("Check loose-file loading", () =>
      api.previewIniActivation(selectedAppid!),
    );
    if (p) iniPreview = p;
  }

  // The "Enable" button ONLY opens the preview modal — it never writes.
  function openIniModal() {
    iniModalOpen = true;
  }

  // The modal confirm is the ONLY thing that authorizes the write.
  async function onActivateIni() {
    const outcome = await run("Enable loose-file loading", () =>
      api.applyIniActivation(selectedAppid!, "Block"),
    );
    if (outcome !== undefined) {
      iniModalOpen = false;
      await loadIniPreview();
    }
  }

  // Conflict resolution: replace the user's value with NexTwist's (reversible on purge).
  async function onUseNexTwistValue() {
    const outcome = await run("Enable loose-file loading", () =>
      api.applyIniActivation(selectedAppid!, "UseNexTwist"),
    );
    if (outcome !== undefined) await loadIniPreview();
  }

  // --- NexusMods account ---

  // The backend surfaces the no-keyring hard-fail as a distinct error string
  // containing "no keyring backend"; the UI keys its destructive banner on that.
  function isNoKeyring(e: unknown): boolean {
    return String(e).toLowerCase().includes("no keyring backend");
  }

  async function loadAccount() {
    try {
      userInfo = await api.accountInfo();
    } catch (e) {
      // Account read failing is non-fatal (stay logged-out); surface for visibility.
      error = `Account info failed: ${String(e)}`;
    }
  }

  async function onLoginApiKey() {
    if (!apiKeyInput) {
      error = "Paste your NexusMods API key first.";
      return;
    }
    busy = true;
    error = null;
    status = null;
    try {
      userInfo = await api.loginWithApiKey(apiKeyInput);
      noKeyring = false;
      apiKeyInput = ""; // never keep the key in component state after use
      apiKeyReveal = false;
      status = "Logged in";
    } catch (e) {
      if (isNoKeyring(e)) {
        noKeyring = true; // block login behind the destructive banner
      } else {
        error = `Login failed: ${String(e)}. Try again, or use an API key instead.`;
      }
    } finally {
      busy = false;
    }
  }

  async function onLoginOAuth() {
    await run("Log in with NexusMods", api.loginOAuthStart);
  }

  async function onLogout() {
    const ok = await run("Log out", api.logout);
    if (ok !== undefined) {
      userInfo = null;
      confirmLogout = false;
    }
  }

  // --- NexusMods downloads. Everything is event-driven. ---

  /** Apply a `download://progress` event to the matching row. */
  function applyProgress(p: DownloadProgress) {
    const idx = downloads.findIndex((d) => d.id === p.id);
    if (idx === -1) return;
    const row = downloads[idx];
    row.downloaded = p.downloaded;
    row.total = p.total;
    if (p.state === "expired") {
      // §C.3: an expired free-user link is a Warning notice, NOT a Failed row.
      expiredLink = p.reason ?? "This download link has expired.";
      downloads.splice(idx, 1);
      return;
    }
    if (p.state === "ratelimited") {
      // A transient, auto-recoverable rate-limit pause. Show the
      // backoff notice and mark the row paused — NOT a terminal failure.
      rateLimited = true;
      row.state = "ratelimited";
      row.reason = p.reason ?? "rate limited; download will resume automatically";
      downloads[idx] = row;
      return;
    }
    if (p.state === "failed") {
      row.state = "failed";
      row.reason = p.reason ?? "unknown error";
    } else if (p.state === "downloading" || p.state === "extracting" || p.state === "done") {
      row.state = p.state;
      // A healthy tick means the backoff is over — clear the rate-limit notice.
      rateLimited = false;
    }
    // Re-assign so Svelte 5 reactivity sees the row mutation.
    downloads[idx] = row;
  }

  /** Begin a download: push a Downloading row, then call the backend (which streams). */
  async function onStartDownload(args: {
    appid: number;
    gameDomain: string;
    nexusModId: number;
    fileId: number;
    name: string;
    key?: string | null;
    expires?: string | null;
  }) {
    const id = crypto.randomUUID();
    expiredLink = null;
    const source = {
      appid: args.appid,
      gameDomain: args.gameDomain,
      nexusModId: args.nexusModId,
      fileId: args.fileId,
      key: args.key ?? null,
      expires: args.expires ?? null,
    };
    downloads = [
      ...downloads,
      { id, name: args.name, downloaded: 0, total: null, state: "downloading", source },
    ];
    try {
      await api.startDownload({
        id,
        appid: args.appid,
        gameDomain: args.gameDomain,
        nexusModId: args.nexusModId,
        fileId: args.fileId,
        key: args.key ?? null,
        expires: args.expires ?? null,
      });
      // Refresh the mod list so the staged Nexus mod appears as an ordinary ManagedMod.
      if (selectedAppid !== null) loadConflicts();
    } catch (e) {
      // The progress listener typically marks the row failed/expired; this is a fallback.
      const idx = downloads.findIndex((d) => d.id === id);
      if (idx !== -1) {
        downloads[idx] = { ...downloads[idx], state: "failed", reason: String(e) };
      }
    }
  }

  async function onCancelDownload(id: string) {
    await api.cancelDownload(id);
  }

  /** Show the non-blocking nxm:// arrival toast and auto-dismiss it after a short delay. */
  function showNxmToast() {
    nxmToast = "Download started from NexusMods";
    if (nxmToastTimer) clearTimeout(nxmToastTimer);
    nxmToastTimer = setTimeout(() => (nxmToast = null), 4000);
  }

  /**
   * An nxm:// link arrived: the shell already started the download server-side and
   * emits `download://progress` for the new row. Here we just confirm with the Success toast
   * and ensure a row exists so the user sees it appear even before the first progress tick.
   */
  function applyNxmArrival(a: api.NxmArrival) {
    showNxmToast();
    expiredLink = null;
    if (!downloads.some((d) => d.id === a.id)) {
      downloads = [
        ...downloads,
        {
          id: a.id,
          name: a.id,
          downloaded: 0,
          total: null,
          state: "downloading",
          // Store the non-secret coordinates the arrival carries so a Retry of
          // this nxm-originated row can re-issue the download. `appid: 0` is a sentinel the
          // backend resolves from `gameDomain` (it owns the domain→appid map). `key`/
          // `expires` are deliberately absent — a single-use free link can't be replayed, so
          // a free-user retry surfaces the §C.3 expired-link Warning instead of a silent
          // failure; a premium retry (no key needed) re-downloads cleanly.
          source: {
            appid: 0,
            gameDomain: a.game_domain,
            nexusModId: a.mod_id,
            fileId: a.file_id,
          },
        },
      ];
    }
    // Refresh the mod list so the staged Nexus mod shows up as an ordinary ManagedMod.
    if (selectedAppid !== null) loadConflicts();
  }

  /** An expired/invalid nxm:// link: show the Warning, never a Failed row. */
  function applyNxmExpired(x: api.NxmExpired) {
    expiredLink = x.reason;
  }

  async function onDetect() {
    const games = await run("Detect games", api.detectGames);
    if (games) detected = games;
  }

  async function onAdd(appid: number) {
    const game = await run("Add game", () => api.addGame(appid));
    if (game) {
      await refreshManaged();
      selectedAppid = game.appid;
    }
  }

  async function onAddByFolder() {
    if (!folderPath) {
      error = "Enter a game folder path first.";
      return;
    }
    const game = await run("Add game by folder", () =>
      api.addGameByFolder(folderPath, folderAppid),
    );
    if (game) {
      await refreshManaged();
      selectedAppid = game.appid;
    }
  }

  async function onInstall() {
    if (selectedAppid === null) return;
    if (!archivePath) {
      error = "Enter an archive path (.zip/.7z/.rar) first.";
      return;
    }
    // Probe for a guided installer first. A successful parse opens the wizard;
    // a "no fomod/ModuleConfig.xml" archive (or any parse error) falls back to the plain
    // install path — the UI never half-opens a broken wizard (§A.8).
    await tryOpenFomodWizard(selectedAppid!, archivePath);
  }

  /** Plain (non-guided) install — the direct path (also the no-FOMOD fallback). */
  async function onPlainInstall() {
    if (selectedAppid === null || !archivePath) return;
    closeFomodWizard();
    const result = await run("Install mod", () =>
      api.installArchive(selectedAppid!, archivePath),
    );
    if (result) staged = result;
  }

  // --- FOMOD guided installer. Everything routes through api.ts; the engine
  //     owns parse/condition/resolve. The wizard only renders + gates. ---

  /** The steps currently VISIBLE. A step with a `<visible>` condition is re-evaluated by
   *  the engine via resolveFomod; until we have a live answer we show authored steps and
   *  rely on the dry-run resolve for the file-plan truth. (Live visibility skipping is a
   *  human-UAT item — see SUMMARY.) */
  const fomodVisibleSteps = $derived(fomodProj?.steps ?? []);
  const fomodStep = $derived(fomodVisibleSteps[fomodStepIdx] ?? null);
  const fomodIsLastStep = $derived(
    fomodProj !== null && fomodStepIdx >= fomodVisibleSteps.length - 1,
  );
  const fomodOnFirstStep = $derived(fomodStepIdx === 0);

  /** The selection payload the engine consumes (chosen identities + accumulated flags). */
  function fomodSelection(): api.FomodSelection {
    return fomodLogic.selectionOf(fomodProj, fomodChosen);
  }

  /** Whether the current step's group-selection constraints (min/max) are satisfied. */
  const fomodStepValid = $derived(fomodLogic.stepValid(fomodStep, fomodChosen));

  /** Try to open the FOMOD wizard for `archive`; fall back to plain install on no-FOMOD. */
  async function tryOpenFomodWizard(appid: number, archive: string) {
    busy = true;
    error = null;
    status = null;
    fomodFallback = null;
    try {
      const proj = await api.parseFomod(appid, archive);
      // Success — open the guided wizard.
      fomodArchive = archive;
      fomodProj = proj;
      fomodStepIdx = 0;
      fomodPreview = null;
      // Pre-select Required options (the engine installs a plugin's files only when its
      // option is selected; Required is author-locked-on).
      fomodChosen = fomodLogic.preselected(proj);
      status = "FOMOD installer opened";
    } catch (e) {
      // §A.8: a missing fomod/ModuleConfig.xml is the common "plain mod" case — install it
      // directly with no error. A genuinely malformed FOMOD surfaces the verbatim reason +
      // the plain-mod fallback offer.
      const reason = String(e);
      if (reason.toLowerCase().includes("no fomod/moduleconfig.xml")) {
        const result = await run("Install mod", () => api.installArchive(appid, archive));
        if (result) staged = result;
      } else {
        // Couldn't read this mod's FOMOD installer: {reason}. (offer plain-mod fallback)
        fomodFallback = reason;
      }
    } finally {
      busy = false;
    }
  }

  function fomodToggle(step: string, group: string, option: string, groupType: string) {
    fomodChosen = fomodLogic.toggle(fomodChosen, fomodStep, step, group, option, groupType);
    // Live re-eval: the conditional file plan / type-states change with the flag set.
    fomodPreview = null; // a choice invalidates a previously-shown preview (must re-resolve)
  }

  function fomodNext() {
    if (!fomodStepValid) return;
    if (fomodStepIdx < fomodVisibleSteps.length - 1) fomodStepIdx += 1;
  }

  function fomodBack() {
    if (fomodStepIdx > 0) fomodStepIdx -= 1;
  }

  /** The dry-run preview HARD GATE: resolve the plan BEFORE any staging write and show
   *  it. An unresolvable selection throws, so a preview shown is a plan safe to install. */
  async function fomodShowPreview() {
    if (selectedAppid === null || !fomodArchive) return;
    const preview = await run("Resolve FOMOD", () =>
      api.resolveFomod(selectedAppid!, fomodArchive!, fomodSelection()),
    );
    if (preview) fomodPreview = preview;
  }

  /** Apply the confirmed install: stage the validated archive + record it. */
  async function fomodApply() {
    if (selectedAppid === null || !fomodArchive || !fomodProj) return;
    fomodApplying = true;
    const name = fomodProj.module_name || "FOMOD mod";
    const result = await run("Install FOMOD mod", () =>
      api.applyFomod(selectedAppid!, fomodArchive!, name, fomodSelection()),
    );
    fomodApplying = false;
    if (result) {
      closeFomodWizard();
      // The new ManagedMod appears in the existing mod list (loadConflicts reloads mods).
      if (selectedAppid !== null) await loadConflicts();
    }
  }

  function closeFomodWizard() {
    fomodArchive = null;
    fomodProj = null;
    fomodStepIdx = 0;
    fomodChosen = new Set();
    fomodPreview = null;
    fomodApplying = false;
  }

  async function onDeploy() {
    if (selectedAppid === null || !staged) return;
    const result = await run("Deploy", () => api.deploy(selectedAppid!, staged!));
    if (result) deployReport = result;
  }

  async function onPurge() {
    if (selectedAppid === null) return;
    const result = await run("Purge", () => api.purge(selectedAppid!));
    if (result) purgeReport = result;
  }

  async function onVerify() {
    if (selectedAppid === null) return;
    const result = await run("Verify", () => api.verify(selectedAppid!));
    if (result) verifyReport = result;
    // Alongside the Data/-hash verify, classify the prefix-AppData plugins.txt
    // against recorded intent (Starfield only). The engine returns the InSync/Drift verdict.
    if (isStarfield) {
      const rec = await run("Reconcile plugins", () =>
        api.reconcilePlugins(selectedAppid!),
      );
      if (rec !== undefined) reconcileState = rec;
    }
  }

  // --- Conflict view ---

  async function loadConflicts() {
    if (selectedAppid === null) return;
    const ms = await run("List mods", () => api.listMods(selectedAppid!));
    if (ms) mods = ms;
    const cs = await run("List conflicts", () => api.listConflicts(selectedAppid!));
    if (cs) conflicts = cs;
  }

  // Reorder by swapping ranks with the neighbor (▲▼). Keyboard/click reorder is the
  // baseline path (no DnD-only) Pending until Deploy.
  async function onReorder(index: number, dir: -1 | 1) {
    if (selectedAppid === null) return;
    const other = index + dir;
    if (other < 0 || other >= mods.length) return;
    const a = mods[index];
    const b = mods[other];
    const ok = await run("Set priority", async () => {
      await api.setModRank(selectedAppid!, a.id, b.rank);
      await api.setModRank(selectedAppid!, b.id, a.rank);
      return true;
    });
    if (ok) await loadConflicts();
  }

  async function onDeployWinners() {
    if (selectedAppid === null) return;
    const result = await run("Deploy winner set", () =>
      api.deployWinnerSet(selectedAppid!),
    );
    if (result) {
      deployReport = result;
      // Record the deployed signature so the pending banner clears.
      deployedSig = currentSig;
    }
  }

  // --- Plugin manager ---

  // Masters-first / protected-master rules live in $lib/plugins (pure + unit-tested).
  const isMaster = order.isMaster;
  const kindBadge = order.kindBadge;
  const violatesMastersFirst = (i: number, dir: -1 | 1) =>
    order.violatesMastersFirst(plugins, i, dir);

  async function loadPlugins() {
    if (selectedAppid === null) return;
    sortProposal = null;
    mastersFirstError = null;
    const ps = await run("List plugins", () => api.listPlugins(selectedAppid!));
    if (ps) plugins = ps;
  }

  // Reorder a plugin by swapping with its neighbor (▲▼ / keyboard). A move that would
  // violate masters-first is refused with the §B.2 inline warning (controls are also
  // disabled for these, this is the defense-in-depth path).
  async function onPluginReorder(i: number, dir: -1 | 1) {
    // Protected masters are engine-locked; the UI is the courtesy layer, the engine
    // rejects a protected reorder authoritatively.
    const other = i + dir;
    if (other < 0 || other >= plugins.length) return;
    const next = order.reorder(plugins, i, dir);
    if (!next) {
      if (!order.touchesProtected(plugins, i, dir)) {
        mastersFirstError = "Masters must load before regular plugins.";
      }
      return;
    }
    mastersFirstError = null;
    plugins = next;
  }

  async function onPluginToggle(name: string, enabled: boolean) {
    if (selectedAppid === null) return;
    // Protected masters are always active and engine-locked — never toggle.
    if (plugins.find((p) => p.name === name)?.protected) return;
    const ok = await run("Set plugin enabled", () =>
      api.setPluginEnabled(selectedAppid!, name, enabled),
    );
    if (ok !== undefined) {
      plugins = plugins.map((p) => (p.name === name ? { ...p, enabled } : p));
    }
  }

  async function onSavePluginOrder() {
    if (selectedAppid === null) return;
    // Re-index order to the current display order before persisting.
    const ordered = plugins.map((p, idx) => ({ ...p, order: idx }));
    const path = await run("Save plugin order", () =>
      api.savePluginOrder(selectedAppid!, ordered),
    );
    if (path) status = `Wrote plugins.txt at ${path}`;
  }

  async function onSortWithLoot() {
    if (selectedAppid === null) return;
    const proposal = await run("Sort with LOOT", () =>
      api.sortWithLoot(selectedAppid!),
    );
    if (proposal) sortProposal = proposal;
  }

  // Apply a LOOT proposal into the editable list (only on explicit confirm). The
  // proposed name order is materialized into `plugins`, preserving each plugin's
  // kind/enabled; unknown names (shouldn't happen) are dropped, missing ones appended.
  function onApplySortedOrder() {
    if (!sortProposal) return;
    const byName = new Map(plugins.map((p) => [p.name, p]));
    const applied: PluginInfo[] = [];
    for (const name of sortProposal.proposed) {
      const p = byName.get(name);
      if (p) {
        applied.push(p);
        byName.delete(name);
      }
    }
    // Any plugins not named in the proposal keep their relative order at the end.
    for (const p of plugins) if (byName.has(p.name)) applied.push(p);
    plugins = applied.map((p, idx) => ({ ...p, order: idx }));
    sortProposal = null;
    status = "Applied sorted order — review, then Save plugin order to write it.";
  }

  function onDiscardSort() {
    sortProposal = null;
  }

  // --- Profiles ---

  async function loadProfiles() {
    if (selectedAppid === null) return;
    const ps = await run("List profiles", () => api.listProfiles(selectedAppid!));
    if (ps) profiles = ps;
  }

  async function onCreateProfile() {
    if (selectedAppid === null) return;
    const name = newProfileName.trim();
    if (!name) {
      error = "Enter a profile name first.";
      return;
    }
    const created = await run("Create profile", () =>
      api.createProfile(selectedAppid!, name),
    );
    if (created) {
      newProfileName = "";
      await loadProfiles();
    }
  }

  // Selecting a different profile does NOT mutate disk — it opens the confirmation modal
  //. The actual switch runs only on confirm.
  function onSelectProfile(p: Profile) {
    if (p.active) return; // already the deployed profile
    switchTarget = p;
  }

  // Confirmed switch: run the safe-engine reconcile (purge old → deploy new → plugins.txt
  // → mark active), then reload profiles + the per-profile mod/plugin lists (§D.3).
  async function onConfirmSwitch() {
    const target = switchTarget;
    if (selectedAppid === null || !target) return;
    switchTarget = null;
    const report = await run("Switch profile", () =>
      api.switchProfile(selectedAppid!, target.id),
    );
    if (report) {
      switchReport = report;
      // Per-profile preservation (§D.3): the deployed set changed — reload the lists so the
      // conflict/plugin views reflect the new profile's set/order, and reset the pending
      // signature (the on-disk set now matches the freshly-deployed profile).
      await loadProfiles();
      await loadConflicts();
      await loadPlugins();
      deployedSig = currentSig;
    }
  }

  function onCancelSwitch() {
    switchTarget = null;
  }

  function onRequestDelete(p: Profile) {
    deleteTarget = p;
  }

  // Confirmed delete: removes the profile + its mod/plugin selections only. Staged mod
  // files are KEPT (shared staging). Idempotent at the store.
  async function onConfirmDelete() {
    const target = deleteTarget;
    if (selectedAppid === null || !target) return;
    deleteTarget = null;
    const ok = await run("Delete profile", () =>
      api.deleteProfile(selectedAppid!, target.id),
    );
    if (ok !== undefined) await loadProfiles();
  }

  function onCancelDelete() {
    deleteTarget = null;
  }

  // Highlight plugins the proposal would MOVE (their proposed index differs from current).
  const movedByLoot = $derived.by(() => {
    if (!sortProposal) return new Set<string>();
    const moved = new Set<string>();
    sortProposal.proposed.forEach((name, proposedIdx) => {
      const currentIdx = plugins.findIndex((p) => p.name === name);
      if (currentIdx !== -1 && currentIdx !== proposedIdx) moved.add(name);
    });
    return moved;
  });

  // --- Collections. Every action routes through api.ts; the resolve
  //     report and the destructive uninstall are the two consequence gates. ---

  /** §B.3: resolve the pasted manifest into the report BEFORE any download/disk write. */
  async function onResolveCollection() {
    if (selectedAppid === null) return;
    if (!collManifest.trim()) {
      error = "Paste the Collection manifest (collection.json) first.";
      return;
    }
    collDownload = null;
    collDeployReport = null;
    collPurgeReport = null;
    const report = await run("Resolve Collection", () =>
      api.resolveCollection(selectedAppid!, collManifest),
    );
    if (report) collResolve = report;
  }

  /** §B.4: accept the report and bulk-download the available set. The backend enforces the
   *  Premium gate FIRST; per-mod progress flows through the existing download stream. We
   *  pre-seed the per-mod rows so they appear immediately (the ids match the backend's). */
  async function onDownloadCollection() {
    if (selectedAppid === null || !collResolve) return;
    expiredLink = null;
    const result = await run("Download Collection", () =>
      api.downloadCollection({
        appid: selectedAppid!,
        manifestJson: collManifest,
        slug: collSlug.trim() || "collection",
        revision: collRevision,
      }),
    );
    if (result) {
      collDownload = result;
      manualStepsDismissed = false;
      // The staged collection mods now appear as ordinary managed mods.
      loadConflicts();
    }
  }

  /** §C.3: deploy the installed Collection via the existing profile-switch path. */
  async function onDeployCollection() {
    if (selectedAppid === null || !collDownload) return;
    const report = await run("Deploy Collection", () =>
      api.deployCollection(selectedAppid!, collDownload!.collection_id),
    );
    if (report) {
      collDeployReport = report;
      loadProfiles();
    }
  }

  /** §C.4: open the destructive uninstall confirm. */
  function onRequestUninstallCollection() {
    if (!collDownload) return;
    collUninstallTarget = {
      id: collDownload.collection_id,
      name: collResolve?.mods.length ? collSlug || "this Collection" : "this Collection",
    };
  }

  /** §C.4: on confirm, purge-to-pristine + drop profile + remove staged mods (reversible). */
  async function onConfirmUninstallCollection() {
    const target = collUninstallTarget;
    if (selectedAppid === null || !target) return;
    collUninstallTarget = null;
    const report = await run("Uninstall Collection", () =>
      api.uninstallCollection(selectedAppid!, target.id),
    );
    if (report) {
      collPurgeReport = report;
      collDownload = null;
      collDeployReport = null;
      loadConflicts();
      loadProfiles();
    }
  }

  function onCancelUninstallCollection() {
    collUninstallTarget = null;
  }

  const collStatusTag = (status: string): { label: string; cls: string } => {
    switch (status) {
      case "Available":
        return { label: "Available", cls: "tag-ok" };
      case "Archived":
        return { label: "Archived", cls: "tag-warn" };
      case "Unavailable":
        return { label: "Unavailable", cls: "tag-err" };
      default:
        return { label: "Manual step required", cls: "tag-warn" };
    }
  };

  const warningLabel = (w: string) =>
    w === "CrossDevice"
      ? "Cross-device staging (EXDEV): hardlink/reflink unavailable — using symlink/copy. Stage on the same filesystem for best safety."
      : "Filesystem case-folding not confirmed: mod path casing is normalized for Wine instead.";

  // When the selected game changes, reset the conflict view and reload its mods +
  // conflicts. `deployedSig` is reset because the on-disk deployed set is unknown for a
  // freshly-selected game this session.
  $effect(() => {
    const appid = selectedAppid;
    deployedSig = null;
    mods = [];
    conflicts = [];
    plugins = [];
    sortProposal = null;
    mastersFirstError = null;
    profiles = [];
    newProfileName = "";
    switchTarget = null;
    deleteTarget = null;
    switchReport = null;
    // Reset the Collections surface for the newly-selected game (its domain differs).
    collSlug = "";
    collRevision = 1;
    collManifest = "";
    collResolve = null;
    collDownload = null;
    collDeployReport = null;
    collPurgeReport = null;
    collUninstallTarget = null;
    manualStepsDismissed = false;
    starfield = null;
    iniPreview = null;
    iniModalOpen = false;
    if (appid !== null) {
      loadConflicts();
      loadPlugins();
      loadProfiles();
      if (appid === STARFIELD_APPID) loadStarfieldStatus();
    }
  });

  // Load any already-managed games + the current account on mount. Quiet: the user did
  // not ask for this, so it must not post a success banner.
  refreshManaged(true);
  loadAccount();

  // Subscribe to download progress events: the list is updated entirely off
  // these async events so the UI never freezes during a multi-GB download.
  api.onDownloadProgress(applyProgress);
  // Subscribe to nxm:// deep-link events: the arrival toast + the
  // expired/invalid-link Warning. The new download row arrives via the progress stream.
  api.onNxmArrival(applyNxmArrival);
  api.onNxmExpired(applyNxmExpired);
</script>

<main>
  <h1>NexTwist</h1>

  {#if busy}<p class="busy">Working…</p>{/if}
  {#if status}<p class="ok">{status}</p>{/if}
  {#if error}<p class="err">{error}</p>{/if}

  <!-- Account panel: logged-out / logged-in / no-keyring. A token or key
       is never rendered. -->
  <section class="account">
    <h2>Account</h2>

    {#if noKeyring}
      <!-- Hard-fail: no Secret Service backend. Login is blocked; NexTwist
           never falls back to a plaintext file. -->
      <div class="keyring-banner" role="alert">
        <strong>Can't store your login securely</strong>
        <p>
          NexTwist won't save your NexusMods credentials as plaintext. Enable a system
          keyring (GNOME Keyring or KWallet) and try again.
        </p>
      </div>
    {:else if loggedIn && userInfo}
      <p class="account-line">
        <span class="dot" aria-hidden="true">●</span>
        <strong class="username">{userInfo.name}</strong>
        <span class="tier">{userInfo.is_premium ? "Premium" : "Free"}</span>
      </p>
      {#if confirmLogout}
        <div class="confirm">
          <p><strong>Log out of NexusMods?</strong></p>
          <p class="muted">
            This clears your saved login from the system keyring. You'll need to log in
            again to download mods.
          </p>
          <button onclick={onLogout} disabled={busy}>Log out</button>
          <button onclick={() => (confirmLogout = false)} disabled={busy}>Cancel</button>
        </div>
      {:else}
        <button onclick={() => (confirmLogout = true)} disabled={busy}>Log out</button>
      {/if}
    {:else}
      <!-- Logged out -->
      <p class="muted">Log in to download mods from NexusMods.</p>
      <button class="cta" onclick={onLoginOAuth} disabled={busy}>
        Log in with NexusMods
      </button>
      <button class="link-btn" onclick={() => (apiKeyReveal = !apiKeyReveal)} disabled={busy}>
        Use an API key instead
      </button>
      {#if apiKeyReveal}
        <div class="apikey">
          <label>
            API key:
            <input
              type="password"
              bind:value={apiKeyInput}
              placeholder="Paste your NexusMods personal API key"
            />
          </label>
          <button onclick={onLoginApiKey} disabled={busy}>Save key</button>
        </div>
      {/if}
    {/if}
  </section>

  <!-- Downloads list: per-item progress, five row states, rate-limit notice,
       empty state. Driven entirely by async download://progress events. -->
  <section class="downloads">
    <h2>Downloads</h2>

    {#if nxmToast}
      <!-- §C.1: non-blocking arrival toast, Success styling, auto-dismisses. -->
      <div class="nxm-toast" role="status">
        {nxmToast}
        <button class="link-btn" onclick={() => (nxmToast = null)}>Dismiss</button>
      </div>
    {/if}

    {#if loggedIn && userInfo && !userInfo.is_premium}
      <p class="muted free-hint">
        Free account: start downloads from the NexusMods website "Mod Manager Download"
        button.
      </p>
    {:else if loggedIn && userInfo && userInfo.is_premium}
      <p class="muted free-hint">
        Premium account: you can start downloads in-app, or use the NexusMods website
        "Mod Manager Download" button.
      </p>
    {/if}

    {#if rateLimited}
      <div class="rate-notice" role="status">
        Pausing to respect NexusMods rate limits — downloads will resume automatically.
      </div>
    {/if}

    {#if expiredLink}
      <div class="rate-notice" role="status">
        This download link has expired. Re-open it from the NexusMods website.
        <button class="link-btn" onclick={() => (expiredLink = null)}>Dismiss</button>
      </div>
    {/if}

    {#if downloads.length === 0}
      <div class="empty">
        <strong>No downloads yet</strong>
        <p class="muted">
          Use the Log in panel, then start a download from NexusMods. Free accounts: use
          the website "Mod Manager Download" button.
        </p>
      </div>
    {:else}
      <ul class="download-list">
        {#each downloads as d (d.id)}
          <li class="download-row" class:failed={d.state === "failed"}>
            <code class="dl-name">{d.name}</code>
            <div class="bar-track">
              <div
                class="bar-fill"
                class:indeterminate={d.state === "extracting"}
                style={`width: ${d.state === "extracting" || d.state === "done" ? 100 : (pct(d) ?? 0)}%`}
              ></div>
            </div>
            <div class="dl-meta">
              {#if d.state === "queued"}
                <span class="muted">Queued</span>
              {:else if d.state === "downloading"}
                <span class="muted">
                  {pct(d) !== null ? `${pct(d)}% · ` : ""}{fmtBytes(d.downloaded)}{d.total
                    ? ` / ${fmtBytes(d.total)}`
                    : ""}
                </span>
                <button onclick={() => onCancelDownload(d.id)}>Cancel</button>
              {:else if d.state === "extracting"}
                <span class="muted">Extracting…</span>
              {:else if d.state === "ratelimited"}
                <!-- A transient rate-limit pause, not a failure. -->
                <span class="muted">Paused — respecting NexusMods rate limits…</span>
              {:else if d.state === "done"}
                <span class="done">✓ Done — added to staging, ready to deploy</span>
              {:else if d.state === "failed"}
                <span class="err">Download failed: {d.reason}.</span>
                <button
                  onclick={() =>
                    onStartDownload({
                      appid: d.source.appid,
                      gameDomain: d.source.gameDomain,
                      nexusModId: d.source.nexusModId,
                      fileId: d.source.fileId,
                      name: d.name,
                      key: d.source.key,
                      expires: d.source.expires,
                    })}
                >
                  Retry
                </button>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  {#if selectedGame}
    <!-- Collections. The resolve report is a HARD GATE shown
         BEFORE any download; the Premium gate replaces the controls for a free account. -->
    <section class="collections">
      <h2>Collections — {selectedGame.name}</h2>

      {#if !loggedIn}
        <p class="muted">Log in above to browse and install Collections.</p>
      {:else if !isPremium}
        <!-- §B.1 Premium gate: a single factual Warning notice, not a nag. -->
        <div class="warn coll-premium">
          <strong>Collections require a NexusMods Premium account</strong>
          <p class="muted">
            Bulk Collection downloads use NexusMods direct download, which is Premium-only.
            You can still install single mods from the website on a free account.
          </p>
        </div>
      {:else}
        <!-- §B.2 Browse/select: slug/URL + revision for the selected game's domain. -->
        <div class="conflict-toolbar coll-pick">
          <input bind:value={collSlug} placeholder="Collection slug or URL" />
          <input
            type="number"
            min="1"
            bind:value={collRevision}
            title="Revision"
            class="coll-rev" />
        </div>
        <label class="coll-manifest-label">
          Collection manifest (collection.json):
          <textarea
            bind:value={collManifest}
            rows="3"
            placeholder="Paste the Collection revision's collection.json here"
          ></textarea>
        </label>
        <button class="cta" onclick={onResolveCollection} disabled={busy || !collManifest.trim()}>
          Resolve Collection
        </button>

        {#if !collResolve && !collDownload}
          <!-- §B.5 empty / no-selection state. -->
          <div class="empty">
            <strong>No Collection selected</strong>
            <p class="muted">
              Paste a NexusMods Collection link for {selectedGame.name} to see exactly what
              it installs before anything downloads.
            </p>
          </div>
        {/if}

        {#if collResolve}
          <!-- §B.3 Resolve report (HARD GATE): rendered BEFORE any download. -->
          <div class="report coll-resolve">
            <h4>Resolve report</h4>
            <p class="muted coll-summary">
              {collResolveCounts.available} available · {collResolveCounts.archived} archived
              · {collResolveCounts.unavailable} unavailable · {collResolveCounts.manual}
              manual step(s)
            </p>
            <ul class="coll-mod-list">
              {#each collResolve.mods as m (m.name + m.version)}
                {@const tag = collStatusTag(m.status)}
                <li class="coll-mod-row">
                  <span class="coll-mod-name">{m.name}</span>
                  <code class="coll-mod-ver">{m.version}</code>
                  <span class="coll-tag {tag.cls}">{tag.label}</span>
                </li>
              {/each}
            </ul>
            <p class="muted coll-skip-note">
              Unavailable mods are skipped; manual-step mods are listed for you to install
              yourself.
            </p>
            <!-- §B.4 Accept & download: the only accent CTA on this screen. -->
            <button class="cta" onclick={onDownloadCollection} disabled={busy}>
              Download Collection
            </button>
          </div>
        {/if}

        {#if rateLimited && collDownloadRows.length > 0}
          <div class="rate-notice" role="status">
            Pausing to respect NexusMods rate limits — downloads will resume automatically.
          </div>
        {/if}

        {#if collDownloadRows.length > 0}
          <!-- §B.4 per-mod + overall bulk progress. -->
          <div class="coll-progress">
            <p class="muted">
              Downloading {collDownloadRows.filter((d) => d.state === "done").length} of
              {collDownloadRows.length} mods…
            </p>
            <div class="bar-track coll-overall">
              <div
                class="bar-fill"
                style={`width: ${Math.floor(
                  (collDownloadRows.filter((d) => d.state === "done").length /
                    collDownloadRows.length) *
                    100,
                )}%`}
              ></div>
            </div>
            <ul class="download-list">
              {#each collDownloadRows as d (d.id)}
                <li class="download-row" class:failed={d.state === "failed"}>
                  <code class="dl-name">{d.name}</code>
                  <div class="bar-track">
                    <div
                      class="bar-fill"
                      class:indeterminate={d.state === "extracting"}
                      style={`width: ${d.state === "extracting" || d.state === "done" ? 100 : (pct(d) ?? 0)}%`}
                    ></div>
                  </div>
                  <div class="dl-meta">
                    {#if d.state === "downloading"}
                      <span class="muted">
                        {pct(d) !== null ? `${pct(d)}% · ` : ""}{fmtBytes(d.downloaded)}
                      </span>
                    {:else if d.state === "extracting"}
                      <span class="muted">Extracting…</span>
                    {:else if d.state === "ratelimited"}
                      <span class="muted">Paused — respecting NexusMods rate limits…</span>
                    {:else if d.state === "done"}
                      <span class="done">✓ Done — staged</span>
                    {:else if d.state === "failed"}
                      <span class="err">Download failed: {d.reason}.</span>
                      <button
                        onclick={() =>
                          onStartDownload({
                            appid: d.source.appid,
                            gameDomain: d.source.gameDomain,
                            nexusModId: d.source.nexusModId,
                            fileId: d.source.fileId,
                            name: d.name,
                            key: d.source.key,
                            expires: d.source.expires,
                          })}
                      >
                        Retry
                      </button>
                    {/if}
                  </div>
                </li>
              {/each}
            </ul>
          </div>
        {/if}

        {#if collDownload}
          <!-- §C: the installed Collection — manual steps, deploy, uninstall. -->
          <div class="report coll-installed">
            <h4>Collection installed — ready to deploy.</h4>
            {#if collDownload.failed.length > 0}
              <div class="warn">
                <strong>Some mods could not be downloaded:</strong>
                <ul>
                  {#each collDownload.failed as [name, reason] (name)}
                    <li><span class="coll-mod-name">{name}</span>: {reason}</li>
                  {/each}
                </ul>
              </div>
            {/if}

            {#if collDownload.stale_choices.length > 0}
              <div class="warn">
                <strong>Some mods' FOMOD choices no longer match — install them manually:</strong>
                <ul>
                  {#each collDownload.stale_choices as [name, reason] (name)}
                    <li><span class="coll-mod-name">{name}</span>: {reason}</li>
                  {/each}
                </ul>
              </div>
            {/if}

            {#if collDownload.manual_steps.length > 0 && !manualStepsDismissed}
              <!-- §C.2 persistent manual-steps panel (dismissable per-Collection). -->
              <div class="warn coll-manual">
                <strong>Manual steps remaining</strong>
                <ul>
                  {#each collDownload.manual_steps as step (step.name)}
                    <li>
                      <span class="coll-mod-name">{step.name}</span>
                      {#if step.instructions}— {step.instructions}{/if}
                      {#if step.url}
                        — <code>{step.url}</code>
                      {/if}
                    </li>
                  {/each}
                </ul>
                <button class="link-btn" onclick={() => (manualStepsDismissed = true)}>
                  Dismiss
                </button>
              </div>
            {/if}

            <div class="coll-actions">
              <!-- §C.3 Deploy: Accent, the existing profile-switch path. -->
              <button class="cta" onclick={onDeployCollection} disabled={busy}>Deploy</button>
              <!-- §C.4 Uninstall: neutral affordance → Destructive-red confirm modal. -->
              <button class="danger-link" onclick={onRequestUninstallCollection} disabled={busy}>
                Uninstall Collection
              </button>
            </div>

            {#if collDeployReport}
              <p class="muted">
                Purged {collDeployReport.purged.removed} file(s) → deployed
                {collDeployReport.deployed.deployed} file(s). The Collection's modded game is
                ready to launch.
              </p>
            {/if}
          </div>
        {/if}

        {#if collPurgeReport}
          <!-- §C.4 post-uninstall: pristine result. -->
          <div class="report">
            <h4>
              {collPurgeReport.orphans.length === 0
                ? "Collection removed — game folder is pristine."
                : "Collection removed — but drift was detected."}
            </h4>
            <p class="muted">
              Restored {collPurgeReport.restored} file(s), removed
              {collPurgeReport.removed} deployed file(s).
            </p>
          </div>
        {/if}
      {/if}
    </section>
  {/if}

  <section>
    <h2>1. Detect games</h2>
    <button onclick={onDetect} disabled={busy}>Detect games</button>
    {#if detected.length === 0}
      <p class="muted">No detected games yet. Click Detect, or add one by folder below.</p>
    {:else}
      <ul>
        {#each detected as g (g.appid)}
          <li>
            <strong>{g.name}</strong> (AppID {g.appid}) — {g.library_path}
            <button onclick={() => onAdd(g.appid)} disabled={busy}>Add as managed</button>
          </li>
        {/each}
      </ul>
    {/if}

    <h3>Add game by folder (fallback for Snap / non-standard installs)</h3>
    <label>
      Game folder:
      <input bind:value={folderPath} placeholder="/path/to/steamapps/common/Skyrim Special Edition" />
    </label>
    <label>
      Title:
      <select bind:value={folderAppid}>
        {#each SUPPORTED as s (s.appid)}
          <option value={s.appid}>{s.name}</option>
        {/each}
      </select>
    </label>
    <button onclick={onAddByFolder} disabled={busy}>Add by folder</button>
  </section>

  <section>
    <h2>2. Managed games</h2>
    {#if managed.length === 0}
      <p class="muted">No managed games yet.</p>
    {:else}
      <ul>
        {#each managed as g (g.appid)}
          <li>
            <label>
              <input type="radio" name="managed" value={g.appid} bind:group={selectedAppid} />
              <strong>{g.name}</strong> (AppID {g.appid})
            </label>
            <div class="paths">
              <div>install: <code>{g.install_dir}</code></div>
              <div>prefix: <code>{g.prefix}</code></div>
              <div>staging: <code>{g.staging_dir}</code></div>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  {#if selectedGame}
    <!-- Starfield version-drift notice: persistent + advisory. It NEVER blocks
         management; it only appears when the installed build is newer than the validated
         baseline (dormant until a non-zero baseline is seeded). Build numbers render as
         plain text — Svelte escapes them, no {@html}. -->
    {#if isStarfield && starfield?.drift}
      <div class="drift-notice">
        <strong>Starfield build newer than validated</strong>
        <p>
          Installed build {starfield.drift.installed} is newer than the build NexTwist's
          Starfield support was validated against ({starfield.drift.validated}). Management
          stays fully available; some load-order/INI behavior is validated against the older
          build, so double-check results.
        </p>
      </div>
    {/if}

    <!-- StarfieldCustom.ini loose-file activation. Shown once CE2 is Ready
         (while pending, the §5 first-launch gate guides the user instead). Reuses the
         .first-launch info box + .badge state tag and the .warn conflict box — no new
         component or CSS. The state line is read off the read-only preview. -->
    {#if isStarfield && !starfieldPending && iniPreview}
      {#if iniConflict !== null}
        <!-- Surface 3: a pre-existing non-empty user value blocks auto-activation.
             Amber caution (reversible), never a silent clobber — explicit keep/replace. -->
        <div class="warn">
          <strong>StarfieldCustom.ini already sets a loose-file path.</strong>
          <p>Your file sets <code>sResourceDataDirsFinal</code> to:</p>
          <pre class="txt-preview">{iniConflict}</pre>
          <p>
            NexTwist won't overwrite it silently. Keep your value, or replace it with
            NexTwist's (reversible — your original is restored on purge).
          </p>
          <div class="actions">
            <button onclick={loadIniPreview} disabled={busy}>Keep mine</button>
            <button onclick={onUseNexTwistValue} disabled={busy}>
              Use NexTwist's value
            </button>
          </div>
        </div>
      {:else}
        <!-- Surface 1: activation state tag + the Enable control. The button ONLY opens the
             preview modal (surface 2) — it never writes. -->
        <div class="first-launch">
          {#if iniActive}
            <span class="badge ok">● Loose-file loading active</span>
          {:else}
            <span class="badge muted">○ Loose-file loading not active</span>
            <p class="muted">
              Loose-file loading isn't set up yet. Deploy a mod, or enable it here, to let
              Starfield load loose files.
            </p>
            <button onclick={openIniModal} disabled={busy}>Enable loose-file loading</button>
          {/if}
        </div>
      {/if}
    {/if}

    <section>
      <h2>3. Install &amp; deploy — {selectedGame.name}</h2>

      <label>
        Mod archive (.zip / .7z / .rar):
        <input bind:value={archivePath} placeholder="/path/to/mod.zip" />
      </label>
      <button onclick={onInstall} disabled={busy}>Install mod from archive</button>

      {#if fomodFallback}
        <!-- §A.8: a malformed FOMOD never opens a half-broken wizard — show the verbatim
             reason + the plain-mod fallback (Copywriting Contract). -->
        <div class="warn fomod-fallback">
          <p>
            <strong>⚠</strong> Couldn't read this mod's FOMOD installer:
            <code>{fomodFallback}</code>. You can install it as a plain mod instead.
          </p>
          <div class="actions">
            <button onclick={onPlainInstall} disabled={busy}>Install as a plain mod</button>
            <button onclick={() => (fomodFallback = null)} disabled={busy}>Cancel</button>
          </div>
        </div>
      {/if}

      {#if staged}
        <p class="ok">Staged {staged.files.length} file(s) at <code>{staged.staging_root}</code></p>
      {/if}

      <div class="actions">
        <button onclick={onDeploy} disabled={busy || !staged}>Deploy</button>
        <button onclick={onPurge} disabled={busy}>Purge</button>
        <button onclick={onVerify} disabled={busy}>Verify</button>
      </div>

      {#if deployReport}
        <div class="report">
          <h4>Deploy report</h4>
          <p>Deployed {deployReport.deployed} file(s), backed up {deployReport.backed_up} vanilla file(s).</p>
          {#if deployReport.methods.length > 0}
            <ul>
              {#each deployReport.methods as [path, method] (path)}
                <li><code>{path}</code> — <em>{method}</em></li>
              {/each}
            </ul>
          {/if}
          {#if deployReport.fs_warnings.length > 0}
            <div class="warn">
              <strong>Filesystem warnings:</strong>
              <ul>
                {#each deployReport.fs_warnings as w (w)}
                  <li>{warningLabel(w)}</li>
                {/each}
              </ul>
            </div>
          {/if}
        </div>
      {/if}

      {#if purgeReport}
        <div class="report">
          <h4>Purge report</h4>
          <p>Removed {purgeReport.removed}, restored {purgeReport.restored} vanilla file(s).</p>
          {#if purgeReport.orphans.length > 0}
            <div class="warn">
              <strong>Orphans (reported, never deleted):</strong>
              <ul>{#each purgeReport.orphans as o (o)}<li><code>{o}</code></li>{/each}</ul>
            </div>
          {:else}
            <p class="ok">Game folder is pristine — no orphans.</p>
          {/if}
        </div>
      {/if}

      {#if verifyReport}
        <div class="report">
          <h4>Verify report</h4>
          {#if verifyReport.pristine}
            <p class="ok">Pristine — the deployed files all match what NexTwist recorded.</p>
          {:else}
            <div class="warn">
              <strong>Drift detected in deployed files:</strong>
              <ul>
                <li>missing: {verifyReport.missing.length}</li>
                <li>changed: {verifyReport.changed.length}</li>
              </ul>
            </div>
          {/if}
          <!-- Unmanaged files are NOT drift: the set includes the whole vanilla game tree,
               so reporting them as a problem would cry wolf on every untouched install.
               They are listed because purge refuses to delete what it cannot explain. -->
          {#if verifyReport.orphans.length > 0}
            <p class="muted">
              {verifyReport.orphans.length} file{verifyReport.orphans.length === 1 ? "" : "s"}
              in the game folder are not managed by NexTwist (the vanilla game and anything
              you added yourself). These are never touched by deploy or purge.
            </p>
          {/if}
        </div>
      {/if}

      <!-- On-launch reconciliation (Starfield only). Renders the engine verdict:
           InSync is CALM (green, no diff); Drift lists only the beyond-expected names. -->
      {#if isStarfield && reconcileState}
        <div class="report">
          <h4>Load-order reconciliation</h4>
          {#if reconcileState === "InSync"}
            <p class="ok">In sync — Starfield applied its expected on-launch changes.</p>
          {:else}
            <div class="warn">
              <strong>Load order changed unexpectedly</strong>
              <ul>
                {#each reconcileState.Drift as name (name)}<li><code>{name}</code></li>{/each}
              </ul>
              <p class="muted">
                Re-apply your saved plugin order (Save plugin order in section 5) to restore it.
              </p>
            </div>
          {/if}
        </div>
      {/if}
    </section>

    <section>
      <h2>4. Conflicts &amp; priority — {selectedGame.name}</h2>

      <!-- Pending-vs-deployed banner -->
      {#if showPending}
        <div class="warn pending">
          <strong>Changes pending</strong>
          <p>Priority changes aren't on disk yet. Deploy to apply.</p>
        </div>
      {/if}

      <div class="conflict-toolbar">
        <button onclick={loadConflicts} disabled={busy}>Refresh</button>
        {#if showPending}
          <button class="cta" onclick={onDeployWinners} disabled={busy}>Deploy</button>
        {:else}
          <button disabled title="No pending priority changes">Up to date</button>
        {/if}
      </div>

      <!-- Mod priority list: top = highest priority = wins -->
      <h3>Mod priority <span class="muted">(top = highest priority = wins)</span></h3>
      {#if mods.length === 0}
        <p class="muted">No mods for this game yet. Install a mod above to set priority.</p>
      {:else}
        <ol class="priority">
          {#each mods as m, i (m.id)}
            <li class:disabled={!m.enabled}>
              <span class="reorder">
                <button
                  onclick={() => onReorder(i, -1)}
                  disabled={busy || i === 0}
                  aria-label="Increase priority of {m.name}"
                  title="Move up (higher priority)">▲</button>
                <button
                  onclick={() => onReorder(i, 1)}
                  disabled={busy || i === mods.length - 1}
                  aria-label="Decrease priority of {m.name}"
                  title="Move down (lower priority)">▼</button>
              </span>
              <span class="rank">{i + 1}.</span>
              <span class="mod-name">{m.name}</span>
              <span class="mod-state {m.enabled ? 'on' : 'off'}">
                {m.enabled ? "enabled" : "disabled"}
              </span>
            </li>
          {/each}
        </ol>
      {/if}

      <!-- File-level conflict table -->
      <h3>File conflicts</h3>
      {#if conflicts.length === 0}
        <div class="empty">
          <strong>No conflicts</strong>
          <p class="muted">
            No two enabled mods write the same file. Enable more mods or adjust priority
            to see conflicts here.
          </p>
        </div>
      {:else}
        <table class="conflicts">
          <thead>
            <tr>
              <th>File (target)</th>
              <th>Provided by</th>
              <th>Winner</th>
            </tr>
          </thead>
          <tbody>
            {#each conflicts as c (c.target_rel)}
              <tr>
                <td><code>{c.target_rel}</code></td>
                <td class="providers">
                  {#each c.providers as p, pi (p)}
                    <span class:loser={p !== c.winner}
                      >{modName(p)}{pi < c.providers.length - 1 ? ", " : ""}</span>
                  {/each}
                </td>
                <td class="winner">
                  <span class="dot" aria-hidden="true">●</span>
                  <span class="winner-name">{modName(c.winner)}</span>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </section>

    <section>
      <h2>5. Plugins &amp; load order — {selectedGame.name}</h2>

      {#if isStarfield && starfieldPending}
        <!-- First-launch gate: Starfield hasn't created its CE2 config folder
             yet, so load-order/INI management is blocked. Guide the user to launch once via
             Steam (we do NOT auto-launch the Proton game), then Re-check on explicit click. -->
        <div class="first-launch">
          <strong>Launch Starfield once before managing load order</strong>
          <p>
            Starfield hasn't created its config folder yet, so load-order and INI management
            are unavailable. Launch Starfield once via Steam so it creates its config folder,
            then click Re-check.
          </p>
          {#if ce2Path}<p class="muted">Expected config folder: <code>{ce2Path}</code></p>{/if}
          <button class="cta" onclick={loadStarfieldStatus} disabled={busy}>Re-check</button>
        </div>
      {:else}
      <div class="conflict-toolbar">
        <button onclick={loadPlugins} disabled={busy}>Refresh</button>
        <button onclick={onSortWithLoot} disabled={busy || plugins.length === 0}>
          Sort with LOOT
        </button>
        <button class="cta" onclick={onSavePluginOrder} disabled={busy || plugins.length === 0}>
          Save plugin order
        </button>
      </div>

      <!-- Masterlist-age note: informational, muted grey — NOT a caution (Starfield). -->
      {#if isStarfield && sortProposal?.masterlist_date}
        <p class="muted masterlist-age">
          Masterlist from {sortProposal.masterlist_date} — may be stale
        </p>
      {/if}

      <!-- LOOT proposal review: no silent apply -->
      {#if sortProposal}
        <div class="loot-proposal">
          {#if sortProposal.warnings.length > 0}
            <div class="warn">
              <strong>LOOT warnings</strong>
              <ul>
                {#each sortProposal.warnings as w, wi (wi)}<li>{w}</li>{/each}
              </ul>
            </div>
          {/if}
          <h3>Proposed order <span class="muted">(moved plugins highlighted)</span></h3>
          <ol class="proposed">
            {#each sortProposal.proposed as name, pi (name)}
              <li class:moved={movedByLoot.has(name)}>
                <span class="rank">{pi + 1}.</span>
                <span class="mono">{name}</span>
                {#if movedByLoot.has(name)}<span class="moved-tag">moved</span>{/if}
              </li>
            {/each}
          </ol>
          <div class="actions">
            <button class="cta" onclick={onApplySortedOrder} disabled={busy}>
              Apply sorted order
            </button>
            <button onclick={onDiscardSort} disabled={busy}>Discard</button>
          </div>
        </div>
      {/if}

      {#if mastersFirstError}
        <div class="warn"><strong>⚠</strong> {mastersFirstError}</div>
      {/if}

      <!-- Plugin list, masters-first grouped -->
      {#if plugins.length === 0}
        <div class="empty">
          <strong>No plugins found</strong>
          <p class="muted">
            No .esp/.esm/.esl files in the enabled mods or game Data folder. Install or
            enable a mod that adds plugins.
          </p>
        </div>
      {:else}
        <ol class="priority plugins">
          {#each plugins as p, i (p.name)}
            {#if i === 0 || isMaster(plugins[i - 1]) !== isMaster(p)}
              <li class="group-divider" aria-hidden="true">
                {isMaster(p) ? "Masters (load first)" : "Regular plugins"}
              </li>
            {/if}
            <li class:disabled={!p.enabled} class:protected={p.protected}>
              <span class="reorder">
                <button
                  onclick={() => onPluginReorder(i, -1)}
                  disabled={busy || i === 0 || p.protected || violatesMastersFirst(i, -1)}
                  aria-label="Move {p.name} up"
                  title="Move up">▲</button>
                <button
                  onclick={() => onPluginReorder(i, 1)}
                  disabled={busy || i === plugins.length - 1 || p.protected || violatesMastersFirst(i, 1)}
                  aria-label="Move {p.name} down"
                  title="Move down">▼</button>
              </span>
              <label class="plugin-toggle">
                <input
                  type="checkbox"
                  checked={p.enabled || p.protected}
                  disabled={busy || p.protected}
                  onchange={(e) => onPluginToggle(p.name, e.currentTarget.checked)} />
              </label>
              <span class="mono mod-name">{p.name}</span>
              <span class="badge badge-{p.kind}">{kindBadge(p.kind)}</span>
              {#if p.medium}
                <span
                  class="badge badge-medium"
                  aria-label="Medium master (CE2) — loads in the medium-master tier, between full masters and regular plugins."
                  title="Medium master (CE2) — loads in the medium-master tier, between full masters and regular plugins."
                  >MEDIUM</span>
              {/if}
              {#if p.protected}
                <!-- Focusable, non-interactive lock affordance carries the reason into the a11y
                     tree — a bare disabled control alone would not announce it. The
                     tabindex is a deliberate focusable status affordance. -->
                <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
                <span
                  class="badge badge-protected"
                  tabindex="0"
                  role="img"
                  aria-label="Protected master — Starfield requires this and manages its position. It can't be reordered or disabled."
                  title="Protected master — Starfield requires this and manages its position. It can't be reordered or disabled."
                  >🔒 PROTECTED</span>
              {/if}
            </li>
          {/each}
        </ol>

        <!-- plugins.txt preview: read-only asterisk output -->
        <h3>plugins.txt preview <span class="muted">(asterisk = enabled)</span></h3>
        <pre class="txt-preview">{plugins
            .filter((p) => p.kind !== "esm")
            .map((p) => (p.enabled ? "*" : "") + p.name)
            .join("\n")}</pre>
      {/if}
      {/if}
    </section>

    <!-- Profiles: selector + confirmation-gated switch/delete -->
    <section>
      <h2>6. Profiles — {selectedGame.name}</h2>

      <div class="conflict-toolbar">
        <input
          bind:value={newProfileName}
          placeholder="New profile name"
          onkeydown={(e) => e.key === "Enter" && onCreateProfile()} />
        <button class="cta" onclick={onCreateProfile} disabled={busy || !newProfileName.trim()}>
          Create profile
        </button>
      </div>

      {#if profiles.length === 0}
        <div class="empty">
          <strong>Default profile</strong>
          <p class="muted">
            This game has one profile. Create another to keep separate mod/plugin setups.
          </p>
        </div>
      {:else}
        <!-- Profile selector (§D.1): active = deployed, marked with the Accent indicator -->
        <ul class="priority profiles">
          {#each profiles as p (p.id)}
            <li class:active={p.active}>
              {#if p.active}
                <span class="active-dot" aria-hidden="true">●</span>
                <span class="prof-name">{p.name}</span>
                <span class="active-label">active</span>
              {:else}
                <span class="active-dot placeholder" aria-hidden="true">○</span>
                <button class="prof-select" onclick={() => onSelectProfile(p)} disabled={busy}>
                  {p.name}
                </button>
              {/if}
              <span class="prof-actions">
                <button
                  class="danger-link"
                  onclick={() => onRequestDelete(p)}
                  disabled={busy}
                  title="Delete profile">Delete</button>
              </span>
            </li>
          {/each}
        </ul>
        <p class="muted prof-hint">
          Each profile keeps its own enabled-mod set, priority order, and plugin order.
          Switching reloads those lists.
        </p>
      {/if}

      {#if switchReport}
        <div class="report">
          <h4>Switch report</h4>
          <p>
            Purged {switchReport.purged.removed} file(s) → deployed
            {switchReport.deployed.deployed} file(s). The game stayed reversible across the
            switch.
          </p>
          <p class="muted">plugins.txt: <code>{switchReport.plugins_txt}</code></p>
        </div>
      {/if}
    </section>
  {/if}

  <!-- No-silent-edit preview + confirm. Shows the exact lines + provenance
       BEFORE any write; ONLY this confirm authorizes the write. Same .overlay/.modal gate
       discipline as the profile-switch / FOMOD dry-run below. -->
  {#if iniModalOpen && iniPreview}
    <div class="overlay" role="dialog" aria-modal="true" aria-labelledby="ini-title">
      <div class="modal">
        <h3 id="ini-title">Enable loose-file loading</h3>
        <p>NexTwist will ensure these lines under <code>[Archive]</code>:</p>
        <pre class="txt-preview">[Archive]
{iniPreview.lines.join("\n")}</pre>
        <p class="muted">
          {#if iniPreview.will_create}
            NexTwist will create StarfieldCustom.ini (it does not exist yet).
          {:else}
            NexTwist will edit your existing StarfieldCustom.ini — only these two lines are
            added; every other section, key, comment, and line ending is preserved.
          {/if}
        </p>
        <div class="actions">
          <button class="cta" onclick={onActivateIni} disabled={busy}>
            Activate loose-file loading
          </button>
          <button onclick={() => (iniModalOpen = false)} disabled={busy}>Cancel</button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Confirmation-gated profile switch. Disk is mutated
       ONLY on confirm; the safe engine runs purge-old → deploy-new. -->
  {#if switchTarget}
    <div class="overlay" role="dialog" aria-modal="true" aria-labelledby="switch-title">
      <div class="modal">
        <h3 id="switch-title">Switch to "{switchTarget.name}"?</h3>
        <p>
          This purges the current deployment and deploys "{switchTarget.name}". Your game
          stays fully reversible. Continue?
        </p>
        <div class="actions">
          <button class="cta" onclick={onConfirmSwitch} disabled={busy}>Switch</button>
          <button onclick={onCancelSwitch} disabled={busy}>Cancel</button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Destructive delete confirmation (Copywriting): Destructive-red confirm; staged
       mod files are kept. -->
  {#if deleteTarget}
    <div class="overlay" role="dialog" aria-modal="true" aria-labelledby="delete-title">
      <div class="modal">
        <h3 id="delete-title">Delete "{deleteTarget.name}"?</h3>
        <p>
          This removes the profile and its mod/plugin selections. Staged mod files are
          kept. This can't be undone.
        </p>
        <div class="actions">
          <button class="destructive" onclick={onConfirmDelete} disabled={busy}>Delete</button>
          <button onclick={onCancelDelete} disabled={busy}>Cancel</button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Destructive Collection uninstall. Purges the deployment,
       drops the profile, and deletes staged mods — restoring the game to vanilla. -->
  {#if collUninstallTarget}
    <div class="overlay" role="dialog" aria-modal="true" aria-labelledby="coll-uninstall-title">
      <div class="modal">
        <h3 id="coll-uninstall-title">Uninstall "{collUninstallTarget.name}"?</h3>
        <p>
          This purges the Collection's deployment, removes its profile, and deletes its
          staged mods, restoring the game to its original vanilla state. This can't be
          undone.
        </p>
        <div class="actions">
          <button class="destructive" onclick={onConfirmUninstallCollection} disabled={busy}>
            Uninstall
          </button>
          <button onclick={onCancelUninstallCollection} disabled={busy}>Cancel</button>
        </div>
      </div>
    </div>
  {/if}

  <!-- FOMOD guided install wizard. Opens in the existing.overlay/.modal when
       an archive contains fomod/ModuleConfig.xml. One install step per screen; Back/Next
       (Install on the last step); a hard dry-run conflict-preview gate before any write. -->
  {#if fomodProj && fomodStep}
    <div class="overlay" role="dialog" aria-modal="true" aria-labelledby="fomod-title">
      <div class="modal fomod-modal">
        <h3 id="fomod-title">
          {fomodProj.module_name || "FOMOD installer"}
          <span class="muted">
            Step {fomodStepIdx + 1} of {fomodVisibleSteps.length} · {fomodStep.name}
          </span>
        </h3>

        {#if !fomodPreview}
          <!-- Step body: one install step, its option groups rendered by FOMOD type. -->
          <div class="fomod-body">
            {#each fomodStep.groups as group (group.name)}
              <fieldset class="fomod-group">
                <legend>{group.name}</legend>
                {#if group.group_type === "SelectAtLeastOne"}
                  {@const n = group.options.filter((o) =>
                    fomodChosen.has(fomodLogic.fomodKey(fomodStep.name, group.name, o.name)),
                  ).length}
                  {#if n < 1}
                    <p class="warn fomod-min">Select at least 1 option(s) to continue.</p>
                  {/if}
                {/if}
                <ul class="fomod-options">
                  {#each group.options as opt (opt.name)}
                    {@const key = fomodLogic.fomodKey(fomodStep.name, group.name, opt.name)}
                    {@const checked = fomodChosen.has(key)}
                    {@const isRadio =
                      group.group_type === "SelectExactlyOne" ||
                      group.group_type === "SelectAtMostOne"}
                    {@const locked =
                      opt.default_type === "Required" || group.group_type === "SelectAll"}
                    {@const notUsable = opt.default_type === "NotUsable"}
                    <li class="fomod-option" class:disabled={notUsable}>
                      <label
                        title={notUsable ? "Not available with your current choices." : undefined}
                      >
                        <input
                          type={isRadio ? "radio" : "checkbox"}
                          name={`${fomodStep.name}::${group.name}`}
                          checked={checked}
                          disabled={locked || notUsable || busy}
                          onchange={() =>
                            fomodToggle(
                              fomodStep.name,
                              group.name,
                              opt.name,
                              group.group_type,
                            )}
                        />
                        <span class="fomod-opt-label" class:selected={checked}>{opt.name}</span>
                        {#if opt.default_type !== "Optional"}
                          <span class="type-tag tag-{opt.default_type.toLowerCase()}">
                            {opt.default_type}
                          </span>
                        {/if}
                      </label>
                      {#if opt.image}
                        <img class="fomod-img" src={opt.image} alt={opt.name} />
                      {/if}
                      {#if opt.description}
                        <p class="fomod-desc" class:muted={!checked}>{opt.description}</p>
                      {/if}
                    </li>
                  {/each}
                </ul>
              </fieldset>
            {/each}
          </div>

          <div class="actions fomod-nav">
            <button onclick={fomodBack} disabled={fomodOnFirstStep || busy}>Back</button>
            {#if fomodIsLastStep}
              <button
                class="cta"
                onclick={fomodShowPreview}
                disabled={!fomodStepValid || busy}
              >
                Install
              </button>
            {:else}
              <button class="cta" onclick={fomodNext} disabled={!fomodStepValid || busy}>
                Next
              </button>
            {/if}
            <button onclick={closeFomodWizard} disabled={busy}>Cancel</button>
          </div>
        {:else}
          <!-- Dry-run preview HARD GATE: the resolved file plan BEFORE any staging
               write. An unresolvable selection errors out of resolve, so a preview
               shown here is always a plan that is safe to install. -->
          <div class="report fomod-preview">
            <h4>Install preview</h4>
            <p class="ok">No conflicts — safe to install.</p>
            <ul class="fomod-plan">
              {#each fomodPreview.plan as row (row.dest)}
                <li>
                  <code>{row.src}</code> → <code>{row.dest}</code>
                </li>
              {/each}
            </ul>
            {#if fomodPreview.plan.length === 0}
              <p class="muted">This selection installs no files.</p>
            {/if}
          </div>

          <div class="actions fomod-nav">
            <button onclick={() => (fomodPreview = null)} disabled={fomodApplying || busy}>
              Back
            </button>
            <button
              class="cta"
              onclick={fomodApply}
              disabled={fomodApplying || busy}
            >
              Install
            </button>
            <button onclick={closeFomodWizard} disabled={fomodApplying || busy}>Cancel</button>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</main>

<style>
  main {
    font-family: system-ui, sans-serif;
    max-width: 820px;
    margin: 0 auto;
    padding: 1.5rem;
    line-height: 1.4;
  }
  section {
    border: 1px solid #ccc;
    border-radius: 8px;
    padding: 1rem;
    margin-bottom: 1.25rem;
  }
  h1 { font-size: 1.5rem; }
  h2 { font-size: 1.15rem; margin-top: 0; }
  label { display: block; margin: 0.4rem 0; }
  input, select { padding: 0.3rem; min-width: 22rem; }
  select { min-width: 14rem; }
  button { padding: 0.35rem 0.8rem; margin: 0.2rem 0.3rem 0.2rem 0; cursor: pointer; }
  button:disabled { cursor: not-allowed; opacity: 0.6; }
  code { background: #f3f3f3; padding: 0 0.2rem; border-radius: 3px; word-break: break-all; }
  .paths { font-size: 0.85rem; margin: 0.2rem 0 0.6rem 1.4rem; }
  .actions { margin: 0.6rem 0; }
  .report { border-top: 1px solid #eee; margin-top: 0.8rem; padding-top: 0.6rem; }
  .muted { color: #777; }
  .busy { color: #555; font-style: italic; }
  .ok { color: #1a7f37; font-weight: 600; }
  .err { color: #cf222e; font-weight: 600; }
  .warn { color: #9a6700; background: #fff8e5; border: 1px solid #e6c200; border-radius: 6px; padding: 0.5rem 0.75rem; margin-top: 0.5rem; }
  /* Persistent advisory drift notice: amber, non-blocking. */
  .drift-notice { color: #9a6700; background: #fff8e5; border: 1px solid #e6c200; border-radius: 6px; padding: 0.5rem 0.75rem; margin: 0.5rem 0; }
  .drift-notice p { margin: 0.3rem 0 0; }
  /* First-launch gate: neutral info box with the Re-check action. */
  .first-launch { background: #f3f3f3; border-radius: 6px; padding: 0.75rem; }
  .first-launch p { margin: 0.3rem 0 0.6rem; }

  /* --- Account panel --- */
  .account-line { display: flex; align-items: center; gap: 0.5rem; margin: 0.25rem 0 0.75rem; }
  .account-line .dot { color: #1a7f37; }            /* Success green status dot */
  .account-line .username { font-weight: 600; }
  .account-line .tier {
    font-size: 0.875rem;
    background: #f3f3f3;
    border: 1px solid #ccc;
    border-radius: 3px;
    padding: 0 0.4rem;
  }
  /* The single Accent (10%) primary login CTA. */
  button.cta { background: #0a66c2; color: #fff; border: 1px solid #0a66c2; }
  button.cta:disabled { opacity: 0.6; }
  /* Neutral secondary "Use an API key instead" reveal — link-styled, not accent. */
  button.link-btn { background: none; border: none; color: #0a66c2; text-decoration: underline; padding: 0.35rem 0; }
  .apikey { margin-top: 0.5rem; }
  .confirm { border: 1px solid #ccc; border-radius: 6px; padding: 0.5rem 0.75rem; margin-top: 0.5rem; background: #f3f3f3; }
  /* Destructive no-keyring banner. */
  .keyring-banner {
    color: #cf222e;
    background: #fff;
    border: 1px solid #cf222e;
    border-radius: 6px;
    padding: 0.75rem 1rem;
  }
  .keyring-banner p { color: #333; margin: 0.4rem 0 0; }

  /* --- Downloads list --- */
  .downloads .free-hint { margin: 0 0 0.5rem; }
  .rate-notice {
    color: #9a6700;
    background: #fff8e5;
    border: 1px solid #e6c200;
    border-radius: 4px;
    padding: 0.5rem 0.75rem;
    margin: 0 0 0.75rem;
    font-size: 0.875rem;
  }
  /* §C.1 arrival toast — Success styling (green), non-blocking, auto-dismissing. */
  .nxm-toast {
    color: #1a7f37;
    background: #eaf6ed;
    border: 1px solid #1a7f37;
    border-radius: 4px;
    padding: 0.5rem 0.75rem;
    margin: 0 0 0.75rem;
    font-size: 0.875rem;
    font-weight: 600;
  }
  .downloads .empty {
    border: 1px solid #ccc;
    border-radius: 8px;
    padding: 1rem;
    background: #f3f3f3;
  }
  .downloads .empty p { margin: 0.4rem 0 0; }
  ul.download-list { list-style: none; padding: 0; margin: 0.4rem 0; }
  .download-row {
    display: grid;
    grid-template-columns: 1fr;
    gap: 0.25rem;
    min-height: 40px; /* download-row density */
    padding: 0.5rem 0.75rem;
    border: 1px solid #eee;
    border-radius: 4px;
    margin-bottom: 0.4rem;
  }
  .download-row.failed { border-color: #cf222e; }
  .dl-name {
    font-family: ui-monospace, monospace;
    background: #f3f3f3;
    border-radius: 3px;
    padding: 0 0.25rem;
    word-break: break-all;
  }
  .bar-track { height: 8px; background: #f3f3f3; border-radius: 4px; overflow: hidden; }
  .bar-fill { height: 100%; background: #0a66c2; transition: width 0.15s linear; }
  .bar-fill.indeterminate { animation: pulse 1s ease-in-out infinite; }
  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.5; } }
  .dl-meta { display: flex; align-items: center; gap: 0.5rem; font-size: 0.875rem; }
  .dl-meta .done { color: #1a7f37; }

  /* --- Conflict view --- */
  .pending p { margin: 0.25rem 0 0; }
  .conflict-toolbar { margin: 0.75rem 0; display: flex; gap: 0.5rem; align-items: center; }
  /* Accent (10%) reserved for the single primary Deploy CTA when changes are pending. */
  button.cta {
    background: #0a66c2;
    color: #fff;
    border: 1px solid #0a66c2;
    font-weight: 600;
  }
  button.cta:hover:not(:disabled) { background: #0857a6; }

  h3 .muted { font-weight: 400; font-size: 0.85rem; }

  ol.priority { list-style: none; padding: 0; margin: 0.4rem 0; }
  ol.priority li,
  ul.priority li {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-height: 32px; /* desktop reorder hit target */
    padding: 0.25rem 0.5rem;
    border: 1px solid #eee;
    border-radius: 6px;
    margin-bottom: 0.25rem;
  }
  ol.priority li.disabled { color: #777; }
  .reorder { display: inline-flex; flex-direction: column; line-height: 1; }
  .reorder button {
    padding: 0 0.4rem;
    margin: 0;
    min-height: 16px;
    font-size: 0.75rem;
  }
  .rank { color: #777; min-width: 1.5rem; }
  .mod-name { font-weight: 600; flex: 1; }
  .mod-state { font-size: 0.85rem; }
  .mod-state.on { color: #1a7f37; }
  .mod-state.off { color: #777; }

  .empty { background: #f3f3f3; border-radius: 6px; padding: 0.75rem; }
  .empty p { margin: 0.25rem 0 0; }

  table.conflicts { width: 100%; border-collapse: collapse; margin-top: 0.4rem; }
  table.conflicts th, table.conflicts td {
    text-align: left;
    padding: 4px 8px; /* dense table padding */
    border-bottom: 1px solid #eee;
    vertical-align: top;
  }
  table.conflicts thead th { background: #f3f3f3; font-weight: 600; }
  td.providers .loser { color: #777; }
  td.winner .dot { color: #0a66c2; }
  td.winner .winner-name { font-weight: 600; }

  /* --- Plugin manager --- */
  .mono { font-family: ui-monospace, monospace; }
  ol.priority.plugins li.group-divider {
    display: block;
    border: none;
    background: #f3f3f3;
    color: #555;
    font-size: 0.8rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    padding: 0.2rem 0.5rem;
    margin: 0.4rem 0 0.2rem;
    min-height: 0;
  }
  .plugin-toggle { margin: 0; display: inline-flex; }
  .badge {
    font-size: 0.7rem;
    font-weight: 600;
    padding: 0.05rem 0.35rem;
    border-radius: 3px;
    border: 1px solid #ccc;
    color: #555;
  }
  .badge-esm { background: #eef3fb; border-color: #b8cdec; color: #0a4a8a; }
  .badge-esl { background: #f0eefb; border-color: #c8bce8; color: #5a3a8a; }
  .badge-esp { background: #f3f3f3; }
  /* MEDIUM tier — teal-neutral, deliberately NOT the esm-blue (accent). */
  .badge-medium { background: #eaf3f0; border-color: #9cccb9; color: #1a6b52; }
  /* PROTECTED — neutral muted reuse of li.disabled tones (no new hue). */
  .badge-protected { background: #f3f3f3; color: #777; }
  /* Protected masters render as locked rows — muted, with a subtle neutral fill. */
  li.protected { background: #f3f3f3; color: #777; }

  .loot-proposal {
    border: 1px solid #e6c200;
    background: #fffdf5;
    border-radius: 6px;
    padding: 0.5rem 0.75rem;
    margin: 0.5rem 0;
  }
  ol.proposed { list-style: none; padding: 0; margin: 0.3rem 0; }
  ol.proposed li {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.1rem 0.4rem;
    border-radius: 4px;
  }
  ol.proposed li.moved { background: #fff8e5; }
  .moved-tag {
    font-size: 0.7rem;
    color: #9a6700;
    border: 1px solid #e6c200;
    border-radius: 3px;
    padding: 0 0.3rem;
  }
  pre.txt-preview {
    background: #f3f3f3;
    border-radius: 6px;
    padding: 0.5rem 0.75rem;
    font-size: 0.85rem;
    overflow-x: auto;
    white-space: pre;
    margin: 0.3rem 0;
  }

  /* --- Profiles --- */
  ul.priority.profiles { list-style: none; padding: 0; margin: 0.4rem 0; }
  ul.priority.profiles li.active { border-color: #0a66c2; background: #f5f9ff; }
  /* Accent indicator marks the active (deployed) profile (§D.1). */
  .active-dot { color: #0a66c2; min-width: 1rem; }
  .active-dot.placeholder { color: #ccc; }
  .prof-name { font-weight: 600; flex: 1; }
  .active-label {
    font-size: 0.7rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: #0a66c2;
    border: 1px solid #b8cdec;
    background: #eef3fb;
    border-radius: 3px;
    padding: 0 0.35rem;
  }
  .prof-select {
    flex: 1;
    text-align: left;
    background: none;
    border: none;
    font-weight: 600;
    color: #0a4a8a;
    text-decoration: underline;
    padding: 0;
    margin: 0;
  }
  .prof-actions { margin-left: auto; }
  .danger-link {
    background: none;
    border: none;
    color: #cf222e;
    padding: 0;
    margin: 0;
    font-size: 0.85rem;
    text-decoration: underline;
  }
  .prof-hint { font-size: 0.85rem; margin: 0.3rem 0 0; }

  /* Destructive (red) confirm button — reserved for delete (Copywriting rule). */
  button.destructive {
    background: #cf222e;
    color: #fff;
    border: 1px solid #cf222e;
    font-weight: 600;
  }
  button.destructive:hover:not(:disabled) { background: #a40e26; }

  /* Confirmation modal (every disk-mutating profile action is gated — §D.2). */
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 10;
  }
  .modal {
    background: #fff;
    border-radius: 8px;
    padding: 1.25rem 1.5rem;
    max-width: 28rem;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.25);
  }
  .modal h3 { margin-top: 0; }
  .modal .actions { margin-bottom: 0; }

  /* --- FOMOD wizard --- */
  .fomod-modal { max-width: 40rem; max-height: 80vh; overflow-y: auto; }
  .fomod-modal h3 .muted { display: block; font-size: 0.875rem; margin-top: 0.2rem; }
  .fomod-body { margin: 0.75rem 0; }
  .fomod-fallback .actions { margin-bottom: 0; }
  .fomod-group {
    border: 1px solid #ccc;
    border-radius: 8px;
    padding: 0.5rem 0.75rem;
    margin: 0 0 0.75rem;
  }
  .fomod-group legend { font-weight: 600; padding: 0 0.3rem; }
  .fomod-min { margin: 0.25rem 0; font-size: 0.875rem; }
  ul.fomod-options { list-style: none; padding: 0; margin: 0; }
  .fomod-option {
    min-height: 40px; /* §Spacing: option-row density */
    padding: 0.4rem 0.25rem;
    border-bottom: 1px solid #eee;
  }
  .fomod-option:last-child { border-bottom: none; }
  .fomod-option.disabled { color: #777; }
  .fomod-option label { display: flex; align-items: center; gap: 0.5rem; margin: 0; }
  .fomod-opt-label { font-weight: 400; }
  .fomod-opt-label.selected { font-weight: 600; }
  .fomod-img { max-height: 96px; display: block; margin: 0.3rem 0 0 1.6rem; border-radius: 3px; }
  .fomod-desc { margin: 0.2rem 0 0 1.6rem; font-size: 0.95rem; }
  /* Type-state tags (§A.4). Required = neutral; Recommended/CouldBeUsable = amber;
     NotUsable = muted. */
  .type-tag {
    font-size: 0.75rem;
    font-weight: 600;
    padding: 0.05rem 0.4rem;
    border-radius: 3px;
    border: 1px solid #ccc;
    color: #555;
  }
  .type-tag.tag-required { background: #f3f3f3; border-color: #ccc; color: #555; }
  .type-tag.tag-recommended,
  .type-tag.tag-couldbeusable {
    background: #fff8e5;
    border-color: #e6c200;
    color: #9a6700;
  }
  .type-tag.tag-notusable { background: #f3f3f3; border-color: #ccc; color: #999; }
  .fomod-nav { margin-top: 0.75rem; }
  .fomod-preview { margin: 0.5rem 0; }
  ul.fomod-plan {
    list-style: none;
    padding: 0;
    margin: 0.4rem 0;
    max-height: 16rem;
    overflow-y: auto;
  }
  ul.fomod-plan li { padding: 0.15rem 0; word-break: break-all; }

  /* Collections. Reuses the report/warn/bar/tag visual language. */
  .coll-pick .coll-rev { max-width: 6rem; }
  .coll-manifest-label { display: block; margin: 0.5rem 0; }
  .coll-manifest-label textarea {
    display: block;
    width: 100%;
    margin-top: 0.25rem;
    font-family: ui-monospace, monospace;
    font-size: 0.8rem;
  }
  .coll-summary { font-weight: 600; }
  ul.coll-mod-list { list-style: none; padding: 0; margin: 0.4rem 0; }
  .coll-mod-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.2rem 0;
    border-bottom: 1px solid #f0f0f0;
  }
  .coll-mod-name { font-weight: 500; }
  .coll-mod-ver { color: #777; font-size: 0.8rem; }
  .coll-tag {
    margin-left: auto;
    font-size: 0.75rem;
    font-weight: 600;
    padding: 0.05rem 0.4rem;
    border-radius: 3px;
    border: 1px solid #ccc;
  }
  .coll-tag.tag-ok { background: #e6f4ea; border-color: #1a7f37; color: #1a7f37; }
  .coll-tag.tag-warn { background: #fff8e5; border-color: #e6c200; color: #9a6700; }
  .coll-tag.tag-err { background: #fbe9e7; border-color: #cf222e; color: #cf222e; }
  .coll-skip-note { margin-top: 0.4rem; }
  .coll-overall { margin: 0.3rem 0 0.6rem; }
  .coll-actions { display: flex; align-items: center; gap: 0.75rem; margin-top: 0.5rem; }
  .coll-manual ul, .coll-installed .warn ul { margin: 0.3rem 0 0; padding-left: 1.1rem; }
  .coll-premium p { margin: 0.3rem 0 0; }
</style>
