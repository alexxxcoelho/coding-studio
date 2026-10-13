// Test data of the Analyser and Rédiger screens. Fictional library study.

import type {
  AuditClaim,
  DecisionRecord,
  FinalAudit,
  MatrixResult,
  PassageBrief,
  PassageLink,
  ThemePassage,
  ThemeSheet,
  ThemeSummary,
  ThemeWriting,
} from "./analysis-api"
import { THEME_PARTS } from "./analysis-api"
import type { Case, CodeNode, ProjectConfig } from "./types"

export const project = {
  attributes: [
    { id: "sector", label: "Type de bibliothèque", kind: "choice", choices: ["scolaire", "communale"], shared_with_model: false },
    { id: "stance", label: "Statut des propos", kind: "choice", choices: ["déploiement vécu", "projet"], shared_with_model: false },
  ],
  memo_templates: [],
  checklist: [],
  theme_checks: [
    { id: "coherence", label: "Les extraits sont-ils cohérents entre eux ?" },
    { id: "distinct", label: "Le thème se distingue-t-il des autres ?" },
    { id: "asserts", label: "L'intitulé affirme-t-il quelque chose ?" },
  ],
  evidence_natures: [
    { id: "first-hand", label: "Témoignage de première main" },
    { id: "opinion", label: "Opinion" },
  ],
  link_types: [
    { id: "supports", label: "soutient" },
    { id: "nuances", label: "nuance" },
    { id: "contradicts", label: "contredit" },
  ],
  matrix_dimensions: [{ id: "who-pays", label: "Qui paie" }],
  interpretive_framework: null,
} as unknown as ProjectConfig

export const projectWithFramework = {
  ...project,
  interpretive_framework: {
    preset: "toe",
    dimensions: [
      { id: "T", label: "Technologie", definition: "" },
      { id: "O", label: "Organisation", definition: "" },
      { id: "E", label: "Environnement", definition: "" },
    ],
  },
} as unknown as ProjectConfig

export const themeSummaries: ThemeSummary[] = [
  { theme_id: "T-0001", title: "La non-adoption n'est pas toujours un échec", status: "revised", ord: 1, codes_count: 2, interviews_count: 9, research_questions: ["Q1"] },
  { theme_id: "T-0002", title: "Le groupe impose le marquage", status: "retained", ord: 2, codes_count: 3, interviews_count: 8, research_questions: ["Q1", "Q2"] },
  { theme_id: "T-0003", title: "Qui supporte les coûts", status: "retained", ord: 3, codes_count: 1, interviews_count: 11, research_questions: [] },
]

export const sheet = (over: Partial<ThemeSheet> = {}): ThemeSheet => ({
  ...themeSummaries[0],
  proposition: "Non-adoption rationnelle plutôt que frein.",
  conditions: "",
  alternative_interpretation: "",
  limits: "",
  decision_memo: "",
  codes: [{ code_id: "C-0001", name: "Absence de besoin", passages: 12, interviews: 9 }],
  supporting_cases: [],
  divergent_cases: [],
  checks: [
    { check_id: "coherence", done: true, note: "" },
    { check_id: "distinct", done: false, note: "" },
    { check_id: "asserts", done: false, note: "" },
  ],
  updated_at: "2026-10-13T10:00:00Z",
  ...over,
})

export const codeNode = (id: string, name: string, parent: string | null = null): CodeNode =>
  ({
    code_id: id,
    token: id,
    name,
    label: name,
    definition: "",
    includes: "",
    excludes: "",
    parent_id: parent,
    origin: "field",
    toe: null,
    research_questions: [],
    active: true,
    system: false,
    in_prompt: true,
    merged_into: null,
    version: 1,
    example: null,
    reason: null,
    origin_passage_id: null,
    example_passage_id: null,
    created_at: null,
    aliases: [],
    counts: { passages: 4, interviews: 2, passages_rollup: 4, interviews_rollup: 2 },
  }) as CodeNode

export const codes: CodeNode[] = [codeNode("C-0001", "Absence de besoin"), codeNode("C-0002", "Lot ou unité"), codeNode("C-0003", "Prêt papier suffisant", "C-0001")]

