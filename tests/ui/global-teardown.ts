/**
 * Writes `test-results/screenshots/index.html`, a contact sheet of every
 * screenshot the run took, grouped by folder, so a reviewer can look at every
 * combination in one page without opening hundreds of files.
 */
import { readdirSync, statSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";
import process from "node:process";

const ROOT = join(process.cwd(), "test-results", "screenshots");

function pictures(directory: string): string[] {
  return readdirSync(directory).flatMap((entry) => {
    const path = join(directory, entry);
    if (statSync(path).isDirectory()) return pictures(path);
    return entry.endsWith(".png") ? [path] : [];
  });
}

export default function globalTeardown(): void {
  let files: string[];
  try {
    files = pictures(ROOT).sort();
  } catch {
    return;
  }
  const groups = new Map<string, string[]>();
  for (const file of files) {
    const path = relative(ROOT, file);
    const folder = path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "(top)";
    groups.set(folder, [...(groups.get(folder) ?? []), path]);
  }
  const body = [...groups]
    .map(
      ([folder, paths]) =>
        `<h2>${folder} <small>${String(paths.length)}</small></h2><div class="grid">${paths
          .map(
            (path) =>
              `<figure><img loading="lazy" src="${path}" alt="${path}"><figcaption>${path.slice(path.lastIndexOf("/") + 1, -4)}</figcaption></figure>`,
          )
          .join("")}</div>`,
    )
    .join("\n");
  writeFileSync(
    join(ROOT, "index.html"),
    `<!doctype html><meta charset="utf-8"><title>Quota interface screenshots</title>
<style>body{font:14px system-ui;margin:16px;background:#eee}h2{margin-top:32px}small{color:#666}
.grid{display:flex;flex-wrap:wrap;gap:12px;align-items:flex-start}figure{margin:0;background:#fff;padding:6px;border-radius:6px}
img{display:block;max-width:440px;height:auto;background:repeating-conic-gradient(#ddd 0 25%,#fff 0 50%) 0 0/16px 16px}
figcaption{font-size:11px;color:#444;margin-top:4px;max-width:440px;overflow-wrap:anywhere}</style>
<h1>Quota interface screenshots (${String(files.length)})</h1>
${body}`,
  );
}
