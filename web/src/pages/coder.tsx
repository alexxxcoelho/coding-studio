import {ParticipantExport} from "@/components/interview/participant-export"
// Coder (design/macos/Coder.dc.html): the transcript with its turns, the
// filters and the progress, the inspector (Codes, Détail, Mémo), the panel
// "Fin d'entretien" and the bar of shortcuts.

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { CodeFinder } from "@/components/code-finder"
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog"
import { CodeList } from "@/components/coder/code-list"
import { DetailPanel } from "@/components/coder/detail-panel"
import { EndPanel } from "@/components/coder/end-panel"
import { MemoPanel } from "@/components/coder/memo-panel"
import { LexiconPassageTab } from "@/components/lexicon/passage-tab"
import { NewCodeDialog, type NewCodeTarget } from "@/components/coder/new-code-dialog"
import { SelectionMenu } from "@/components/coder/selection-menu"
import { Turn } from "@/components/coder/turn"
import { EmptyState, PageTitle } from "@/components/bits"
import { SearchIcon } from "@/components/icons"
import { AudioCapsule } from "@/components/coder/audio-capsule"
import { t } from "@/i18n"
import { AudioProvider } from "@/lib/audio-player"
import { CorrectTextDialog } from "@/components/coder/correct-text"
import { ReviewDialog } from "@/components/coder/review-dialog"
import { useReviewCounts } from "@/lib/transcript-review"
import { ToolbarActions } from "@/components/toolbar"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { api } from "@/lib/api"
import { codeForNumber, firstApplicable, LATER_ID, numberKeys } from "@/lib/code-tree"
import { useCodeSearchRows } from "@/lib/code-search"
import { resolveKey, isTypingTarget } from "@/lib/coder-keys"
import {
  applyFilter,
  countStates,
  FILTER_LABELS,
  groupByUnit,

  initialUnit,
  isCodable,
  isEndOfInterview,
  neighbour,
  unitsFromRows,
  type CoderFilter,
} from "@/lib/coder-model"
import { invalidatePassages, keys, useCodes, useCodingActions, useMemoView, usePassages, useUnits } from "@/lib/hooks"
import { isFirstVisit } from "@/lib/memo-overrides"
import { isMac } from "@/lib/nav"
import { appliedCodes } from "@/lib/passage-cache"
import { interviewHref } from "@/lib/router"
import { UndoStack } from "@/lib/undo"
import type { Passage, UnitMark } from "@/lib/types"
import { cn } from "@/lib/utils"

type Tab = "codes" | "detail" | "lexique" | "memo"

function visitedKey(id: string) {
  return `cs-visited-${id}`
}
function wasVisited(id: string): boolean {
  try {
    return window.localStorage.getItem(visitedKey(id)) === "1"
  } catch {
    return false
  }
}
function markVisited(id: string) {
  try {
    window.localStorage.setItem(visitedKey(id), "1")
  } catch {
    // Storage unavailable: the memo opens first again next time.
  }
}

export function StepsCapsule({ id, current, highlights }: { id: string; current: "coder" | "surlignages" | "revoir" | null; highlights?: number }) {
  return (
    <nav className="cap" aria-label={t("coder.etapes_de_l_entretien")}>
      <a className={cn("ci", current === "coder" && "on")} href={interviewHref(id, "coding")} aria-current={current === "coder" ? "page" : undefined}>
        {t("common.coder")}
      </a>
      <a className={cn("ci", current === "surlignages" && "on")} href={`#/interviews/${encodeURIComponent(id)}/highlights`} aria-current={current === "surlignages" ? "page" : undefined}>
        {t("common.surlignages")}{highlights !== undefined && highlights > 0 && <span className="n">{highlights}</span>}
      </a>
      <a className={cn("ci", current === "revoir" && "on")} href={interviewHref(id, "review")} aria-current={current === "revoir" ? "page" : undefined}>
        {t("coder.revoir")}
      </a>
    </nav>
  )
}

