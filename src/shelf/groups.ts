// Subgroups. A group `父-子` sits under `父`, as long as a group of that name exists: `Sci-Fi` is
// a plain group until there is a group called `Sci`. There are two levels at most, and the same
// rule is in library.rs.

const SEPARATOR = "-";

/** The group `name` sits under, if any: the longest top-level group it continues. */
export function parentOf(name: string, groups: string[]): string | null {
  const candidates = groups.filter((g) => g !== name && name.startsWith(g + SEPARATOR) && parentOf(g, groups) === null);
  return candidates.sort((a, b) => b.length - a.length)[0] ?? null;
}

/** What a group is called under its parent. */
export function labelOf(name: string, parent: string | null): string {
  return parent ? name.slice(parent.length + SEPARATOR.length) : name;
}

export const childName = (parent: string | null, label: string) => (parent ? `${parent}${SEPARATOR}${label.trim()}` : label.trim());

export interface GroupNode {
  name: string;
  children: string[];
}

/** Top-level groups with their subgroups, in the shelf's order. */
export function groupTree(groups: string[]): GroupNode[] {
  return groups.filter((g) => parentOf(g, groups) === null).map((name) => ({ name, children: groups.filter((g) => parentOf(g, groups) === name) }));
}

/** Whether a book in group `group` shows under `filter`: a group includes its subgroups. */
export function inGroup(group: string | null, filter: string, groups: string[]): boolean {
  return !!group && (group === filter || parentOf(group, groups) === filter);
}
