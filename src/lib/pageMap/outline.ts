// SPDX-License-Identifier: AGPL-3.0-only
import type { PageMapNode } from "../tauri";

const MAX_OUTLINE_DEPTH = 32;
const MAX_OUTLINE_NODES = 500;

/** Markdown text used as a node label must stay text, including on its own line. */
export function escapeOutlineLabel(value: string): string {
  return value
    .replace(/[\r\n\u0000-\u001f\u007f]+/gu, " ")
    .trim()
    .replace(/[\\`*_{}\[\]()<>#+\-.!|>]/gu, "\\$&");
}

/**
 * Render only active parent/child relationships. The page root supplies the
 * outline's context, so its title is deliberately omitted from the bullets.
 */
export function buildActiveOutline(
  nodes: readonly PageMapNode[],
  sectionTitle: string,
  labelOverrides: ReadonlyMap<string, string> = new Map(),
): string | null {
  const active = nodes.filter((node) => node.status === "active");
  const root = active.find((node) => node.parent_id === null);
  if (!root) return null;

  const childrenByParent = new Map<string, PageMapNode[]>();
  const activeIds = new Set(active.map((node) => node.id));
  for (const node of active) {
    if (!node.parent_id || !activeIds.has(node.parent_id)) continue;
    const children = childrenByParent.get(node.parent_id) ?? [];
    children.push(node);
    childrenByParent.set(node.parent_id, children);
  }
  for (const children of childrenByParent.values()) {
    children.sort((left, right) => left.rank - right.rank || left.id.localeCompare(right.id));
  }

  const visited = new Set<string>([root.id]);
  const lines: string[] = [];
  const visit = (parent: PageMapNode, depth: number): void => {
    for (const child of childrenByParent.get(parent.id) ?? []) {
      if (visited.has(child.id)) continue;
      visited.add(child.id);
      if (depth > MAX_OUTLINE_DEPTH) {
        throw new Error(`Page map outline exceeds maximum depth ${MAX_OUTLINE_DEPTH}`);
      }
      if (visited.size > MAX_OUTLINE_NODES) {
        throw new Error(`Page map outline exceeds maximum size ${MAX_OUTLINE_NODES}`);
      }
      const rawLabel = child.label?.trim() ||
        labelOverrides.get(`${child.ref_kind}:${child.ref_id}`)?.trim() || child.ref_id;
      lines.push(`${"  ".repeat(depth)}- ${escapeOutlineLabel(rawLabel)}`);
      visit(child, depth + 1);
    }
  };
  visit(root, 0);
  if (lines.length === 0) return null;
  return `## ${escapeOutlineLabel(sectionTitle)}\n\n${lines.join("\n")}\n`;
}

export interface AppendedOutline {
  content: string;
  appendedText: string;
  alreadyPresent: boolean;
}

/** Append an exact block while leaving every existing byte in place. */
export function appendOutline(content: string, outline: string): AppendedOutline {
  const exactBlock = outline.trimEnd();
  if (!exactBlock) return { content, appendedText: "", alreadyPresent: false };
  const alreadyPresent = content.includes(exactBlock);
  const separator = content.length === 0
    ? ""
    : content.endsWith("\n\n")
      ? ""
      : content.endsWith("\n")
        ? "\n"
        : "\n\n";
  const appendedText = `${separator}${exactBlock}\n`;
  return {
    content: alreadyPresent ? content : content + appendedText,
    appendedText,
    alreadyPresent,
  };
}
