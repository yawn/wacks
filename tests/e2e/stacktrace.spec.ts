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

let rawFrames: Frame[];
let capturedFrames: Frame[];

test.beforeEach(async ({ page }) => {
  await page.goto("http://localhost:3333");
  await page.waitForFunction(() => (window as any).__wasm_ready === true);
  await page.evaluate(() => (window as any).triggerPanic());
  rawFrames = await page.evaluate(() => (window as any).__raw_frames);
  capturedFrames = await page.evaluate(() => (window as any).__captured_frames);
});

// ---------------------------------------------------------------------------
// Raw Frame::parse() — native browser behavior, no name section backfill
// ---------------------------------------------------------------------------

test.describe("raw (Frame::parse)", () => {
  test("captures wasm frames", async ({ browserName }) => {
    const wasmFrames = rawFrames.filter((f) => f.wasm_function_index != null);
    expect(wasmFrames.length).toBeGreaterThanOrEqual(3);

    // WebKit nondeterministically drops WASM names from Error.stack
    if (browserName === "webkit") return;

    const names = wasmFrames
      .map((f) => f.function)
      .filter(Boolean) as string[];
    expect(names.some((n) => n.includes("trigger_panic"))).toBe(true);
    expect(names.some((n) => n.includes("panic"))).toBe(true);
  });

  test("chrome and firefox provide byte offsets", async ({ browserName }) => {
    test.skip(browserName === "webkit", "WebKit omits wasm byte offsets");

    const wasmFrames = rawFrames.filter(
      (f) => f.wasm_function_index != null
    );
    expect(wasmFrames.every((f) => f.wasm_byte_offset != null)).toBe(true);
  });

  test("webkit provides function index without byte offset", async ({
    browserName,
  }) => {
    test.skip(browserName !== "webkit", "WebKit-specific");

    const wasmFrames = rawFrames.filter(
      (f) => f.wasm_function_index != null
    );
    expect(wasmFrames.length).toBeGreaterThan(0);
    expect(wasmFrames.every((f) => f.wasm_byte_offset == null)).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// capture() — parse + name section backfill, should work on all browsers
// ---------------------------------------------------------------------------

test.describe("capture()", () => {
  test("captures named wasm frames", async () => {
    const wasmFrames = capturedFrames.filter(
      (f) => f.wasm_function_index != null
    );
    expect(wasmFrames.length).toBeGreaterThanOrEqual(3);

    const names = wasmFrames
      .map((f) => f.function)
      .filter(Boolean) as string[];
    expect(names.some((n) => n.includes("trigger_panic"))).toBe(true);
    expect(names.some((n) => n.includes("panic"))).toBe(true);
  });

  test("demangled names strip hash suffix", async () => {
    const wasmNamed = capturedFrames.filter(
      (f) => f.wasm_function_index != null && f.function
    );

    for (const f of wasmNamed) {
      expect(f.function).not.toMatch(/::h[0-9a-f]{5,}$/);
    }

    const withHash = wasmNamed.filter(
      (f) => f.raw_function && f.raw_function !== f.function
    );
    expect(withHash.length).toBeGreaterThan(0);
    expect(
      withHash.some((f) => /::h[0-9a-f]{5,}$/.test(f.raw_function!))
    ).toBe(true);
  });

  test("in_app classifies std/core as infrastructure", async () => {
    const infraFrames = capturedFrames.filter(
      (f) => !f.in_app && f.wasm_function_index != null
    );
    expect(infraFrames.length).toBeGreaterThan(0);

    for (const f of infraFrames) {
      expect(f.function).toMatch(/^(std::|core::|alloc::|__rustc)/);
    }

    const tp = capturedFrames.find(
      (f) => f.function === "trigger_panic" && f.wasm_function_index != null
    );
    expect(tp?.in_app).toBe(true);
  });

  test("module prefix is stripped", async () => {
    const wasmNamed = capturedFrames.filter(
      (f) => f.wasm_function_index != null && f.function
    );

    for (const f of wasmNamed) {
      expect(f.function).not.toContain(".wasm.");
      expect(f.raw_function).not.toContain(".wasm.");
    }
  });
});
