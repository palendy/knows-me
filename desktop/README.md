# knows-me desktop shell (Tauri 2)

Thin GUI layer for the whole app. It owns the app window, manages one `AppState`
plus a `Services` registry for the unlocked session, and exposes the
`knows-me-core` command layer to the React frontend (`../src/`) as
`#[tauri::command]` handlers. All real logic (crypto, masking, storage,
ingestion, knowledge, persona) lives in the verified `knows-me-core` library —
this crate only assembles services and bridges IPC.

## Why a separate crate?

`knows-me-core` is a pure library with **no** Tauri dependency, so it builds and
runs its full unit + property test suite on any toolchain (and in CI sandboxes
without a webview). Tauri's build needs the platform webview toolchain, so it is
isolated here.

## Prerequisites

- A C toolchain + Tauri system deps (webview2 on Windows, `webkit2gtk` +
  `libsoup` on Linux, Xcode CLT on macOS). See <https://tauri.app/start/prerequisites/>.
- Node.js (for the frontend) and the Tauri CLI: `npm install` at the repo root.
- App icons (not committed): generate once with `npx tauri icon <a 1024px png>`,
  which writes `desktop/icons/*` referenced by `tauri.conf.json`.

## Run / build

From the **repo root**:

```bash
npm install
npx tauri dev      # launches Vite + the desktop window
npx tauri build    # production bundle
```

`tauri.conf.json` points `frontendDist` at `../dist` and `devUrl` at the Vite dev
server on port 1420.

## Commands exposed

Mirrored by the typed frontend bridges in `../src/shared/ipc.ts` and the
`../src/features/**` API adapters.

- **U1 — security & session**: `get_status`, `setup_password`, `unlock`, `lock`,
  `get_config`, `set_transfer_policy`, `set_server_enabled`, `list_transfers`,
  `local_api_status`
- **U3 — interview queue**: `queue_list`, `queue_answer`
- **U4 — read views & persona**: `get_dashboard`, `get_minihome`, `get_graph`,
  `persona_chat`, `persona_draft`

The U3/U4 commands go through `Services`, which is assembled on `setup_password`
/`unlock` (store readable) and torn down on `lock`; calling them while locked
returns `AppError::Locked`.
