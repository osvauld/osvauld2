// protect.ts — mechanical enforcement of the working rules in AGENTS.md:
//   reference-only crates are never written (docs/architecture.md "Crates").
//   (The ~100-line slice-cap warning was removed 2026-10-03 with the cap itself.)
import { isToolCallEventType, type ExtensionAPI } from "@earendil-works/pi-coding-agent";
import * as path from "node:path";

const REFERENCE_CRATES = [
  "app_engine", "sthalam", "doc_editor", "block_doc", "code_editor", "code_highlight",
  "text_edit", "rich_text", "pdf_paint", "table_core", "table_query", "table_import",
];

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

function blockRef(p: string) {
  return {
    block: true,
    reason:
      `${rel(p)} is a removed reference-only crate (read it at the \`reference-crates\` tag). ` +
      `Port its lessons, never its code — see docs/architecture.md ("Crates"). If a port is genuinely wanted, ` +
      `it lands in a live crate with the user's explicit nod.`,
  };
}

export default function (pi: ExtensionAPI) {
  pi.on("tool_call", async (event) => {
    if (isToolCallEventType("write", event) || isToolCallEventType("edit", event)) {
      const p = event.input.path;
      if (isReference(p)) return blockRef(p);
    }
  });
}
