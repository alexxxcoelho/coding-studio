// Checks of the pseudonymisation list before it is sent (the server checks them
// again: docs/API.md, "V1 lots 3 and 4").

interface Row {
  kind: string
  original: string
  pseudonym: string
}

const norm = (s: string) => s.trim().toLocaleLowerCase("fr")

/** The first problem of the list in the study's words, or null. A blank row is a problem, not silently dropped. */
export function entriesProblem(rows: Row[]): string | null {
  const names = new Set<string>()
  for (let i = 0; i < rows.length; i++) {
    const r = rows[i]
    if (norm(r.original) === "") return `Le nom réel de la ligne ${i + 1} est vide.`
    if (norm(r.pseudonym) === "") return `Le pseudonyme de « ${r.original.trim()} » est vide.`
    if (norm(r.original) === norm(r.pseudonym)) return `« ${r.original.trim()} » ne change pas : le pseudonyme doit être différent.`
    if (names.has(norm(r.original))) return `« ${r.original.trim()} » figure deux fois dans la liste.`
    names.add(norm(r.original))
  }
  for (const r of rows) {
    if (names.has(norm(r.pseudonym))) return `Le pseudonyme « ${r.pseudonym.trim()} » est lui-même un nom de la liste.`
  }
  return null
}

export function sameEntries(a: Row[], b: Row[]): boolean {
  return a.length === b.length && a.every((r, i) => r.kind === b[i].kind && r.original === b[i].original && r.pseudonym === b[i].pseudonym)
}
