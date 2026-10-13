// Small display helpers.

export function shortHash(h: string | null | undefined, n = 10): string {
  return h ? h.slice(0, n) : ""
}

export function bytes(n: number): string {
  if (n < 1024) return `${n} B`
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`
  return `${(n / 1024 / 1024).toFixed(1)} MB`
}

export function dateTime(iso: string | null | undefined): string {
  if (!iso) return ""
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return iso
  return d.toLocaleString("fr-CH", { dateStyle: "medium", timeStyle: "short" })
}

export function percent(x: number, digits = 0): string {
  return `${(x * 100).toFixed(digits)} %`
}

export function plural(n: number, one: string, many = `${one}s`): string {
  // French: zero and one are singular.
  return `${n} ${n < 2 ? one : many}`
}

/** Local time as a value for <input type="datetime-local">. */
export function nowLocalInput(): string {
  const d = new Date()
  const pad = (x: number) => String(x).padStart(2, "0")
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
}

export function roleLabel(role: string): string {
  return role === "interviewer" ? "INT" : role === "respondent" ? "RESP" : "UNK"
}