export function CoderPage({ id, unit: askedUnit = null }: { id: string; unit?: string | null }) {
  const qc = useQueryClient()
  const unitsQ = useUnits(id)
  const passagesQ = usePassages(id)
  const codesQ = useCodes()
  const memoQ = useMemoView(id)
  const marksQ = useQuery({ queryKey: keys.marks(id), queryFn: () => api.unitMarks(id) })

  const [undo] = useState(() => new UndoStack())
  const actions = useCodingActions(id, undo)

  const [codeSearchOpen, setCodeSearchOpen] = useState(false)
  const [reviewOpen, setReviewOpen] = useState(false)
  const [correctOpen, setCorrectOpen] = useState(false)
  // Number(): keeps React Compiler from tying the count to the Map it comes from once t() receives it.
  const nReview = Number(useReviewCounts().get(id) ?? 0)
  const [filter, setFilter] = useState<CoderFilter>("all")
  const [search, setSearch] = useState("")
  const [picked, setSelected] = useState<string | null>(null)
  const [seenAsked, setSeenAsked] = useState(askedUnit)
  const [pending, setPending] = useState<{ unitId: string; start: number; end: number } | null>(null)
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null)
  const [tab, setTab] = useState<Tab>("codes")
  const [tabChosen, setTabChosen] = useState(false)
  const [query, setQuery] = useState("")
  const [passageId, setPassageId] = useState<string | null>(null)
  const [newCode, setNewCode] = useState<NewCodeTarget | null>(null)

  const sectionRef = useRef<HTMLElement>(null)
  const searchRef = useRef<HTMLInputElement>(null)
  const endRef = useRef<HTMLElement>(null)

  const units = useMemo(() => (unitsQ.data ? unitsFromRows(unitsQ.data) : []), [unitsQ.data])
  const passages = useMemo(() => passagesQ.data ?? [], [passagesQ.data])
  const byUnit = useMemo(() => groupByUnit(passages), [passages])
  const codes = useMemo(() => codesQ.data?.codes ?? [], [codesQ.data])
  const marked = useMemo(() => new Set((marksQ.data ?? []).map((m) => m.unit_id)), [marksQ.data])
  const counts = useMemo(() => countStates(units, byUnit, marked), [units, byUnit, marked])
  const shown = useMemo(() => applyFilter(units, byUnit, filter, search, marked), [units, byUnit, filter, search, marked])
  const found = useCodeSearchRows(codes, query)
  const rows = found.rows
  const keyMap = useMemo(() => numberKeys(rows), [rows])
  const nameById = useMemo(() => new Map(codes.map((c) => [c.code_id, c.name])), [codes])

  // A unit asked by the link wins; until one is picked, the first unit to code is selected.
  if (askedUnit !== seenAsked) {
    setSeenAsked(askedUnit)
    if (askedUnit) setSelected(askedUnit)
  }
  const selected = picked ?? (units.length > 0 && passagesQ.data ? initialUnit(units, byUnit, askedUnit) : null)

  // First visit of an interview without memo answers: the memo opens first.
  const [visitChecked, setVisitChecked] = useState<string | null>(null)
  if (visitChecked !== id && memoQ.data && !tabChosen) {
    setVisitChecked(id)
    if (!wasVisited(id) && isFirstVisit(memoQ.data)) setTab("memo")
  }
  const visitedOnce = !!memoQ.data
  useEffect(() => {
    if (visitedOnce) markVisited(id)
  }, [visitedOnce, id])
  const chooseTab = (t: Tab) => {
    setTab(t)
    setTabChosen(true)
  }

  const current = units.find((u) => u.unit_id === selected) ?? null
  const currentPassages = current ? (byUnit.get(current.unit_id) ?? []) : []
  const range = useMemo(
    () => (pending && current && pending.unitId === current.unit_id ? { start: pending.start, end: pending.end } : null),
    [pending, current],
  )
  const detailPassage = passages.find((p) => p.passage_id === passageId) ?? null
  const applied = current ? appliedCodes(passages, { unitId: current.unit_id, ...(range ?? {}) }, current.text.length) : new Set<string>()
  const laterApplied = current ? appliedCodes(passages, { unitId: current.unit_id, ...(range ?? {}) }, current.text.length).has(LATER_ID) : false

  // End of interview: scroll to the panel when it appears.
  const end = isEndOfInterview(counts)
  const wasEnd = useRef(end)
  useEffect(() => {
    if (end && !wasEnd.current) endRef.current?.scrollIntoView({ block: "nearest", behavior: "smooth" })
    wasEnd.current = end
  }, [end])

  const clearSelection = useCallback(() => {
    setPending(null)
    setMenu(null)
  }, [])

  const select = useCallback(
    (unitId: string) => {
      if (unitId !== selected) {
        setPending(null)
        setMenu(null)
        setPassageId(null)
      }
      setSelected(unitId)
    },
    [selected],
  )

  const move = useCallback(
    (dir: 1 | -1) => {
      const next = neighbour(shown, selected, dir)
      if (next) {
        select(next)
        requestAnimationFrame(() => document.querySelector(`[data-unit="${CSS.escape(next)}"]`)?.scrollIntoView({ block: "nearest" }))
      }
    },
    [shown, selected, select],
  )

  const requireCodable = useCallback(() => {
    if (!current) return false
    if (!isCodable(current)) {
      toast.info(t("coder.les_tours_de_l_intervieweur"))
      return false
    }
    return true
  }, [current])

  const applyCode = useCallback(
    async (c: (typeof codes)[number], keepSelection = false) => {
      if (!current || !requireCodable()) return
      const r = range
      if (!keepSelection) clearSelection()
      await actions.toggleCode(current, r, c)
    },
    [current, range, actions, clearSelection, requireCodable],
  )

  const doHighlight = useCallback(async () => {
    if (!current || !requireCodable()) return undefined
    if (!range) {
      toast.info(t("coder.selectionnez_d_abord_un_passage"))
      return undefined
    }
    const r = range
    clearSelection()
    return actions.highlight(current, r)
  }, [current, range, actions, clearSelection, requireCodable])

  const doUndo = useCallback(async () => {
    try {
      const e = await undo.undo()
      if (!e) {
        toast.info(t("coder.rien_a_annuler"))
        return
      }
      toast.success(t("coder.annule_label", { label: e.label }))
      await invalidatePassages(qc, id)
      await qc.invalidateQueries({ queryKey: keys.units(id) })
    } catch (err) {
      toast.error(err instanceof Error ? err.message : t("coder.l_annulation_a_echoue"))
    }
  }, [undo, qc, id])

  // R: the mark "à revoir" of the turn, apart from the code Plus tard.
  const toggleReview = useCallback(
    async (unitId: string) => {
      const key = keys.marks(id)
      const was = (qc.getQueryData<UnitMark[]>(key) ?? []).some((m) => m.unit_id === unitId)
      const set = async (on: boolean) => {
        qc.setQueryData<UnitMark[]>(key, (l = []) =>
          on ? [...l.filter((m) => m.unit_id !== unitId), { interview_id: id, unit_id: unitId, mark: "to_review", note: "", at: "", by: "" }] : l.filter((m) => m.unit_id !== unitId),
        )
        try {
          if (on) await api.markUnit(id, unitId)
          else await api.unmarkUnit(id, unitId)
        } catch (err) {
          toast.error(err instanceof Error ? err.message : t("coder.la_marque_n_a_pas"))
          throw err
        } finally {
          void qc.invalidateQueries({ queryKey: key })
          void qc.invalidateQueries({ queryKey: keys.libraryProgress })
        }
      }
      try {
        await set(!was)
        undo.push({ label: was ? t("common.a_revoir_retire") : t("coder.a_revoir_pose"), run: () => set(was) })
      } catch {
        // The toast says it; the cache is refetched.
      }
    },
    [qc, id, undo],
  )

  const openNewCode = useCallback(() => {
    if (!current || !requireCodable()) return
    setMenu(null)
    setNewCode({ unitId: current.unit_id, range, text: range ? current.text.slice(range.start, range.end) : current.text })
  }, [current, range, requireCodable])

  // Keyboard.
  const modalOpen = newCode !== null || codeSearchOpen || reviewOpen || correctOpen || menu !== null
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const action = resolveKey(e, { typing: isTypingTarget(e.target), modalOpen: modalOpen || !!document.querySelector('[role="dialog"], [role="alertdialog"], [data-state="open"][role="menu"]') })
      if (!action) return
      switch (action.type) {
        case "code": {
          const c = codeForNumber(rows, action.n)
          if (c) {
            e.preventDefault()
            void applyCode(c)
          }
          return
        }
        case "notRelevant":
          e.preventDefault()
          if (current && requireCodable()) void actions.toggleNotRelevant(current)
          return
        case "review":
          e.preventDefault()
          if (current && requireCodable()) {
            clearSelection()
            void toggleReview(current.unit_id)
          }
          return
        case "highlight":
          e.preventDefault()
          void doHighlight()
          return
        case "detail":
          e.preventDefault()
          chooseTab("detail")
          return
        case "next":
          e.preventDefault()
          move(1)
          return
        case "prev":
          e.preventDefault()
          move(-1)
          return
        case "undo":
          e.preventDefault()
          void doUndo()
          return
        case "codeSearch":
          if (current && requireCodable()) {
            e.preventDefault()
            setMenu(null)
            setCodeSearchOpen(true)
          }
          return
        case "search":
          e.preventDefault()
          chooseTab("codes")
          searchRef.current?.focus()
          return
        case "newCode":
          e.preventDefault()
          openNewCode()
          return
        case "escape":
          if (menu || pending) clearSelection()
          return
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [rows, applyCode, actions, current, range, requireCodable, clearSelection, doHighlight, move, doUndo, openNewCode, toggleReview, modalOpen, menu, pending])

  const saveNote = useMutation({
    mutationFn: (v: { p: Passage; note: string }) => api.addCoding(v.p.passage_id, LATER_ID, v.note),
    onSuccess: () => invalidatePassages(qc, id),
    onError: () => toast.error(t("coder.la_note_n_a_pas")),
  })

  if (unitsQ.isLoading || passagesQ.isLoading || codesQ.isLoading) return <Skeleton className="h-64 w-full" />
  if (unitsQ.error || passagesQ.error) {
    return <EmptyState title={t("common.l_entretien_n_a_pas")}>{(unitsQ.error ?? passagesQ.error)?.message}</EmptyState>
  }
  if (units.length === 0) {
    return (
      <>
        <PageTitle title={id} sub={t("common.codage")} />
        <EmptyState
          title={t("coder.cet_entretien_n_est_pas")}
          action={
            <Button asChild>
              <a href={interviewHref(id, "sources")}>{t("coder.aller_aux_sources_pour_segmenter")}</a>
            </Button>
          }
        >
          {t("coder.le_decoupage_en_tours_de")}
        </EmptyState>
      </>
    )
  }

  const nHighlights = passages.filter((p) => p.kind === "highlight" && !p.stale).length
  const nLater = counts.review
  const cmd = isMac() ? "⌘" : "Ctrl+"
  const total = counts.respondent
  const pct = total > 0 ? (counts.examined / total) * 100 : 0

  return (
    <AudioProvider interviewId={id}>
      <PageTitle title={id} sub={t("coder.codage_examined_sur_total_unites", { examined: counts.examined, total })} />
      <ToolbarActions>
        <StepsCapsule id={id} current="coder" highlights={nHighlights} />
        <AudioCapsule />
        <ParticipantExport id={id}/>
        <nav className="cap" aria-label={t("coder.texte_de_l_entretien")}>
          <button type="button" className="ci" onClick={() => setReviewOpen(true)} title={t("common.versions_du_texte_et_passages")}>
            {t("common.versions")}{nReview > 0 && <span className="n" aria-label={t("coder.n_passages_a_revoir", { n: nReview })}>{nReview}</span>}
          </button>
        </nav>
        <label className="search">
          <SearchIcon />
          <input type="search" value={search} placeholder={t("coder.rechercher_dans_l_entretien")} aria-label={t("coder.rechercher_dans_l_entretien")} onChange={(e) => setSearch(e.target.value)} />
        </label>
      </ToolbarActions>

      <div className="content">
        <section ref={sectionRef} className="tx" tabIndex={-1} aria-label={t("coder.transcription")} style={{ position: "relative" }}>
          <div className="fbar">
            <div className="seg" role="tablist" aria-label={t("coder.filtrer_les_unites")}>
              {FILTER_LABELS.map(({ key, label }) => (
                <button key={key} type="button" role="tab" aria-selected={filter === key} className={cn("s", filter === key && "on")} onClick={() => setFilter(key)}>
                  {label}
                  {key !== "all" && <span className="n">{key === "remaining" ? counts.remaining : key === "later" ? counts.review : key === "coded" ? counts.coded : counts.non_pertinent}</span>}
                </button>
              ))}
            </div>
            <div
              className="prog"
              title={t("coder.coded_codees_non_pertinent_non", { coded: counts.coded, non_pertinent: counts.non_pertinent, review: counts.review, remaining: counts.remaining })}
            >
              <span>
                {counts.examined} / {total}
              </span>
              <i className="bar" role="progressbar" aria-valuemin={0} aria-valuemax={total} aria-valuenow={counts.examined} aria-label={t("coder.unites_examinees")}>
                <b style={{ width: `${pct}%` }} />
              </i>
            </div>
          </div>

          {shown.length === 0 && (
            <p className="l2" style={{ padding: "12px 12px" }}>
              {t("coder.aucune_unite_dans_ce_filtre")}
            </p>
          )}
          {shown.map((u) => (
            <Turn
              key={u.unit_id}
              unit={u}
              passages={byUnit.get(u.unit_id) ?? []}
              selected={u.unit_id === selected}
              codeName={(cid, fb) => nameById.get(cid) ?? fb}
              marked={marked.has(u.unit_id)}
              pending={pending && pending.unitId === u.unit_id ? pending : null}
              currentPassage={passageId}
              onSelect={() => select(u.unit_id)}
              onRange={(r, rect) => {
                select(u.unit_id)
                setPending({ unitId: u.unit_id, ...r })
                const box = sectionRef.current?.getBoundingClientRect()
                if (box) setMenu({ x: Math.max(0, Math.min(rect.left - box.left, box.width - 280)), y: rect.bottom - box.top + 6 })
              }}
              onPassage={(p) => {
                setPassageId(p.passage_id)
                chooseTab("detail")
              }}
            />
          ))}
          {menu && pending && current && (
            <SelectionMenu
              codes={codes}
              keyOf={(cid) => keyMap.get(cid)}
              x={menu.x}
              y={menu.y}
              onPick={(c) => void applyCode(c, true)}
              onHighlight={() => void doHighlight()}
              onQuote={async () => {
                const p = await doHighlight()
                if (p) setPassageId(p.passage_id)
                chooseTab("detail")
              }}
              onNewCode={openNewCode}
              onClose={clearSelection}
            />
          )}
          {end && <EndPanel ref={endRef} interviewId={id} />}
        </section>

        <aside className="insp insp-sticky" style={{ flexBasis: 300 }} aria-label={t("coder.inspecteur")}>
          <div className="insp-in" style={{ paddingBottom: tab === "codes" ? 0 : undefined }}>
            <div className="seg" role="tablist" aria-label={t("coder.inspecteur")} style={{ alignSelf: "stretch", display: "flex" }}>
              {(
                [
                  ["codes", t("coder.codes")],
                  ["detail", t("coder.detail")],
                  ["lexique", t("coder.lexique")],
                  ["memo", t("coder.memo")],
                ] as const
              ).map(([key, label]) => (
                <button key={key} type="button" role="tab" aria-selected={tab === key} className={cn("s", tab === key && "on")} style={{ flex: 1 }} onClick={() => chooseTab(key)}>
                  {label}
                </button>
              ))}
            </div>
          </div>
          {tab === "codes" && (
            <>
              <CodeList
                rows={rows}
                keys={keyMap}
                applied={applied}
                query={query}
                matched={found.matched}
                paths={found.paths}
                onQuery={setQuery}
                searchRef={searchRef}
                onPick={(c) => { void applyCode(c); setQuery(""); searchRef.current?.focus() }}
                onPickLater={() => {
                  if (!current || !requireCodable()) return
                  const r = range
                  clearSelection()
                  void actions.toggleLater(current, r)
                }}
                laterApplied={laterApplied}
                onEnter={() => {
                  const c = firstApplicable(rows)
                  if (c) { void applyCode(c); setQuery(""); searchRef.current?.focus() }
                }}
              />
              <div className="insp-in" style={{ marginTop: "auto" }}>
                <Button variant="outline" style={{ width: "100%" }} disabled={!current} onClick={() => setCorrectOpen(true)}>
                  {t("coder.corriger_le_texte_de_cette")}
                </Button>
                <Button variant="outline" style={{ width: "100%" }} onClick={openNewCode} disabled={!current || !isCodable(current)}>
                  {range ? t("common.nouveau_code_depuis_la_selection") : t("coder.nouveau_code_depuis_cette_unite")}
                </Button>
                <div className="grp">
                  <div className="row">
                    <span className="k">{t("coder.unites_a_revoir")}</span>
                    <span className="v">{nLater}</span>
                  </div>
                  <a className="row" href={`#/interviews/${encodeURIComponent(id)}/highlights`} style={{ textDecoration: "none", color: "inherit" }}>
                    <span className="k">{t("coder.surlignages_sans_code")}</span>
                    <span className="v">{nHighlights}</span>
                  </a>
                </div>
              </div>
            </>
          )}
          {tab === "detail" && (
            <DetailPanel
              passage={detailPassage}
              unitLabel={current ? current.unit_id.replace(/^.*-(U\d+)$/, "$1") : null}
              unitPassages={currentPassages}
              actions={actions}
              onSelectPassage={(p) => setPassageId(p.passage_id)}
              onNote={(p, note) => saveNote.mutate({ p, note })}
            />
          )}
          {tab === "lexique" && <LexiconPassageTab passage={detailPassage ?? currentPassages.find((x) => x.kind === "highlight") ?? currentPassages[0] ?? null} />}
          {tab === "memo" && <MemoPanel interviewId={id} />}
        </aside>
      </div>

      <div className="keys" aria-label={t("common.raccourcis")} style={{ margin: "0 -1.25rem" }}>
        <span>
          <span className="kbd">↑ ↓</span> {t("common.unite")}
        </span>
        <span>
          <span className="kbd">C</span> {t("common.chercher_un_code")}
        </span>
        <span>
          <span className="kbd">1–9</span> {t("coder.coder")}
        </span>
        <span>
          <span className="kbd">N</span> {t("common.non_pertinent")}
        </span>
        <span>
          <span className="kbd">R</span> {t("common.a_revoir_2")}
        </span>
        <span>
          <span className="kbd">H</span> {t("coder.surligner")}
        </span>
        <span>
          <span className="kbd">D</span> {t("coder.detail_et_citation")}
        </span>
        <span>
          <span className="kbd">{cmd}Z</span> {t("coder.annuler")}
        </span>
        <span style={{ marginLeft: "auto" }}>{t("coder.sans_selection_le_code_couvre")}</span>
      </div>

      <Dialog open={codeSearchOpen} onOpenChange={setCodeSearchOpen}>
        <DialogContent aria-describedby={undefined} onCloseAutoFocus={(e) => {
          e.preventDefault()
          sectionRef.current?.focus({ preventScroll: true })
        }}>
          <DialogTitle>{t("common.chercher_un_code")}</DialogTitle>
          <CodeFinder codes={codes} applicableOnly autoFocus max={30} onPick={(c) => {
            void applyCode(c, true)
          }} />
        </DialogContent>
      </Dialog>
      <CorrectTextDialog interviewId={id} unit={current ?? null} open={correctOpen} onClose={() => setCorrectOpen(false)} onReview={() => setReviewOpen(true)} />
      <ReviewDialog interviewId={id} open={reviewOpen} onClose={() => setReviewOpen(false)} />
      <NewCodeDialog interviewId={id} target={newCode} codes={codes} undo={undo} onClose={() => setNewCode(null)} />
    </AudioProvider>
  )
}

