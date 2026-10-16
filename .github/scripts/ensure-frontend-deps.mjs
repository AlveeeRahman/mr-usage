// Run `npm ci` only when node_modules is missing or older than package-lock.json.
//
// Tauri's beforeDevCommand and beforeBuildCommand call this, so a fresh clone
// works with `cargo tauri dev` and no manual install. An unconditional `npm ci`
// would fail on Windows whenever the Tauri CLI itself runs from node_modules
// (`npm run tauri ...`, tauri-action): npm cannot delete the CLI's native
// module while that process has it open.
import { existsSync, statSync } from "node:fs";
import { spawnSync } from "node:child_process";

const marker = "node_modules/.package-lock.json";
const stale = !existsSync(marker) || statSync(marker).mtimeMs < statSync("package-lock.json").mtimeMs;

if (stale) {
  const result = spawnSync("npm ci", { stdio: "inherit", shell: true });
  process.exit(result.status ?? 1);
}
