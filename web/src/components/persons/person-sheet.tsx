// The fiche of a person or an organisation (right of Personnes): identity,
// consent, attributes of the register, the "Anonymiser" switch with its
// pseudonym and aliases, groups and interviews with the role. Each field is
// saved when it is left (the same gesture as the attributes of an interview);
// a refusal of the server is explained in plain words under the title.

import { useState, type ReactNode } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { AttributeRow } from "@/components/library/fiche"
import { Alerts, Chip, NativeSelect, TEXTAREA } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { t } from "@/i18n"
import { api } from "@/lib/api"
import { validDate } from "@/lib/attributes"
import {
  CONSENT_STATUSES,
  ROLES,
  consentLabel,
  flattenGroups,
  invalidateRegister,
  kindLabel,
  personsApi,
  roleLabel,
  usePersonDetail,
  type GroupNode,
  type InterviewRole,
  type PersonBody,
  type PersonDetail,
} from "@/lib/persons-api"
import { plainError } from "@/lib/persons-errors"
import { interviewHref } from "@/lib/router"

/** A text field saved when it is left, or on Enter. A new saved value replaces the draft. */
function DraftField({
  id,
  label,
  value,
  onCommit,
  multiline,
  type,
  placeholder,
  check,
  hint,
  width = 168,
}: {
  id: string
  label: string
  value: string
  onCommit: (v: string) => void
  multiline?: boolean
  type?: string
  placeholder?: string
  /** A reason to refuse the value before the call, or null. */
  check?: (v: string) => string | null
  hint?: string
  width?: number
}) {
  const [draft, setDraft] = useState(value)
  const [seen, setSeen] = useState(value)
  const [error, setError] = useState<string | null>(null)
  if (seen !== value) {
    setSeen(value)
    setDraft(value)
    setError(null)
  }
  const commit = () => {
    const v = draft.trim()
    if (v === value) return
    const reason = check?.(v) ?? null
    setError(reason)
    if (!reason) onCommit(v)
  }
  if (multiline) {
    return (
      <div className="flex flex-col gap-1 px-3 py-2" style={{ borderTop: ".5px solid var(--sep)" }}>
        <label className="k" htmlFor={id}>
          {label}
        </label>
        <textarea id={id} className={TEXTAREA} rows={3} value={draft} onChange={(e) => setDraft(e.target.value)} onBlur={commit} />
      </div>
    )
  }
  return (
    <div className="row" style={{ flexWrap: "wrap" }}>
      <label className="k" htmlFor={id}>
        {label}
        {hint && <small>{hint}</small>}
      </label>
      <Input
        id={id}
        style={{ width }}
        type={type ?? "text"}
        placeholder={placeholder}
        value={draft}
        aria-invalid={!!error}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => e.key === "Enter" && commit()}
      />
      {error && (
        <p role="alert" className="small basis-full text-right" style={{ color: "var(--red-text)", margin: 0 }}>
          {error}
        </p>
      )}
    </div>
  )
}

function Heading({ children }: { children: ReactNode }) {
  return <div className="hs">{children}</div>
}

export function PersonSheet({ pid, tree, onDeleted }: { pid: string; tree: GroupNode[]; onDeleted: () => void }) {
  const detail = usePersonDetail(pid)
  if (detail.isLoading) return <Skeleton className="m-3 h-64" />
  if (detail.error || !detail.data) return <Alerts messages={[detail.error ? plainError(detail.error) : t("persons.not_found")]} />
  return <Sheet key={detail.data.person_id} p={detail.data} tree={tree} onDeleted={onDeleted} />
}

