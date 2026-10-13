// Client of the register of the people interviewed (docs/API.md, "Personnes et
// groupes"): persons and organisations, the tree of groups, the links to
// interviews, the CSV import and the verification package. The shapes are the
// ones the server answers.

import { useQuery, type QueryClient } from "@tanstack/react-query"

import { request } from "./api"
import type { AttributeDef } from "./types"
import { t } from "@/i18n"

const enc = encodeURIComponent

export type PersonKind = "person" | "organisation"
export const KINDS: PersonKind[] = ["person", "organisation"]

/** The kind a new record starts as, given the kind filter of the list. */
export function startKind(filter: string): PersonKind {
  return filter === "organisation" ? "organisation" : "person"
}
export type InterviewRole = "respondent" | "co_respondent" | "mentioned" | "interviewer"
export type ConsentStatus = "" | "pending" | "given" | "refused" | "withdrawn"

export const ROLES: InterviewRole[] = ["respondent", "co_respondent", "mentioned", "interviewer"]
export const CONSENT_STATUSES: ConsentStatus[] = ["", "pending", "given", "refused", "withdrawn"]

export function roleLabel(role: string): string {
  switch (role) {
    case "respondent":
      return t("persons.role_respondent")
    case "co_respondent":
      return t("persons.role_co_respondent")
    case "mentioned":
      return t("persons.role_mentioned")
    case "interviewer":
      return t("persons.role_interviewer")
    default:
      return role
  }
}

export function consentLabel(status: string): string {
  switch (status) {
    case "pending":
      return t("persons.consent_pending")
    case "given":
      return t("persons.consent_given")
    case "refused":
      return t("persons.consent_refused")
    case "withdrawn":
      return t("persons.consent_withdrawn")
    default:
      return t("persons.consent_none")
  }
}

export function kindLabel(kind: string): string {
  return kind === "organisation" ? t("common.organisation") : t("persons.kind_person")
}

export interface Person {
  person_id: string
  kind: PersonKind
  name: string
  organisation: string
  function: string
  email: string
  phone: string
  notes: string
  consent_status: ConsentStatus
  consent_date: string
  consent_document: string
  anonymise: boolean
  pseudonym: string
  aliases: string[]
  attributes: Record<string, string>
  group_ids: string[]
  created_at: string
  updated_at: string
}

/** A row of the list: the record and the number of interviews it is linked to. */
export interface PersonRow extends Person {
  n_interviews: number
}

export interface PersonList {
  n: number
  persons: PersonRow[]
}

export interface GroupRef {
  group_id: string
  name: string
  path: string
}

export interface PersonInterview {
  interview_id: string
  role: InterviewRole
  folder_name: string
  attributes: Record<string, string>
}

export interface PersonDetail extends Person {
  groups: GroupRef[]
  interviews: PersonInterview[]
  attribute_definitions: AttributeDef[]
  interview_attribute_definitions: AttributeDef[]
}

/** A person of an interview, with the role in it. */
export interface InterviewPerson extends Person {
  role: InterviewRole
}

export interface GroupNode {
  group_id: string
  name: string
  parent_id: string | null
  description: string
  created_at: string
  updated_at: string
  /** Slash-separated names from the root. */
  path: string
  depth: number
  /** Members of the group itself. */
  n_persons: number
  /** Distinct members of the group and of every group below it. */
  n_persons_total: number
  n_interviews_total: number
  children: GroupNode[]
}

export interface GroupTree {
  groups: GroupNode[]
  n_groups: number
  n_persons: number
  n_ungrouped_persons: number
}

export interface PersonFilters {
  kind?: PersonKind
  group_id?: string
  interview_id?: string
  anonymise?: boolean
  q?: string
}

/** The body of a create or update: absent fields are left as they are. */
export interface PersonBody {
  kind?: PersonKind
  name?: string
  organisation?: string
  function?: string
  email?: string
  phone?: string
  notes?: string
  consent_status?: ConsentStatus
  consent_date?: string
  consent_document?: string
  anonymise?: boolean
  pseudonym?: string
  aliases?: string[]
  attributes?: Record<string, string | null>
  group_ids?: string[]
}

export interface ImportProblem {
  row: number
  message: string
}

export interface ImportRow {
  row: number
  action: "create" | "update" | "unchanged"
  person_id: string
  name: string
}

export interface ImportReport {
  dry_run: boolean
  applied: boolean
  ok: boolean
  n_rows: number
  n_create: number
  n_update: number
  n_unchanged: number
  n_links: number
  groups_to_create: string[]
  rows: ImportRow[]
  problems: ImportProblem[]
  warnings: string[]
  csv_sha256: string
}

export interface VerificationResult {
  dir: string
  files: { name: string; sha256: string; size: number }[]
  manifest_sha256: string
  interview_ids: string[]
  /** Replacements by kind of record, never the names. */
  replacements: Record<string, number>
  pseudonymisation_digest: string
  warnings: string[]
}

