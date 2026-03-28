import { test } from "@playwright/test";

test("dump raw frames", async ({ page, browserName }) => {
  await page.goto("http://localhost:3333");
  await page.waitForFunction(() => (window as any).__wasm_ready === true);
  await page.evaluate(() => (window as any).triggerPanic());
  const frames = await page.evaluate(() => (window as any).__raw_frames);
  console.log(`\n=== ${browserName} raw (${frames.length} frames) ===`);
  for (const f of frames) {
    console.log(JSON.stringify(f));
  }
});

test("dump captured frames", async ({ page, browserName }) => {
  await page.goto("http://localhost:3333");
  await page.waitForFunction(() => (window as any).__wasm_ready === true);
  await page.evaluate(() => (window as any).triggerPanic());
  const frames = await page.evaluate(() => (window as any).__captured_frames);
  console.log(`\n=== ${browserName} captured (${frames.length} frames) ===`);
  for (const f of frames) {
    console.log(JSON.stringify(f));
  }
});
