// What the list of Personnes shows: everybody, the persons without a group, or a
// group and its sub-groups.

export type GroupSelection = { kind: "all" } | { kind: "none" } | { kind: "group"; id: string }

export const ALL: GroupSelection = { kind: "all" }

export function sameSelection(a: GroupSelection, b: GroupSelection): boolean {
  return a.kind === b.kind && (a.kind !== "group" || (b.kind === "group" && a.id === b.id))
}