function query(filters: PersonFilters): string {
  const q = new URLSearchParams()
  if (filters.kind) q.set("kind", filters.kind)
  if (filters.group_id) q.set("group_id", filters.group_id)
  if (filters.interview_id) q.set("interview_id", filters.interview_id)
  if (filters.anonymise !== undefined) q.set("anonymise", String(filters.anonymise))
  if (filters.q?.trim()) q.set("q", filters.q.trim())
  const s = q.toString()
  return s ? `?${s}` : ""
}

export const personsApi = {
  list: (filters: PersonFilters = {}) => request<PersonList>("GET", `/api/persons${query(filters)}`),
  get: (pid: string) => request<PersonDetail>("GET", `/api/persons/${enc(pid)}`),
  create: (body: PersonBody) => request<Person>("POST", "/api/persons", body),
  update: (pid: string, body: PersonBody) => request<Person>("PUT", `/api/persons/${enc(pid)}`, body),
  remove: (pid: string, withLinks = false) => request<{ deleted: string }>("DELETE", `/api/persons/${enc(pid)}${withLinks ? "?with_links=true" : ""}`),
  link: (pid: string, interview_id: string, role: InterviewRole) => request<PersonDetail>("POST", `/api/persons/${enc(pid)}/interviews`, { interview_id, role }),
  unlink: (pid: string, interviewId: string) => request<PersonDetail>("DELETE", `/api/persons/${enc(pid)}/interviews/${enc(interviewId)}`),
  interviewPersons: (id: string) => request<{ interview_id: string; persons: InterviewPerson[] }>("GET", `/api/interviews/${enc(id)}/persons`),
  linkToInterview: (id: string, person_id: string, role: InterviewRole) =>
    request<{ interview_id: string; persons: InterviewPerson[] }>("POST", `/api/interviews/${enc(id)}/persons`, { person_id, role }),
  unlinkFromInterview: (id: string, pid: string) => request<{ interview_id: string; persons: InterviewPerson[] }>("DELETE", `/api/interviews/${enc(id)}/persons/${enc(pid)}`),
  groups: () => request<GroupTree>("GET", "/api/person-groups"),
  createGroup: (body: { name: string; parent_id?: string | null; description?: string }) => request<GroupNode>("POST", "/api/person-groups", body),
  updateGroup: (gid: string, body: { name?: string; parent_id?: string | null; description?: string }) => request<GroupNode>("PUT", `/api/person-groups/${enc(gid)}`, body),
  deleteGroup: (gid: string, reassignTo?: string) =>
    request<{ deleted: string }>("DELETE", `/api/person-groups/${enc(gid)}${reassignTo ? `?reassign_to=${enc(reassignTo)}` : ""}`),
  importCsv: (csv: string, dry_run: boolean) => request<ImportReport>("POST", "/api/persons/import", { csv, dry_run }),
  verificationExport: (body: { interview_ids?: string[]; all?: boolean; out_dir?: string }) => request<VerificationResult>("POST", "/api/export/verification", body),
}

export const personKeys = {
  all: ["persons"] as const,
  list: (f: PersonFilters) => ["persons", "list", f] as const,
  detail: (pid: string) => ["persons", "detail", pid] as const,
  groups: ["persons", "groups"] as const,
  ofInterview: (id: string) => ["persons", "interview", id] as const,
}

export const usePersonGroups = () => useQuery({ queryKey: personKeys.groups, queryFn: personsApi.groups })
export const usePersons = (filters: PersonFilters = {}) => useQuery({ queryKey: personKeys.list(filters), queryFn: () => personsApi.list(filters) })
export const usePersonDetail = (pid: string | null) => useQuery({ queryKey: personKeys.detail(pid ?? ""), queryFn: () => personsApi.get(pid!), enabled: !!pid })
export const useInterviewPersons = (id: string) => useQuery({ queryKey: personKeys.ofInterview(id), queryFn: () => personsApi.interviewPersons(id) })

/** The groups of a tree, flattened in display order with their depth. */
export function flattenGroups(nodes: GroupNode[]): GroupNode[] {
  const out: GroupNode[] = []
  const walk = (list: GroupNode[]) => {
    for (const n of list) {
      out.push(n)
      walk(n.children)
    }
  }
  walk(nodes)
  return out
}

/** Ids of a group and of everything below it. */
export function subtreeIds(node: GroupNode): string[] {
  return [node.group_id, ...node.children.flatMap(subtreeIds)]
}

/** After any write on the register: the lists, the tree, the pseudonymisation list and the audit chain badge. */
export function invalidateRegister(qc: QueryClient): Promise<unknown> {
  return Promise.all([
    qc.invalidateQueries({ queryKey: personKeys.all }),
    qc.invalidateQueries({ queryKey: ["project-pseudonymisation"] }),
    qc.invalidateQueries({ queryKey: ["pseudonymisation"] }),
    qc.invalidateQueries({ queryKey: ["chain"] }),
  ])
}
