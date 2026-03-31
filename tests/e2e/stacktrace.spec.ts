import { readFileSync } from "fs";
import { join } from "path";
import { test, expect } from "@playwright/test";

const wasmOf = (frames: any[]) =>
  frames.filter((f) => f.wasm_function_index != null);

const namedWasmOf = (frames: any[]) =>
  wasmOf(frames).filter((f) => f.function);

test.describe("capture()", () => {
  let capturedFrames: any[];

  test.beforeEach(async ({ page }) => {
    await page.goto("http://localhost:3333");
    await page.waitForFunction(() => (window as any).__wasm_ready === true);
    await page.evaluate(() => (window as any).install_hook());
    await page.evaluate(() => (window as any).triggerPanic());
    capturedFrames = await page.evaluate(() => (window as any).__captured_frames);
  });

  test("captures deep pipeline frames", async () => {
    expect(wasmOf(capturedFrames).length).toBeGreaterThanOrEqual(10);

    const names = namedWasmOf(capturedFrames).map((f) => f.function);
    for (const fn of [
      "trigger_panic",
      "pipeline::ingest",
      "pipeline::validate",
      "pipeline::decode",
      "pipeline::transform",
      "pipeline::normalize",
      "pipeline::enrich",
      "pipeline::compress",
      "pipeline::encrypt",
      "pipeline::flush",
      "pipeline::commit",
    ]) {
      expect(names.some((n) => n.includes(fn)), `missing ${fn}`).toBe(true);
    }
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

test.describe("source locations", () => {
  let frames: any[];

  test.beforeEach(async ({ page }) => {
    await page.goto("http://localhost:3333");
    await page.waitForFunction(() => (window as any).__wasm_ready === true);
    await page.evaluate(() => (window as any).install_hook_with_framemap());
    await page.evaluate(() => (window as any).triggerPanic());
    frames = await page.evaluate(() => (window as any).__captured_frames);
  });

  test("resolved frames have filename, lineno, and colno", async () => {
    const resolved = wasmOf(frames).filter(
      (f) => f.filename && f.lineno != null && f.colno != null
    );
    expect(resolved.length).toBeGreaterThan(0);
  });

  test("fixture source paths are relative", async () => {
    const resolved = wasmOf(frames).filter((f) => f.filename);

    for (const f of resolved) {
      expect(f.filename).not.toMatch(/^\//);
      expect(f.filename).not.toContain("/Users/");
      expect(f.filename).not.toContain("/home/");
    }
  });

  test("resolved source lines match fixture source", async () => {
    const source = readFileSync(
      join(__dirname, "fixture/src/lib.rs"),
      "utf-8"
    ).split("\n");

    for (const [fn_name, expected] of [
      ["wacks_test_fixture::pipeline::commit", "panic!"],
      ["wacks_test_fixture::pipeline::flush", "commit("],
      ["wacks_test_fixture::pipeline::encrypt", "flush("],
      ["wacks_test_fixture::pipeline::compress", "encrypt("],
      ["wacks_test_fixture::pipeline::enrich", "compress("],
      ["wacks_test_fixture::pipeline::normalize", "enrich("],
      ["wacks_test_fixture::pipeline::transform", "normalize("],
      ["wacks_test_fixture::pipeline::decode", "transform("],
      ["wacks_test_fixture::pipeline::validate", "decode("],
      ["wacks_test_fixture::pipeline::ingest", "validate("],
    ]) {
      const f = wasmOf(frames).find((f: any) => f.function === fn_name);
      expect(f, `frame for ${fn_name}`).toBeDefined();
      expect(f.filename, `filename for ${fn_name}`).toMatch(/src\/lib\.rs$/);
      expect(f.lineno, `lineno for ${fn_name}`).toBeGreaterThan(0);

      const context_line = source[f.lineno - 1];
      expect(context_line, `context_line for ${fn_name}`).toContain(expected);
    }
  });
});