function Sheet({ p, tree, onDeleted }: { p: PersonDetail; tree: GroupNode[]; onDeleted: () => void }) {
  const qc = useQueryClient()
  const [error, setError] = useState<string | null>(null)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const isPerson = p.kind === "person"

  const save = useMutation({
    mutationFn: (body: PersonBody) => personsApi.update(p.person_id, body),
    onSuccess: async () => {
      setError(null)
      await invalidateRegister(qc)
    },
    onError: (e) => setError(plainError(e)),
  })
  const patch = (body: PersonBody) => save.mutate(body)

  return (
    <div className="insp-in">
      <div className="r8" style={{ justifyContent: "space-between", flexWrap: "nowrap" }}>
        <span className="b mono" style={{ fontSize: 15, fontWeight: 600 }}>
          {p.person_id}
        </span>
        <span className="r8" style={{ flexWrap: "nowrap" }}>
          <span className="badge">{kindLabel(p.kind)}</span>
          <Button variant="ghost" onClick={() => setConfirmDelete(true)}>
            {t("common.supprimer")}
          </Button>
        </span>
      </div>

      <Input
        key={p.name}
        aria-label={t("common.nom")}
        defaultValue={p.name}
        style={{ height: 28, fontSize: 15, fontWeight: 600 }}
        onBlur={(e) => e.target.value.trim() !== p.name && patch({ name: e.target.value })}
        onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
      />
      {error && <Alerts messages={[error]} />}

      <Heading>{t("persons.identity")}</Heading>
      <div className="grp">
        {isPerson && <DraftField id="p-org" label={t("common.organisation")} value={p.organisation} onCommit={(v) => patch({ organisation: v })} />}
        <DraftField id="p-function" label={isPerson ? t("persons.function") : t("persons.activity")} value={p.function} onCommit={(v) => patch({ function: v })} />
        <DraftField id="p-email" label={t("persons.email")} type="email" value={p.email} onCommit={(v) => patch({ email: v })} />
        <DraftField id="p-phone" label={t("persons.phone")} value={p.phone} onCommit={(v) => patch({ phone: v })} />
        <DraftField id="p-notes" label={t("persons.notes")} multiline value={p.notes} onCommit={(v) => patch({ notes: v })} />
      </div>

      <Heading>{t("persons.consent")}</Heading>
      <div className="grp">
        <div className="row">
          <label className="k" htmlFor="p-consent">
            {t("persons.consent_status")}
          </label>
          <NativeSelect id="p-consent" style={{ width: 168 }} value={p.consent_status} onChange={(e) => patch({ consent_status: e.target.value as PersonBody["consent_status"] })}>
            {CONSENT_STATUSES.map((s) => (
              <option key={s} value={s}>
                {s === "" ? t("persons.consent_none") : consentLabel(s)}
              </option>
            ))}
          </NativeSelect>
        </div>
        <DraftField
          id="p-consent-date"
          label={t("persons.consent_date")}
          placeholder={t("persons.date_format")}
          value={p.consent_date}
          check={(v) => (v === "" || validDate(v) ? null : t("persons.err_date"))}
          onCommit={(v) => patch({ consent_date: v })}
        />
        <DraftField
          id="p-consent-doc"
          label={t("persons.consent_document")}
          hint={t("persons.consent_document_hint")}
          value={p.consent_document}
          onCommit={(v) => patch({ consent_document: v })}
        />
      </div>

      <Heading>{t("persons.attributes")}</Heading>
      <div className="grp">
        {p.attribute_definitions.length === 0 ? (
          <div className="row">
            <span className="k l2">
              {t("persons.no_attribute")}
              <small>
                {t("persons.define_attributes_in")} <a href="#/settings">{t("common.parametres_de_l_etude")}</a>.
              </small>
            </span>
          </div>
        ) : (
          p.attribute_definitions.map((d) => <AttributeRow key={d.id} def={d} saved={p.attributes[d.id] ?? ""} onSave={(value) => patch({ attributes: { [d.id]: value } })} />)
        )}
      </div>

      <Heading>{t("persons.anonymisation")}</Heading>
      <Anonymisation p={p} patch={patch} pending={save.isPending} />

      <Heading>{t("persons.groups")}</Heading>
      <Groups p={p} tree={tree} patch={patch} />

      <Heading>{t("common.entretiens")}</Heading>
      <Interviews p={p} />

      {confirmDelete && <DeletePersonDialog p={p} onClose={() => setConfirmDelete(false)} onDeleted={onDeleted} />}
    </div>
  )
}

