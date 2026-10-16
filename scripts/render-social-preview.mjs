// Render docs/assets/social-preview.html to docs/assets/social-preview.png at
// 1280x640. Needs Playwright with a Chromium build:
//   npx --yes playwright@1.56.1 install chromium   (once, if you have none)
//   node scripts/render-social-preview.mjs
// Playwright is not a project dependency on purpose: the image changes rarely
// and the app's dependency tree stays small.
import { pathToFileURL } from "node:url";
import { resolve } from "node:path";

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE ?? "playwright");
const html = resolve("docs/assets/social-preview.html");
const out = resolve("docs/assets/social-preview.png");

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 640 } });
await page.goto(pathToFileURL(html).href);
await page.waitForLoadState("load");
await page.screenshot({ path: out, clip: { x: 0, y: 0, width: 1280, height: 640 } });
await browser.close();
console.log(`wrote ${out}`);
