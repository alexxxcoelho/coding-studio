// Personnes (Corpus, under Bibliothèque): the register of the people
// interviewed. Left, the tree of groups with its counts; in the middle, the
// persons and organisations of the selected group and its sub-groups, filtered
// by interview, anonymisation, kind and a search; right, the fiche of the
// selected record. The anonymised records feed the pseudonymisation list of the
// project (Paramètres de l'étude), which this screen never edits by hand.

import { useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { EmptyState, PageTitle } from "@/components/bits"
import { DeleteGroupDialog, GroupDialog } from "@/components/persons/group-dialogs"
import { GroupTree } from "@/components/persons/group-tree"
import { ImportDialog } from "@/components/persons/import-dialog"
import { NewPersonDialog } from "@/components/persons/new-person-dialog"
import { PersonSheet } from "@/components/persons/person-sheet"
import { NativeSelect } from "@/components/settings/fields"
import { Capsule, CapsuleItem, ToolbarActions, ToolbarSearch } from "@/components/toolbar"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { t } from "@/i18n"
import { api } from "@/lib/api"
import { reportError } from "@/lib/notify"
import {
  consentLabel,
  flattenGroups,
  invalidateRegister,
  kindLabel,
  personsApi,
  startKind,
  usePersonGroups,
  usePersons,
  type PersonFilters,
  type PersonKind,
  type PersonRow,
} from "@/lib/persons-api"
import { ALL, type GroupSelection } from "@/lib/persons-selection"
import { cn } from "@/lib/utils"

type Dialog = "new-person" | "new-group" | "edit-group" | "delete-group" | "import" | null

export function PersonsPage({ initialId = null }: { initialId?: string | null }) {
  const qc = useQueryClient()
  const tree = usePersonGroups()
  const interviews = useQuery({ queryKey: ["interviews"], queryFn: api.interviews })
  const [selection, setSelection] = useState<GroupSelection>(ALL)
  const [kind, setKind] = useState<PersonKind | "">("")
  const [interview, setInterview] = useState("")
  const [anonymise, setAnonymise] = useState<"" | "yes" | "no">("")
  const [search, setSearch] = useState("")
  const [picked, setPicked] = useState<string | null>(initialId)
  const [dialog, setDialog] = useState<Dialog>(null)

  const groups = useMemo(() => tree.data?.groups ?? [], [tree.data])
  const flat = useMemo(() => flattenGroups(groups), [groups])
  const selectedGroup = selection.kind === "group" ? (flat.find((g) => g.group_id === selection.id) ?? null) : null

  const filters: PersonFilters = {
    ...(kind ? { kind } : {}),
    ...(selectedGroup ? { group_id: selectedGroup.group_id } : {}),
    ...(interview ? { interview_id: interview } : {}),
    ...(anonymise ? { anonymise: anonymise === "yes" } : {}),
    ...(search.trim() ? { q: search.trim() } : {}),
  }
  const list = usePersons(filters)
  const everyone = usePersons({})
  const shown: PersonRow[] = (list.data?.persons ?? []).filter((p) => selection.kind !== "none" || p.group_ids.length === 0)
  const selectedId = picked && shown.some((p) => p.person_id === picked) ? picked : (shown[0]?.person_id ?? null)
  const nPersons = (everyone.data?.persons ?? []).filter((p) => p.kind === "person").length
  const nOrgs = (everyone.data?.persons ?? []).filter((p) => p.kind === "organisation").length
  const groupNames = new Map(flat.map((g) => [g.group_id, g.name]))

  const move = useMutation({
    mutationFn: (v: { id: string; parent: string | null }) => personsApi.updateGroup(v.id, { parent_id: v.parent }),
    onSuccess: () => invalidateRegister(qc),
    onError: (e) => reportError(e, t("persons.move_refused")),
  })

  const sub = everyone.data ? `${t("persons.n_people", { n: nPersons })}, ${t("persons.n_orgs", { n: nOrgs })}, ${t("persons.n_groups", { n: tree.data?.n_groups ?? 0 })}` : " "
  const filtered = kind !== "" || interview !== "" || anonymise !== "" || search.trim() !== "" || selection.kind !== "all"

  return (
    <>
      <PageTitle title={t("persons.title")} sub={sub} />
      <ToolbarActions>
        <Capsule>
          <CapsuleItem onClick={() => setDialog("new-person")}>{t("persons.add_person")}</CapsuleItem>
          <span className="vs" />
          <CapsuleItem onClick={() => setDialog("new-group")}>{t("persons.add_group")}</CapsuleItem>
        </Capsule>
        <Capsule>
          <CapsuleItem onClick={() => setDialog("import")}>{t("persons.import_csv")}</CapsuleItem>
        </Capsule>
        <ToolbarSearch value={search} onChange={setSearch} placeholder={t("persons.filter_placeholder")} />
      </ToolbarActions>

      <div className="content">
        <section aria-label={t("persons.groups")} style={{ flex: "0 1 270px", minWidth: 210, padding: "4px 10px 16px 18px", display: "flex", flexDirection: "column", gap: 8 }}>
          {tree.isLoading ? (
            <Skeleton className="h-48 w-full" />
          ) : (
            <GroupTree
              groups={groups}
              nPersons={tree.data?.n_persons ?? 0}
              nUngrouped={tree.data?.n_ungrouped_persons ?? 0}
              selection={selection}
              onSelect={setSelection}
              onMove={(id, parent) => move.mutate({ id, parent })}
            />
          )}
          {selectedGroup && (
            <div className="flex flex-col gap-1.5" role="group" aria-label={t("persons.selected_group", { name: selectedGroup.name })}>
              {selectedGroup.description && <p className="small l2 m-0">{selectedGroup.description}</p>}
              <div className="r8">
                <Button variant="outline" onClick={() => setDialog("edit-group")}>
                  {t("persons.edit_group_button")}
                </Button>
                <Button variant="outline" onClick={() => setDialog("new-group")}>
                  {t("persons.add_subgroup")}
                </Button>
                <Button variant="ghost" onClick={() => setDialog("delete-group")}>
                  {t("common.supprimer")}
                </Button>
              </div>
            </div>
          )}
          <p className="small l2 m-0">{t("persons.tree_help")}</p>
        </section>

        <section aria-label={t("persons.list")} style={{ flex: "999 1 360px", minWidth: 0, padding: "4px 18px 16px", overflowX: "auto" }}>
          <div className="r8" style={{ marginBottom: 8 }}>
            <NativeSelect aria-label={t("persons.filter_kind")} style={{ width: 140 }} value={kind} onChange={(e) => setKind(e.target.value as PersonKind | "")}>
              <option value="">{t("persons.all_kinds")}</option>
              <option value="person">{t("persons.kind_persons")}</option>
              <option value="organisation">{t("persons.kind_organisations")}</option>
            </NativeSelect>
            <NativeSelect aria-label={t("persons.filter_interview")} style={{ width: 150 }} value={interview} onChange={(e) => setInterview(e.target.value)}>
              <option value="">{t("persons.all_interviews")}</option>
              {(interviews.data ?? []).map((s) => (
                <option key={s.interview_id} value={s.interview_id}>
                  {s.interview_id}
                </option>
              ))}
            </NativeSelect>
            <NativeSelect aria-label={t("persons.filter_anonymise")} style={{ width: 170 }} value={anonymise} onChange={(e) => setAnonymise(e.target.value as "" | "yes" | "no")}>
              <option value="">{t("persons.all_anonymisation")}</option>
              <option value="yes">{t("persons.only_anonymised")}</option>
              <option value="no">{t("persons.only_not_anonymised")}</option>
            </NativeSelect>
            {filtered && (
              <Button
                variant="ghost"
                onClick={() => {
                  setKind("")
                  setInterview("")
                  setAnonymise("")
                  setSearch("")
                  setSelection(ALL)
                }}
              >
                {t("persons.reset_filters")}
              </Button>
            )}
          </div>

          {list.isLoading ? (
            <div className="flex flex-col gap-2">
              {[0, 1, 2, 3].map((i) => (
                <Skeleton key={i} className="h-12 w-full" />
              ))}
            </div>
          ) : (everyone.data?.n ?? 0) === 0 ? (
            <EmptyState
              title={t("persons.empty_title")}
              action={
                <div className="r8">
                  <Button onClick={() => setDialog("new-person")}>{t("persons.add_person")}</Button>
                  <Button variant="outline" onClick={() => setDialog("import")}>
                    {t("persons.import_csv")}
                  </Button>
                </div>
              }
            >
              {t("persons.empty_text")}
            </EmptyState>
          ) : shown.length === 0 ? (
            <EmptyState title={t("persons.none_match")}>{t("persons.none_match_text")}</EmptyState>
          ) : (
            <table className="tbl" aria-label={t("persons.list")}>
              <thead>
                <tr>
                  <th>{t("common.nom")}</th>
                  <th>{t("persons.pseudonym_column")}</th>
                  <th style={{ textAlign: "right" }}>{t("common.entretiens")}</th>
                  <th>{t("persons.consent")}</th>
                </tr>
              </thead>
              <tbody>
                {shown.map((p) => {
                  const on = p.person_id === selectedId
                  const context = [p.kind === "organisation" ? kindLabel(p.kind) : p.organisation, p.function, ...p.group_ids.map((g) => groupNames.get(g) ?? g)].filter(Boolean).join(" · ")
                  return (
                    <tr
                      key={p.person_id}
                      className={cn("row-click", on && "sel")}
                      data-state={on ? "selected" : undefined}
                      aria-selected={on}
                      tabIndex={0}
                      onClick={() => setPicked(p.person_id)}
                      onKeyDown={(e) => e.key === "Enter" && setPicked(p.person_id)}
                    >
                      <td>
                        <div style={{ fontWeight: 600 }}>{p.name}</div>
                        <div className="small l2" style={{ overflowWrap: "anywhere" }}>
                          <span className="mono">{p.person_id}</span>
                          {context && ` · ${context}`}
                        </div>
                      </td>
                      <td>
                        {p.anonymise ? (
                          <span title={t("persons.replaced_by")}>{p.pseudonym}</span>
                        ) : (
                          <span className="l2">{t("persons.not_anonymised")}</span>
                        )}
                      </td>
                      <td style={{ textAlign: "right" }}>{p.n_interviews}</td>
                      <td className="l2">{p.consent_status ? consentLabel(p.consent_status) : t("persons.consent_none")}</td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          )}
          {list.data && shown.length > 0 && (
            <p className="small l2" style={{ margin: "10px 10px 0" }}>
              {t("persons.n_shown", { n: shown.length })}
            </p>
          )}
        </section>

        {selectedId && (
          <aside className="insp insp-sticky" style={{ flexBasis: 340 }} aria-label={t("persons.fiche")}>
            <PersonSheet key={selectedId} pid={selectedId} tree={groups} onDeleted={() => setPicked(null)} />
          </aside>
        )}
      </div>

      {dialog === "new-person" && (
        <NewPersonDialog
          open
          onClose={() => setDialog(null)}
          tree={groups}
          defaultGroup={selectedGroup?.group_id ?? null}
          defaultKind={startKind(kind)}
          onCreated={(id) => {
            setPicked(id)
            setKind("")
            setSearch("")
            setInterview("")
            setAnonymise("")
          }}
        />
      )}
      {(dialog === "new-group" || (dialog === "edit-group" && selectedGroup)) && (
        <GroupDialog
          key={dialog === "edit-group" ? selectedGroup?.group_id : "new"}
          open
          onClose={() => setDialog(null)}
          tree={groups}
          group={dialog === "edit-group" ? selectedGroup : null}
          defaultParent={dialog === "new-group" ? (selectedGroup?.group_id ?? null) : null}
          onSaved={(id) => setSelection({ kind: "group", id })}
        />
      )}
      {dialog === "delete-group" && selectedGroup && (
        <DeleteGroupDialog open onClose={() => setDialog(null)} tree={groups} group={selectedGroup} onDeleted={() => setSelection(ALL)} />
      )}
      {dialog === "import" && <ImportDialog open onClose={() => setDialog(null)} />}
    </>
  )
}
