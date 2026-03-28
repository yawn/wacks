import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { join, extname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(fileURLToPath(new URL(".", import.meta.url)), "static");

const MIME = {
  ".html": "text/html",
  ".js": "application/javascript",
  ".wasm": "application/wasm",
};

createServer(async (req, res) => {
  const url = new URL(req.url, "http://localhost");
  const filePath = join(root, url.pathname === "/" ? "index.html" : url.pathname);
  try {
    const data = await readFile(filePath);
    const ct = MIME[extname(filePath)] || "application/octet-stream";
    res.writeHead(200, { "Content-Type": ct });
    res.end(data);
  } catch {
    res.writeHead(404);
    res.end("Not found");
  }
}).listen(3333, () => console.log("serving on http://localhost:3333"));
