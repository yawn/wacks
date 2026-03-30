import { readFileSync } from "fs";
import { join } from "path";
import { test, expect } from "@playwright/test";
import { SourceMapConsumer } from "source-map-js";

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

test.describe("source map", () => {
  let frames: any[];

  test.beforeEach(async ({ page }) => {
    await page.goto("http://localhost:3333");
    await page.waitForFunction(() => (window as any).__wasm_ready === true);

    await page.evaluate(async () => {
      const resp = await fetch("./pkg/wacks_test_fixture_bg.wasm.map");
      const json = await resp.text();
      (window as any).setup_source_map(json);
    });

    await page.evaluate(() => (window as any).triggerPanic());
    frames = await page.evaluate(() => (window as any).__captured_frames);
  });

  test("resolves source file for wasm frames", async ({ browserName }) => {
    test.skip(browserName === "webkit", "WebKit omits wasm byte offsets");
    const wasm = wasmOf(frames).filter((f) => f.filename);
    expect(wasm.length).toBeGreaterThan(0);

    for (const f of wasm) {
      expect(f.filename).toMatch(/\.rs$/);
      expect(f.lineno).toBeGreaterThan(0);
    }
  });

  test("fixture functions resolve to fixture source", async ({
    browserName,
  }) => {
    test.skip(browserName === "webkit", "WebKit omits wasm byte offsets");
    const fixture = frames.filter(
      (f) =>
        f.filename &&
        f.function &&
        /^wacks_test_fixture::level_\d$/.test(f.function)
    );
    expect(fixture.length).toBeGreaterThan(0);

    for (const f of fixture) {
      expect(f.filename).toContain("lib.rs");
    }
  });

  test("resolved lines contain expected source code", async ({
    browserName,
  }) => {
    test.skip(browserName === "webkit", "WebKit omits wasm byte offsets");

    const source = readFileSync(
      join(__dirname, "fixture/src/lib.rs"),
      "utf-8"
    ).split("\n");

    for (const [fn_name, expected] of [
      ["wacks_test_fixture::level_3", "panic!"],
      ["wacks_test_fixture::level_2", "level_3()"],
      ["wacks_test_fixture::level_1", "level_2()"],
    ]) {
      const f = frames.find((f: any) => f.function === fn_name);
      expect(f, `frame for ${fn_name}`).toBeDefined();
      const line = source[f.lineno - 1];
      expect(line, `${fn_name} at line ${f.lineno}`).toContain(expected);
    }
  });
});

test.describe("source map proxy", () => {
  let frames: any[];

  test.beforeEach(async ({ page }) => {
    await page.goto("http://localhost:3333");
    await page.waitForFunction(() => (window as any).__wasm_ready === true);

    await page.evaluate(() => {
      (window as any).setup_source_map_proxy("app.wasm.js");
    });

    await page.evaluate(() => (window as any).triggerPanic());
    frames = await page.evaluate(() => (window as any).__captured_frames);
  });

  test("rewrites wasm frames as JS-compatible locations", async ({
    browserName,
  }) => {
    test.skip(browserName === "webkit", "WebKit omits wasm byte offsets");
    const wasm = wasmOf(frames).filter((f) => f.filename);
    expect(wasm.length).toBeGreaterThan(0);

    for (const f of wasm) {
      expect(f.filename).toBe("app.wasm.js");
      expect(f.lineno).toBe(1);
      expect(f.colno).toBeGreaterThan(0);
    }
  });

  test("proxy frames resolve via standard JS source map consumer", async ({
    browserName,
  }) => {
    test.skip(browserName === "webkit", "WebKit omits wasm byte offsets");

    const mapPath = join(__dirname, "static/pkg/wacks_test_fixture_bg.wasm.map");
    const mapJson = JSON.parse(readFileSync(mapPath, "utf-8"));
    const consumer = new SourceMapConsumer(mapJson);

    const fixture = wasmOf(frames).filter(
      (f) =>
        f.colno != null &&
        f.function &&
        /^wacks_test_fixture::level_\d$/.test(f.function)
    );
    expect(fixture.length).toBeGreaterThan(0);

    for (const f of fixture) {
      const original = consumer.originalPositionFor({
        line: f.lineno,
        column: f.colno,
      });

      expect(original.source).toContain("lib.rs");
      expect(original.line).toBeGreaterThan(0);
    }
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
    test.skip(browserName === "webkit", "WebKit omits wasm byte offsets");

    const mapPath = join(__dirname, "static/pkg/wacks_test_fixture_bg.wasm.map");
    const mapJson = JSON.parse(readFileSync(mapPath, "utf-8"));
    const consumer = new SourceMapConsumer(mapJson);

    for (const [fn_name, expected] of [
      ["wacks_test_fixture::level_3", "panic!"],
      ["wacks_test_fixture::level_2", "level_3()"],
      ["wacks_test_fixture::level_1", "level_2()"],
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
      expect(
        lines[original.line - 1],
        `${fn_name} at line ${original.line}`
      ).toContain(expected);
    }
  });
});
