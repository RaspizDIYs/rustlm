import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { resolve, extname, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../out/", import.meta.url));
const fixture = await readFile(new URL("./fixtures/league-sharing-browser.js", import.meta.url), "utf8");
const types = { ".txt": "text/plain; charset=utf-8", ".webp": "image/webp", ".jpg": "image/jpeg", ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".css": "text/css", ".svg": "image/svg+xml", ".png": "image/png", ".woff2": "font/woff2" };
const server = createServer(async (request, response) => {
  try {
    let path = decodeURIComponent(new URL(request.url, "http://127.0.0.1").pathname);
    if (path === "/") path = "/accounts.html";
    else if (path.endsWith("/")) path = path.slice(0, -1) + ".html";
    else if (!extname(path)) path += ".html";
    const file = resolve(root, "." + path);
    if (!file.startsWith(resolve(root) + sep)) { response.writeHead(403).end(); return; }
    let bytes = await readFile(file);
    if (extname(file) === ".html") bytes = Buffer.from(bytes.toString().replace("<head>", `<head><script>${fixture}</script>`));
    response.writeHead(200, { "Content-Type": types[extname(file)] || "application/octet-stream", "Cache-Control": "no-store" }).end(bytes);
  } catch { response.writeHead(404).end("Build the app first: npm run build"); }
});
server.listen(3215, "127.0.0.1", () => {
  process.stdout.write("Sender: http://127.0.0.1:3215/accounts/?side=sender\nRecipient: http://127.0.0.1:3215/accounts/?side=recipient\n");
});
