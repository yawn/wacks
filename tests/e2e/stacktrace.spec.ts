import { test, expect } from "@playwright/test";

interface Frame {
  function: string | null;
  raw_function: string | null;
  filename: string | null;
  lineno: number | null;
  colno: number | null;
  wasm_function_index: number | null;
  wasm_byte_offset: number | null;
  in_app: boolean;
}

let frames: Frame[];

test.beforeEach(async ({ page }) => {
  await page.goto("http://localhost:3333");
  await page.waitForFunction(() => (window as any).__wasm_ready === true);
  await page.evaluate(() => (window as any).triggerPanic());
  frames = await page.evaluate(() => (window as any).__captured_frames);
});

test("captures wasm frames", async ({ browserName }) => {
  const wasmFrames = frames.filter((f) => f.wasm_function_index != null);
  expect(wasmFrames.length).toBeGreaterThanOrEqual(3);

  // WebKit nondeterministically resolves WASM function names from the name
  // section — the same binary can produce named or numeric-only frames
  // across browser contexts (~50% of runs).
  if (browserName === "webkit") return;

  const names = wasmFrames.map((f) => f.function).filter(Boolean) as string[];
  expect(names.some((n) => n.includes("trigger_panic"))).toBe(true);
  expect(names.some((n) => n.includes("panic"))).toBe(true);
});

test("demangled names strip hash suffix", async ({ browserName }) => {
  test.skip(browserName === "webkit", "WebKit nondeterministically omits WASM names");

  const wasmNamed = frames.filter(
    (f) => f.wasm_function_index != null && f.function
  );

  for (const f of wasmNamed) {
    expect(f.function).not.toMatch(/::h[0-9a-f]{5,}$/);
  }

  // Raw names should retain the hash for non-extern functions
  const withHash = wasmNamed.filter(
    (f) => f.raw_function && f.raw_function !== f.function
  );
  expect(withHash.length).toBeGreaterThan(0);
  expect(
    withHash.some((f) => /::h[0-9a-f]{5,}$/.test(f.raw_function!))
  ).toBe(true);
});

test("in_app classifies std/core as infrastructure", async ({
  browserName,
}) => {
  test.skip(browserName === "webkit", "WebKit nondeterministically omits WASM names");

  const infraFrames = frames.filter(
    (f) => !f.in_app && f.wasm_function_index != null
  );
  expect(infraFrames.length).toBeGreaterThan(0);

  for (const f of infraFrames) {
    expect(f.function).toMatch(/^(std::|core::|alloc::|__rustc)/);
  }

  // trigger_panic is user code
  const tp = frames.find(
    (f) => f.function === "trigger_panic" && f.wasm_function_index != null
  );
  expect(tp?.in_app).toBe(true);
});

test("module prefix is stripped", async ({ browserName }) => {
  test.skip(browserName === "webkit", "WebKit nondeterministically omits WASM names");

  const wasmNamed = frames.filter(
    (f) => f.wasm_function_index != null && f.function
  );

  for (const f of wasmNamed) {
    expect(f.function).not.toContain(".wasm.");
    expect(f.raw_function).not.toContain(".wasm.");
  }
});

test("chrome and firefox provide byte offsets", async ({ browserName }) => {
  test.skip(browserName === "webkit", "WebKit omits wasm byte offsets");

  const wasmFrames = frames.filter((f) => f.wasm_function_index != null);
  expect(wasmFrames.every((f) => f.wasm_byte_offset != null)).toBe(true);
});

test("webkit provides function index without byte offset", async ({
  browserName,
}) => {
  test.skip(browserName !== "webkit", "WebKit-specific");

  const wasmFrames = frames.filter((f) => f.wasm_function_index != null);
  expect(wasmFrames.length).toBeGreaterThan(0);
  expect(wasmFrames.every((f) => f.wasm_byte_offset == null)).toBe(true);
});