export const brief = (n: number, over: Partial<PassageBrief> = {}): PassageBrief => ({
  passage_id: `P-${n}`,
  interview_id: "ECO-07",
  unit_id: `ECO-07-U0${n}`,
  text: `Extrait numéro ${n}.`,
  verification: "verified",
  ...over,
})

export const themePassages: ThemePassage[] = [
  { ...brief(18), code_ids: ["C-0001"], cell_value: "present" },
  { ...brief(61, { interview_id: "ECO-32", unit_id: "ECO-32-U061", verification: "unverified" }), code_ids: ["C-0002"], cell_value: "contradicted" },
]

export const matrix: MatrixResult = {
  rows: [
    { key: "interview:ECO-16", label: "ECO-16", coded: true },
    { key: "interview:RET-22", label: "RET-22", coded: true },
    { key: "interview:ECO-05", label: "ECO-05", coded: false },
  ],
  cols: [
    { key: "theme:T-0001", label: "Absence de besoin", coded: true },
    { key: "theme:T-0002", label: "Qui supporte les coûts", coded: true },
  ],
  cells: [
    { row_key: "interview:ECO-16", col_key: "theme:T-0001", value: "present", summary: "Argument exprimé, sans regret", n_passages: 3, n_interviews: 1 },
    { row_key: "interview:ECO-16", col_key: "theme:T-0002", value: null, summary: "", n_passages: 5, n_interviews: 1 },
    { row_key: "interview:RET-22", col_key: "theme:T-0002", value: "contradicted", summary: "Le groupe paie, le magasin paie", n_passages: 2, n_interviews: 1 },
  ],
}

export const link = (n: number, part: (typeof THEME_PARTS)[number]["key"], type = "supports", note = ""): PassageLink => ({
  link_id: `L-${n}`,
  passage: brief(n),
  target: { kind: "theme_part", theme_id: "T-0002", part },
  link_type: type,
  note,
})

export const writing = (over: Partial<ThemeWriting> = {}): ThemeWriting => ({
  theme_id: "T-0002",
  title: "Le groupe impose le marquage",
  status: "retained",
  research_questions: ["Q1"],
  parts: Object.fromEntries(THEME_PARTS.map((p) => [p.key, { text: "", nature: null, updated_at: null, links: [] }])) as unknown as ThemeWriting["parts"],
  ...over,
})

export const claim = (n: number, over: Partial<AuditClaim> = {}): AuditClaim => ({
  claim_key: `theme_part:T-0002:mechanism_evidence${n}`,
  part_title: "Mécanisme et preuves",
  sentence: `Le groupe exige le marquage ${n}.`,
  theme: { theme_id: "T-0002", title: "Le groupe impose le marquage" },
  rq_id: null,
  codes: [{ code_id: "C-0001", name: "Absence de besoin" }],
  links: [{ link_id: `L-${n}`, link_type: "supports", note: "Exigence", passage: brief(18), interview_version: "v2" }],
  gaps: [],
  verdict: null,
  ...over,
})

export const audit = (claims: AuditClaim[]): FinalAudit => ({ audit_id: 4, seed: "abcd", n: 3, created_at: "2026-10-13T11:00:00Z", claims })

export const cases: Case[] = [
  { case_id: "ECO-05-C1", interview_id: "ECO-05", label: "Commune X, nouveau système de prêt", decision: "adopted", units: ["ECO-05-U001"], interviews: ["ECO-05"] } as unknown as Case,
  { case_id: "RET-22-C1", interview_id: "RET-22", label: "RET-22, projet textile", decision: "rejected", units: [], interviews: ["RET-22"] } as unknown as Case,
]

/** The same decisions as `/api/decisions` answers them: `name` and `outcome` in the words of the guide. */
export const decisions: DecisionRecord[] = [
  { ...cases[0], decision_id: "ECO-05-C1", name: "Commune X, nouveau système de prêt", outcome: "adopt", outcome_label: "adopter", passages: [], characteristics: {} } as unknown as DecisionRecord,
  { ...cases[1], decision_id: "RET-22-C1", name: "RET-22, projet textile", outcome: "reject", outcome_label: "refuser", passages: [], characteristics: {} } as unknown as DecisionRecord,
]
