import { test, expect } from "@playwright/test";

let rawFrames: any[];
let capturedFrames: any[];

test.beforeEach(async ({ page }) => {
  await page.goto("http://localhost:3333");
  await page.waitForFunction(() => (window as any).__wasm_ready === true);
  await page.evaluate(() => (window as any).triggerPanic());
  rawFrames = await page.evaluate(() => (window as any).__raw_frames);
  capturedFrames = await page.evaluate(() => (window as any).__captured_frames);
});

const wasmOf = (frames: any[]) =>
  frames.filter((f) => f.wasm_function_index != null);

const namedWasmOf = (frames: any[]) =>
  wasmOf(frames).filter((f) => f.function);

test.describe("raw (Frame::parse)", () => {
  test("captures wasm frames", async ({ browserName }) => {
    expect(wasmOf(rawFrames).length).toBeGreaterThanOrEqual(3);

    // WebKit nondeterministically drops WASM names from Error.stack
    if (browserName === "webkit") return;

    const names = namedWasmOf(rawFrames).map((f) => f.function);
    expect(names.some((n) => n.includes("trigger_panic"))).toBe(true);
    expect(names.some((n) => n.includes("panic"))).toBe(true);
  });

  test("chrome and firefox provide byte offsets", async ({ browserName }) => {
    test.skip(browserName === "webkit", "WebKit omits wasm byte offsets");
    expect(wasmOf(rawFrames).every((f) => f.wasm_byte_offset != null)).toBe(
      true
    );
  });

  test("webkit provides function index without byte offset", async ({
    browserName,
  }) => {
    test.skip(browserName !== "webkit", "WebKit-specific");
    const wasm = wasmOf(rawFrames);
    expect(wasm.length).toBeGreaterThan(0);
    expect(wasm.every((f) => f.wasm_byte_offset == null)).toBe(true);
  });
});

test.describe("capture()", () => {
  test("captures named wasm frames", async () => {
    expect(wasmOf(capturedFrames).length).toBeGreaterThanOrEqual(3);

    const names = namedWasmOf(capturedFrames).map((f) => f.function);
    expect(names.some((n) => n.includes("trigger_panic"))).toBe(true);
    expect(names.some((n) => n.includes("panic"))).toBe(true);
  });

  test("demangled names strip hash suffix", async () => {
    const wasm = namedWasmOf(capturedFrames);

    for (const f of wasm) {
      expect(f.function).not.toMatch(/::h[0-9a-f]{5,}$/);
    }

    const withHash = wasm.filter((f) => f.raw_function && f.raw_function !== f.function);
    expect(withHash.length).toBeGreaterThan(0);
    expect(
      withHash.some((f) => /::h[0-9a-f]{5,}$/.test(f.raw_function))
    ).toBe(true);
  });

  test("in_app classifies std/core as infrastructure", async () => {
    const infra = capturedFrames.filter(
      (f) => !f.in_app && f.wasm_function_index != null
    );
    expect(infra.length).toBeGreaterThan(0);

    for (const f of infra) {
      expect(f.function).toMatch(/^(std::|core::|alloc::|__rustc)/);
    }

    const tp = capturedFrames.find(
      (f) => f.function === "trigger_panic" && f.wasm_function_index != null
    );
    expect(tp?.in_app).toBe(true);
  });

  test("module prefix is stripped", async () => {
    for (const f of namedWasmOf(capturedFrames)) {
      expect(f.function).not.toContain(".wasm.");
      expect(f.raw_function).not.toContain(".wasm.");
    }
  });
});