function Anonymisation({ p, patch, pending }: { p: PersonDetail; patch: (b: PersonBody) => void; pending: boolean }) {
  const [alias, setAlias] = useState("")
  const addAlias = () => {
    const a = alias.trim()
    if (!a) return
    setAlias("")
    if (!p.aliases.some((x) => x.toLowerCase() === a.toLowerCase())) patch({ aliases: [...p.aliases, a] })
  }
  return (
    <>
      <div className="grp">
        <div className="row">
          <span className="k">
            <label id="p-anon-label">{t("persons.anonymise")}</label>
            <small>{p.anonymise ? t("persons.anonymise_on", { pseudonym: p.pseudonym, n: p.aliases.length }) : t("persons.anonymise_off")}</small>
          </span>
          <button type="button" role="switch" className="switch" aria-checked={p.anonymise} aria-labelledby="p-anon-label" disabled={pending} onClick={() => patch({ anonymise: !p.anonymise })} />
        </div>
        <DraftField
          id="p-pseudonym"
          label={t("persons.pseudonym")}
          hint={t("persons.pseudonym_hint")}
          value={p.pseudonym}
          onCommit={(v) => patch({ pseudonym: v })}
        />
        <div className="flex flex-col gap-1.5 px-3 py-2" style={{ borderTop: ".5px solid var(--sep)" }}>
          <label className="k" htmlFor="p-alias">
            {t("persons.aliases")}
            <small className="l2" style={{ display: "block", fontSize: 11.5 }}>
              {t("persons.aliases_hint")}
            </small>
          </label>
          {p.aliases.length > 0 && (
            <span className="inline-flex flex-wrap gap-1" role="list" aria-label={t("persons.aliases")}>
              {p.aliases.map((a) => (
                <span key={a} role="listitem">
                  <Chip value={a} removeLabel={t("persons.remove_alias", { alias: a })} onRemove={() => patch({ aliases: p.aliases.filter((x) => x !== a) })} />
                </span>
              ))}
            </span>
          )}
          <form
            className="r8"
            style={{ flexWrap: "nowrap" }}
            onSubmit={(e) => {
              e.preventDefault()
              addAlias()
            }}
          >
            <Input id="p-alias" value={alias} placeholder={t("persons.add_alias_placeholder")} onChange={(e) => setAlias(e.target.value)} />
            <Button type="submit" variant="outline" disabled={!alias.trim()}>
              {t("common.ajouter")}
            </Button>
          </form>
        </div>
      </div>
    </>
  )
}

function Groups({ p, tree, patch }: { p: PersonDetail; tree: GroupNode[]; patch: (b: PersonBody) => void }) {
  const flat = flattenGroups(tree)
  const free = flat.filter((g) => !p.group_ids.includes(g.group_id))
  const path = (id: string) => flat.find((g) => g.group_id === id)?.path ?? id
  return (
    <div className="grp">
      <div className="flex flex-col gap-1.5 px-3 py-2">
        {p.group_ids.length === 0 ? (
          <span className="l2 small">{t("persons.no_group_yet")}</span>
        ) : (
          <span className="inline-flex flex-wrap gap-1" role="list" aria-label={t("persons.groups")}>
            {p.group_ids.map((id) => (
              <span key={id} role="listitem">
                <Chip value={path(id)} removeLabel={t("persons.leave_group", { name: path(id) })} onRemove={() => patch({ group_ids: p.group_ids.filter((x) => x !== id) })} />
              </span>
            ))}
          </span>
        )}
        <NativeSelect
          aria-label={t("persons.add_to_group")}
          value=""
          disabled={free.length === 0}
          onChange={(e) => e.target.value && patch({ group_ids: [...p.group_ids, e.target.value] })}
        >
          <option value="">{flat.length === 0 ? t("persons.no_group_exists") : t("persons.add_to_group")}</option>
          {free.map((g) => (
            <option key={g.group_id} value={g.group_id}>
              {g.path}
            </option>
          ))}
        </NativeSelect>
      </div>
    </div>
  )
}

