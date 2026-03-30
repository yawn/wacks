import { readFileSync } from "fs";
import { join } from "path";
import { test, expect } from "@playwright/test";
import { SourceMapConsumer } from "source-map-js";

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

test.describe("source map", () => {
  let frames: any[];

  test.beforeEach(async ({ page }) => {
    await page.goto("http://localhost:3333");
    await page.waitForFunction(() => (window as any).__wasm_ready === true);
    await page.evaluate(() => (window as any).install_hook_with_sourcemap_and_framemap("app.wasm.js"));
    await page.evaluate(() => (window as any).triggerPanic());
    frames = await page.evaluate(() => (window as any).__captured_frames);
  });

  test("source map embeds fixture source code", async () => {
    const mapPath = join(__dirname, "static/pkg/wacks_test_fixture_bg.wasm.map");
    const mapJson = JSON.parse(readFileSync(mapPath, "utf-8"));
    const consumer = new SourceMapConsumer(mapJson);

    // The fixture's lib.rs appears as "src/lib.rs" (relative DWARF path)
    const fixtureSource = consumer.sources.find(
      (s: string) => s === "src/lib.rs"
    );
    expect(fixtureSource).toBeDefined();

    const embedded = consumer.sourceContentFor(fixtureSource!);
    const actual = readFileSync(join(__dirname, "fixture/src/lib.rs"), "utf-8");
    expect(embedded).toBe(actual);
  });

  test("resolved source lines match embedded content", async ({
    browserName,
  }) => {
    const mapPath = join(__dirname, "static/pkg/wacks_test_fixture_bg.wasm.map");
    const mapJson = JSON.parse(readFileSync(mapPath, "utf-8"));
    const consumer = new SourceMapConsumer(mapJson);

    const CONTEXT_LINES = 5;

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

      const original = consumer.originalPositionFor({
        line: f.lineno,
        column: f.colno,
      });

      const content = consumer.sourceContentFor(original.source);
      expect(content, `sourcesContent for ${fn_name}`).not.toBeNull();
      const lines = content!.split("\n");

      const context_line = lines[original.line - 1];
      expect(context_line, `context_line for ${fn_name}`).toContain(expected);

      const pre_context = lines.slice(
        Math.max(0, original.line - 1 - CONTEXT_LINES),
        original.line - 1
      );
      const post_context = lines.slice(
        original.line,
        original.line + CONTEXT_LINES
      );

      expect(
        pre_context.length + post_context.length,
        `surrounding context for ${fn_name}`
      ).toBeGreaterThan(0);

      // Full context reconstructs a contiguous source window
      const window = [...pre_context, context_line, ...post_context];
      expect(window.every((l) => typeof l === "string")).toBe(true);
      expect(window.length).toBeGreaterThanOrEqual(CONTEXT_LINES);
    }
  });
});
