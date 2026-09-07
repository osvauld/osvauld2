// protect.ts — mechanical enforcement of the working rules in AGENTS.md:
//   1. reference-only crates are never written (docs/architecture.md "Crates")
//   2. a single write of >100 changed lines of code warns — slices, not dumps
//      (tests and docs ride free; the cap is about code the user must judge)
import { isToolCallEventType, type ExtensionAPI } from "@earendil-works/pi-coding-agent";
import * as fs from "node:fs";
import * as path from "node:path";

const REFERENCE_CRATES = [
  "app_engine", "sthalam", "doc_editor", "block_doc", "code_editor", "code_highlight",
  "text_edit", "rich_text", "pdf_paint", "table_core", "table_query", "table_import",
];
const SLICE_LIMIT = 100;

function rel(p: string): string {
  try {
    return path.relative(process.cwd(), p);
  } catch {
    return p;
  }
}

function isReference(p: string): boolean {
  const r = rel(p);
  return REFERENCE_CRATES.some((d) => r === d || r.startsWith(d + path.sep) || r.startsWith(d + "/"));
}

// The cap governs code the user judges by reading: Rust/Lua/TOML sources, tests excluded.
function isCappedCode(p: string): boolean {
  const r = rel(p);
  if (!/\.(rs|lua|toml)$/.test(r)) return false;
  if (/(tests?\.rs)$/.test(r) || r.includes("/tests/")) return false;
  return true;
}

// Size of the changed block: strip the common prefix and suffix lines, count what remains
// on the new side. Naive, but the cap is a heuristic anyway.
function changedLines(oldText: string, newText: string): number {
  const a = oldText.split("\n");
  const b = newText.split("\n");
  let pre = 0;
  while (pre < a.length && pre < b.length && a[pre] === b[pre]) pre++;
  let suf = 0;
  while (suf < a.length - pre && suf < b.length - pre && a[a.length - 1 - suf] === b[b.length - 1 - suf]) suf++;
  return Math.max(0, b.length - pre - suf);
}

function blockRef(p: string) {
  return {
    block: true,
    reason:
      `${rel(p)} is in a reference-only crate. Port its lessons, never its code — see ` +
      `docs/architecture.md ("Crates") and docs/status.md. If a port is genuinely wanted, ` +
      `it lands in a live crate with the user's explicit nod.`,
  };
}

export default function (pi: ExtensionAPI) {
  pi.on("tool_call", async (event, ctx) => {
    if (isToolCallEventType("write", event)) {
      const p = event.input.path;
      if (isReference(p)) return blockRef(p);
      if (isCappedCode(p) && typeof event.input.content === "string") {
        let old = "";
        try {
          old = fs.readFileSync(p, "utf8");
        } catch {
          /* new file */
        }
        const n = changedLines(old, event.input.content);
        if (n > SLICE_LIMIT) {
          ctx.ui.notify(
            `⚠ slice cap: ${n} changed lines in ${rel(p)} (cap ${SLICE_LIMIT}). ` +
              `Break it down, or say explicitly that this slice is bigger and why.`
          );
        }
      }
      return;
    }

    if (isToolCallEventType("edit", event)) {
      const p = event.input.path;
      if (isReference(p)) return blockRef(p);
      if (isCappedCode(p) && Array.isArray(event.input.edits)) {
        const n = event.input.edits.reduce(
          (sum, e) => sum + changedLines(String(e.oldText ?? ""), String(e.newText ?? "")),
          0
        );
        if (n > SLICE_LIMIT) {
          ctx.ui.notify(
            `⚠ slice cap: ~${n} changed lines across ${event.input.edits.length} edits in ` +
              `${rel(p)} (cap ${SLICE_LIMIT}). Break it down, or say explicitly why not.`
          );
        }
      }
    }
  });
}
