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

  let busy = $state(false);
  let error = $state<string | null>(null);
  let status = $state<string | null>(null);

  const selectedGame = $derived(managed.find((g) => g.appid === selectedAppid) ?? null);

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

  const warningLabel = (w: string) =>
    w === "CrossDevice"
      ? "Cross-device staging (EXDEV): hardlink/reflink unavailable — using symlink/copy. Stage on the same filesystem for best safety."
      : "Filesystem case-folding not confirmed: mod path casing is normalized for Wine instead.";

  // Load any already-managed games on mount.
  refreshManaged();
</script>

<main>
  <h1>NexTwist — Walking Skeleton</h1>

  {#if busy}<p class="busy">Working…</p>{/if}
  {#if status}<p class="ok">{status}</p>{/if}
  {#if error}<p class="err">{error}</p>{/if}

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
</style>
