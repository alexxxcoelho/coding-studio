// TanStack Query keys and hooks of the Analyser and Rédiger screens. Every
// query goes through `analysisApi` (lib/analysis-api.ts), except the ones that
// already exist for other screens (codes, project configuration, cases).

import { useQuery, type QueryClient } from "@tanstack/react-query"

import { analysisApi, type MatrixConfig, type ThemeSummary } from "./analysis-api"
import { api } from "./api"
import { validateMatrix } from "./matrix"
import type { ProjectConfig } from "./types"

export const akeys = {
  themes: ["themes"] as const,
  theme: (id: string) => ["theme", id] as const,
  themePassages: (id: string) => ["theme-passages", id] as const,
  framework: (id: string) => ["framework", id] as const,
  frameworkTable: ["framework-table"] as const,
  matrix: (c: MatrixConfig) => ["matrix", c] as const,
  matrixViews: ["matrix-views"] as const,
  cell: (row: string, col: string, filter?: MatrixConfig["filter"]) => ["matrix-cell", row, col, filter ?? null] as const,
  writing: (id: string) => ["writing", id] as const,
  synthesis: (rq: string) => ["synthesis", rq] as const,
  finalAudit: ["final-audit"] as const,
  cases: ["cases-all"] as const,
  decisions: ["decisions"] as const,
  questions: ["questions"] as const,
  interviews: ["interviews"] as const,
}

export const useThemes = () => useQuery({ queryKey: akeys.themes, queryFn: analysisApi.themes })
export const useTheme = (id: string | null) => useQuery({ queryKey: akeys.theme(id ?? ""), queryFn: () => analysisApi.theme(id!), enabled: !!id })
export const useThemePassages = (id: string | null) =>
  useQuery({ queryKey: akeys.themePassages(id ?? ""), queryFn: () => analysisApi.themePassages(id!), enabled: !!id })
export const useThemeWriting = (id: string | null) =>
  useQuery({ queryKey: akeys.writing(id ?? ""), queryFn: () => analysisApi.themeWriting(id!), enabled: !!id })
export const useFrameworkReading = (id: string | null, enabled = true) =>
  useQuery({ queryKey: akeys.framework(id ?? ""), queryFn: () => analysisApi.frameworkReading(id!), enabled: !!id && enabled })
export const useCasesAll = () => useQuery({ queryKey: akeys.cases, queryFn: api.allCases })
export const useDecisions = () => useQuery({ queryKey: akeys.decisions, queryFn: analysisApi.decisions })
export const useQuestions = () => useQuery({ queryKey: akeys.questions, queryFn: api.questions })

export function useMatrix(config: MatrixConfig, project: ProjectConfig | undefined) {
  const invalid = validateMatrix(config, project)
  return useQuery({ queryKey: akeys.matrix(config), queryFn: () => analysisApi.matrix(config), enabled: !!project && !invalid })
}

/** The retained themes, in the order of the chapter. */
export function retainedThemes(themes: ThemeSummary[] | undefined): ThemeSummary[] {
  return [...(themes ?? [])].filter((t) => t.status === "retained").sort((a, b) => a.ord - b.ord)
}

/** After a change to a theme: the lists, the sheet, the matrix columns and the writing that depend on it. */
export function invalidateThemes(qc: QueryClient, id?: string): Promise<unknown> {
  return qc.invalidateQueries({
    predicate: (q) => {
      const k = q.queryKey[0]
      if (k === "themes" || k === "matrix" || k === "framework-table" || k === "synthesis") return true
      return !!id && (k === "theme" || k === "theme-passages" || k === "writing" || k === "framework") && q.queryKey[1] === id
    },
  })
}

export function invalidateWriting(qc: QueryClient, id: string): Promise<unknown> {
  return qc.invalidateQueries({ predicate: (q) => (q.queryKey[0] === "writing" && q.queryKey[1] === id) || q.queryKey[0] === "final-audit" })
}
