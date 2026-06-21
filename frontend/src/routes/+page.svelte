<script lang="ts">
  // Functional-minimal Svelte 5 UI driving the full detect -> install -> deploy ->
  // purge round-trip. Visual polish is deferred (CONTEXT.md); every action goes
  // through lib/api.ts and the UI holds NO business logic / path resolution.
  import * as api from "$lib/api";
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
    Profile,
    SwitchReport,
    UserInfo,
    DownloadItem,
    DownloadProgress,
  } from "$lib/api";

  // Supported Bethesda AppIDs (display only; the backend enforces the allow-list).
  const SUPPORTED = [
    { appid: 489830, name: "Skyrim Special Edition" },
    { appid: 377160, name: "Fallout 4" },
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

  // Conflict view state (UI-SPEC §A). `mods` is the priority list (rank-ascending);
  // `conflicts` is the file-level conflict table. `deployedSig` captures the winner/
  // priority signature that was last deployed, so we can show the pending-vs-deployed
  // banner (D-04) when the current set differs.
  let mods = $state<ManagedMod[]>([]);
  let conflicts = $state<FileConflict[]>([]);
  let deployedSig = $state<string | null>(null);

  // Plugin manager state (UI-SPEC §B/§C). `plugins` is the editable, ordered list (the
  // backend returns it merged scan + per-profile state). `sortProposal` holds a LOOT
  // proposal awaiting review; it is applied into `plugins` only on explicit confirm (D-12).
  let plugins = $state<PluginInfo[]>([]);
  let sortProposal = $state<SortProposal | null>(null);
  let mastersFirstError = $state<string | null>(null);

  // Profile state (UI-SPEC §D). `profiles` is the per-game selector source; the active
  // profile is marked with the Accent indicator. Switching and deleting are disk-mutating
  // and confirmation-gated (D-15): selecting a target opens a modal, and ONLY on confirm
  // does the safe engine run (purge old → deploy new → write plugins.txt → mark active).
  let profiles = $state<Profile[]>([]);
  let newProfileName = $state("");
  let switchTarget = $state<Profile | null>(null); // pending confirm-to-switch
  let deleteTarget = $state<Profile | null>(null); // pending confirm-to-delete
  let switchReport = $state<SwitchReport | null>(null);

  // Account panel state (UI-SPEC §A). `userInfo` drives logged-in vs logged-out; the
  // refresh token / API key NEVER reaches the UI (only UserInfo does). `noKeyring` is
  // set when a login attempt hits the NEXUS-02 no-Secret-Service hard-fail — it blocks
  // login behind the destructive banner. `apiKeyReveal` toggles the key-paste fallback.
  let userInfo = $state<UserInfo | null>(null);
  let noKeyring = $state(false);
  let apiKeyReveal = $state(false);
  let apiKeyInput = $state("");
  let confirmLogout = $state(false);
  const loggedIn = $derived(userInfo !== null);

  // Downloads list state (UI-SPEC §B). `downloads` is the per-item list, driven entirely
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

  let busy = $state(false);
  let error = $state<string | null>(null);
  let status = $state<string | null>(null);

  const selectedGame = $derived(managed.find((g) => g.appid === selectedAppid) ?? null);

  // Enabled mods in priority order — the input the resolver folds over.
  const enabledMods = $derived(mods.filter((m) => m.enabled));

  // The current winner/priority signature: enabled mod ids in rank order. When this
  // differs from what was last deployed, priority changes are PENDING on disk (D-04).
  const currentSig = $derived(enabledMods.map((m) => m.id).join(","));
  const pending = $derived(deployedSig !== null && deployedSig !== currentSig);
  // Before any deploy this session we cannot prove the on-disk state, so treat an
  // un-deployed selection with conflicts as pending too (so Deploy is offered).
  const showPending = $derived(deployedSig === null ? mods.length > 0 : pending);

  const modName = (id: number): string =>
    mods.find((m) => m.id === id)?.name ?? `mod #${id}`;

  async function run<T>(label: string, fn: () => Promise<T>): Promise<T | undefined> {
    busy = true;
    error = null;
    status = null;
    try {
      const result = await fn();
      status = `${label} ok`;
      return result;
    } catch (e) {
      error = `${label} failed: ${String(e)}`;
      return undefined;
    } finally {
      busy = false;
    }
  }

  async function refreshManaged() {
    const games = await run("List games", api.listGames);
    if (games) managed = games;
  }

  // --- NexusMods account (UI-SPEC §A) ---

  // The backend surfaces the no-keyring hard-fail (NEXUS-02) as a distinct error string
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
        noKeyring = true; // block login behind the destructive banner (NEXUS-02)
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

  // --- NexusMods downloads (UI-SPEC §B). Everything is event-driven. ---

  /** Apply a `download://progress` event to the matching row (UI-SPEC §B states). */
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
      // WR-01/WR-02 (NEXUS-05): a transient, auto-recoverable rate-limit pause. Show the
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
      // A healthy tick means the backoff is over — clear the WR-01 notice.
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
   * An nxm:// link arrived (NXM-01): the shell already started the download server-side and
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
          // BUG 2 fix: store the non-secret coordinates the arrival carries so a Retry of
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

  /** An expired/invalid nxm:// link (UI-SPEC §C.3): show the Warning, never a Failed row. */
  function applyNxmExpired(x: api.NxmExpired) {
    expiredLink = x.reason;
  }

  /** Human percent for a row, or null when the total is unknown. */
  function pct(d: DownloadItem): number | null {
    return d.total && d.total > 0 ? Math.floor((d.downloaded / d.total) * 100) : null;
  }

  function fmtBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
    return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
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
    const result = await run("Install mod", () =>
      api.installArchive(selectedAppid!, archivePath),
    );
    if (result) staged = result;
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
  }

  // --- Conflict view (UI-SPEC §A) ---

  async function loadConflicts() {
    if (selectedAppid === null) return;
    const ms = await run("List mods", () => api.listMods(selectedAppid!));
    if (ms) mods = ms;
    const cs = await run("List conflicts", () => api.listConflicts(selectedAppid!));
    if (cs) conflicts = cs;
  }

  // Reorder by swapping ranks with the neighbor (▲▼). Keyboard/click reorder is the
  // baseline path (no DnD-only) per UI-SPEC §A.1. Pending until Deploy (D-04).
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
      // Record the deployed signature so the pending banner clears (D-04).
      deployedSig = currentSig;
    }
  }

  // --- Plugin manager (UI-SPEC §B/§C) ---

  // A plugin is in the "masters" group if it is a master (.esm) or ESL-flagged (.esl).
  const isMaster = (p: PluginInfo) => p.kind === "esm" || p.kind === "esl";

  // The badge text for a plugin's kind.
  const kindBadge = (k: PluginInfo["kind"]) => k.toUpperCase();

  // True if swapping `plugins[i]` with its neighbor in `dir` would put a regular plugin
  // before a master (or vice-versa) — a masters-first violation we must PREVENT (§B.2).
  function violatesMastersFirst(i: number, dir: -1 | 1): boolean {
    const other = i + dir;
    if (other < 0 || other >= plugins.length) return true; // out of range: disabled anyway
    // A move is only allowed within the same group (both masters or both regular).
    return isMaster(plugins[i]) !== isMaster(plugins[other]);
  }

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
    const other = i + dir;
    if (other < 0 || other >= plugins.length) return;
    if (violatesMastersFirst(i, dir)) {
      mastersFirstError = "Masters must load before regular plugins.";
      return;
    }
    mastersFirstError = null;
    const next = [...plugins];
    [next[i], next[other]] = [next[other], next[i]];
    plugins = next.map((p, idx) => ({ ...p, order: idx }));
  }

  async function onPluginToggle(name: string, enabled: boolean) {
    if (selectedAppid === null) return;
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

  // Apply a LOOT proposal into the editable list (D-12: only on explicit confirm). The
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

  // --- Profiles (UI-SPEC §D) ---

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
  // (UI-SPEC §D.2 / D-15). The actual switch runs only on confirm.
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
  // files are KEPT (D-14, shared staging). Idempotent at the store.
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
    if (appid !== null) {
      loadConflicts();
      loadPlugins();
      loadProfiles();
    }
  });

  // Load any already-managed games + the current account on mount.
  refreshManaged();
  loadAccount();

  // Subscribe to download progress events (UI-SPEC §B): the list is updated entirely off
  // these async events so the UI never freezes during a multi-GB download.
  api.onDownloadProgress(applyProgress);
  // Subscribe to nxm:// deep-link events (UI-SPEC §C): the arrival toast + the
  // expired/invalid-link Warning. The new download row arrives via the progress stream.
  api.onNxmArrival(applyNxmArrival);
  api.onNxmExpired(applyNxmExpired);
