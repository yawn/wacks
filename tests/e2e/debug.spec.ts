import { test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("http://localhost:3333");
  await page.waitForFunction(() => (window as any).__wasm_ready === true);
  await page.evaluate(() => (window as any).triggerPanic());
});

for (const kind of ["raw", "captured"] as const) {
  const key = kind === "raw" ? "__raw_frames" : "__captured_frames";

  test(`dump ${kind} frames`, async ({ page, browserName }) => {
    const frames = await page.evaluate((k) => (window as any)[k], key);
    console.log(`\n=== ${browserName} ${kind} (${frames.length} frames) ===`);
    for (const f of frames) console.log(JSON.stringify(f));
  });
}