function Interviews({ p }: { p: PersonDetail }) {
  const qc = useQueryClient()
  const all = useQuery({ queryKey: ["interviews"], queryFn: api.interviews })
  const [pick, setPick] = useState("")
  const [role, setRole] = useState<InterviewRole>("respondent")
  const [error, setError] = useState<string | null>(null)
  const done = async () => {
    setError(null)
    await invalidateRegister(qc)
  }
  const link = useMutation({
    mutationFn: (v: { id: string; role: InterviewRole }) => personsApi.link(p.person_id, v.id, v.role),
    onSuccess: done,
    onError: (e) => setError(plainError(e)),
  })
  const unlink = useMutation({
    mutationFn: (id: string) => personsApi.unlink(p.person_id, id),
    onSuccess: done,
    onError: (e) => setError(plainError(e)),
  })
  const linked = new Set(p.interviews.map((i) => i.interview_id))
  const free = (all.data ?? []).filter((s) => !linked.has(s.interview_id))
  return (
    <div className="grp">
      {p.interviews.length === 0 && (
        <div className="row">
          <span className="k l2">{t("persons.no_interview")}</span>
        </div>
      )}
      {p.interviews.map((i) => (
        <div className="row" key={i.interview_id} style={{ flexDirection: "column", alignItems: "stretch", gap: 4 }}>
          <div className="r8" style={{ flexWrap: "nowrap", justifyContent: "space-between" }}>
            <span className="k">
              <a href={interviewHref(i.interview_id, "sources")} className="mono" style={{ fontWeight: 600 }} aria-label={t("persons.open_interview", { id: i.interview_id })}>
                {i.interview_id}
              </a>
              <small style={{ overflowWrap: "anywhere" }}>{i.folder_name}</small>
            </span>
            <Button variant="ghost" aria-label={t("persons.unlink_interview", { id: i.interview_id })} disabled={unlink.isPending} onClick={() => unlink.mutate(i.interview_id)}>
              {t("common.retirer")}
            </Button>
          </div>
          <NativeSelect
            aria-label={t("persons.role_in", { id: i.interview_id })}
            style={{ width: 150 }}
            value={i.role}
            disabled={link.isPending}
            onChange={(e) => link.mutate({ id: i.interview_id, role: e.target.value as InterviewRole })}
          >
            {ROLES.map((r) => (
              <option key={r} value={r}>
                {roleLabel(r)}
              </option>
            ))}
          </NativeSelect>
        </div>
      ))}
      <div className="row" style={{ flexDirection: "column", alignItems: "stretch", gap: 6 }}>
        <NativeSelect aria-label={t("persons.interview_to_link")} value={pick} onChange={(e) => setPick(e.target.value)}>
          <option value="">{free.length === 0 && (all.data ?? []).length > 0 ? t("persons.all_linked") : t("persons.choose_interview")}</option>
          {free.map((s) => (
            <option key={s.interview_id} value={s.interview_id}>
              {s.interview_id}
            </option>
          ))}
        </NativeSelect>
        <div className="r8" style={{ flexWrap: "nowrap" }}>
          <NativeSelect aria-label={t("persons.role_to_give")} style={{ width: 150 }} value={role} onChange={(e) => setRole(e.target.value as InterviewRole)}>
            {ROLES.map((r) => (
              <option key={r} value={r}>
                {roleLabel(r)}
              </option>
            ))}
          </NativeSelect>
          <Button
            variant="outline"
            disabled={!pick || link.isPending}
            onClick={() => {
              link.mutate({ id: pick, role })
              setPick("")
            }}
          >
            {t("persons.link")}
          </Button>
        </div>
      </div>
      {error && (
        <div className="px-3 pb-2">
          <Alerts messages={[error]} />
        </div>
      )}
    </div>
  )
}

function DeletePersonDialog({ p, onClose, onDeleted }: { p: PersonDetail; onClose: () => void; onDeleted: () => void }) {
  const qc = useQueryClient()
  const [error, setError] = useState<string | null>(null)
  const n = p.interviews.length
  const remove = useMutation({
    mutationFn: () => personsApi.remove(p.person_id, n > 0),
    onSuccess: async () => {
      await invalidateRegister(qc)
      toast.success(t("persons.deleted", { id: p.person_id }))
      onDeleted()
      onClose()
    },
    onError: (e) => setError(plainError(e)),
  })
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("persons.delete_title", { name: p.name })}</DialogTitle>
          <DialogDescription>{n > 0 ? t("persons.delete_with_links", { n }) : t("persons.delete_plain")}</DialogDescription>
        </DialogHeader>
        <p className="small l2">{t("persons.delete_note")}</p>
        {error && <Alerts messages={[error]} />}
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t("common.annuler")}
          </Button>
          <Button variant="destructive" disabled={remove.isPending} onClick={() => remove.mutate()}>
            {t("persons.delete_person")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
