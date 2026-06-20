// SPA mode: Tauri serves a single static bundle with no Node server, so disable SSR
// and prerendering. All data comes from Tauri `invoke` calls at runtime.
export const ssr = false;
export const prerender = false;
