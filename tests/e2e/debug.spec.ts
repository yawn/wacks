import { test } from "@playwright/test";

test("dump captured frames", async ({ page, browserName }) => {
  await page.goto("http://localhost:3333");
  await page.waitForFunction(() => (window as any).__wasm_ready === true);
  await page.evaluate(() => (window as any).triggerPanic());
  const frames = await page.evaluate(() => (window as any).__captured_frames);
  console.log(`\n=== ${browserName} (${frames.length} frames) ===`);
  for (const f of frames) {
    console.log(JSON.stringify(f));
  }
});