</script>

<main>
  <h1>NexTwist — Walking Skeleton</h1>

  {#if busy}<p class="busy">Working…</p>{/if}
  {#if status}<p class="ok">{status}</p>{/if}
  {#if error}<p class="err">{error}</p>{/if}

  <!-- Account panel (UI-SPEC §A): logged-out / logged-in / no-keyring. A token or key
       is never rendered. -->
  <section class="account">
    <h2>Account</h2>

    {#if noKeyring}
      <!-- NEXUS-02 hard-fail: no Secret Service backend. Login is blocked; NexTwist
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

  <!-- Downloads list (UI-SPEC §B): per-item progress, five row states, rate-limit notice,
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
                <!-- WR-01/WR-02: a transient rate-limit pause, not a failure. -->
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
    <section>
      <h2>3. Install &amp; deploy — {selectedGame.name}</h2>

      <label>
        Mod archive (.zip / .7z / .rar):
        <input bind:value={archivePath} placeholder="/path/to/mod.zip" />
      </label>
      <button onclick={onInstall} disabled={busy}>Install mod from archive</button>

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
            <p class="ok">Pristine — no drift detected.</p>
          {:else}
            <div class="warn">
              <strong>Drift detected:</strong>
              <ul>
                <li>missing: {verifyReport.missing.length}</li>
                <li>changed: {verifyReport.changed.length}</li>
                <li>orphans: {verifyReport.orphans.length}</li>
              </ul>
            </div>
          {/if}
        </div>
      {/if}
    </section>

    <section>
      <h2>4. Conflicts &amp; priority — {selectedGame.name}</h2>

      <!-- Pending-vs-deployed banner (UI-SPEC §A.4 / D-04) -->
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

      <!-- Mod priority list (UI-SPEC §A.1): top = highest priority = wins -->
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

      <!-- File-level conflict table (UI-SPEC §A.2) -->
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

      <div class="conflict-toolbar">
        <button onclick={loadPlugins} disabled={busy}>Refresh</button>
        <button onclick={onSortWithLoot} disabled={busy || plugins.length === 0}>
          Sort with LOOT
        </button>
        <button class="cta" onclick={onSavePluginOrder} disabled={busy || plugins.length === 0}>
          Save plugin order
        </button>
      </div>

      <!-- LOOT proposal review (UI-SPEC §C): no silent apply (D-12) -->
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

      <!-- Plugin list, masters-first grouped (UI-SPEC §B.1/§B.2) -->
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
            <li class:disabled={!p.enabled}>
              <span class="reorder">
                <button
                  onclick={() => onPluginReorder(i, -1)}
                  disabled={busy || i === 0 || violatesMastersFirst(i, -1)}
                  aria-label="Move {p.name} up"
                  title="Move up">▲</button>
                <button
                  onclick={() => onPluginReorder(i, 1)}
                  disabled={busy || i === plugins.length - 1 || violatesMastersFirst(i, 1)}
                  aria-label="Move {p.name} down"
                  title="Move down">▼</button>
              </span>
              <label class="plugin-toggle">
                <input
                  type="checkbox"
                  checked={p.enabled}
                  disabled={busy}
                  onchange={(e) => onPluginToggle(p.name, e.currentTarget.checked)} />
              </label>
              <span class="mono mod-name">{p.name}</span>
              <span class="badge badge-{p.kind}">{kindBadge(p.kind)}</span>
            </li>
          {/each}
        </ol>

        <!-- plugins.txt preview (UI-SPEC §B.3): read-only asterisk output -->
        <h3>plugins.txt preview <span class="muted">(asterisk = enabled)</span></h3>
        <pre class="txt-preview">{plugins
            .filter((p) => p.kind !== "esm")
            .map((p) => (p.enabled ? "*" : "") + p.name)
            .join("\n")}</pre>
      {/if}
    </section>

    <!-- Profiles (UI-SPEC §D): selector + confirmation-gated switch/delete (D-15) -->
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

  <!-- Confirmation-gated profile switch (UI-SPEC §D.2 / Copywriting). Disk is mutated
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
       mod files are kept (D-14). -->
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

  /* --- Account panel (UI-SPEC §A) --- */
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
  /* NEXUS-02 destructive no-keyring banner. */
  .keyring-banner {
    color: #cf222e;
    background: #fff;
    border: 1px solid #cf222e;
    border-radius: 6px;
    padding: 0.75rem 1rem;
  }
  .keyring-banner p { color: #333; margin: 0.4rem 0 0; }

  /* --- Downloads list (UI-SPEC §B) --- */
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
    min-height: 40px; /* UI-SPEC download-row density */
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

  /* --- Conflict view (UI-SPEC §A) --- */
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
    min-height: 32px; /* desktop reorder hit target (UI-SPEC Spacing) */
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
    padding: 4px 8px; /* dense table padding (UI-SPEC Spacing exception) */
    border-bottom: 1px solid #eee;
    vertical-align: top;
  }
  table.conflicts thead th { background: #f3f3f3; font-weight: 600; }
  td.providers .loser { color: #777; }
  td.winner .dot { color: #0a66c2; }
  td.winner .winner-name { font-weight: 600; }

  /* --- Plugin manager (UI-SPEC §B/§C) --- */
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

  /* --- Profiles (UI-SPEC §D) --- */
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
</style>
