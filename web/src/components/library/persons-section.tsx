// "Personnes" of the Fiche of an interview: the persons and organisations
// linked to it, each with its role and, when anonymised, the pseudonym the
// model sees. A link is added from the register or removed here; the records
// themselves are edited in Personnes.

import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"

import { Alerts, NativeSelect } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { t } from "@/i18n"
import {
  ROLES,
  invalidateRegister,
  personsApi,
  roleLabel,
  useInterviewPersons,
  usePersons,
  type InterviewRole,
} from "@/lib/persons-api"
import { plainError } from "@/lib/persons-errors"
import { personHref } from "@/lib/router"

export function InterviewPersons({ interviewId }: { interviewId: string }) {
  const qc = useQueryClient()
  const linked = useInterviewPersons(interviewId)
  const everyone = usePersons({})
  const [pick, setPick] = useState("")
  const [role, setRole] = useState<InterviewRole>("respondent")
  const [error, setError] = useState<string | null>(null)

  const done = async () => {
    setError(null)
    await invalidateRegister(qc)
  }
  const add = useMutation({
    mutationFn: (v: { pid: string; role: InterviewRole }) => personsApi.linkToInterview(interviewId, v.pid, v.role),
    onSuccess: done,
    onError: (e) => setError(plainError(e)),
  })
  const remove = useMutation({
    mutationFn: (pid: string) => personsApi.unlinkFromInterview(interviewId, pid),
    onSuccess: done,
    onError: (e) => setError(plainError(e)),
  })

  const list = linked.data?.persons ?? []
  const taken = new Set(list.map((p) => p.person_id))
  const free = (everyone.data?.persons ?? []).filter((p) => !taken.has(p.person_id))

  return (
    <>
      <div className="hs">{t("persons.title")}</div>
      <div className="grp" role="group" aria-label={t("persons.of_interview", { id: interviewId })}>
        {list.length === 0 && (
          <div className="row">
            <span className="k l2">
              {t("persons.interview_none")}
              {(everyone.data?.n ?? 0) === 0 && (
                <small>
                  {t("persons.interview_register_empty")} <a href="#/personnes">{t("persons.title")}</a>.
                </small>
              )}
            </span>
          </div>
        )}
        {list.map((p) => (
          <div className="row" key={p.person_id} style={{ flexDirection: "column", alignItems: "stretch", gap: 4 }}>
            <div className="r8" style={{ flexWrap: "nowrap", justifyContent: "space-between" }}>
              <a href={personHref(p.person_id)} style={{ fontWeight: 600, overflowWrap: "anywhere" }}>
                {p.name}
              </a>
              <Button variant="ghost" aria-label={t("persons.remove_link", { name: p.name })} disabled={remove.isPending} onClick={() => remove.mutate(p.person_id)}>
                {t("common.retirer")}
              </Button>
            </div>
            <div className="r8" style={{ flexWrap: "nowrap" }}>
              <NativeSelect
                aria-label={t("persons.role_of", { name: p.name })}
                style={{ width: 124, flex: "none" }}
                value={p.role}
                disabled={add.isPending}
                onChange={(e) => add.mutate({ pid: p.person_id, role: e.target.value as InterviewRole })}
              >
                {ROLES.map((r) => (
                  <option key={r} value={r}>
                    {roleLabel(r)}
                  </option>
                ))}
              </NativeSelect>
              <span className="small l2">{p.anonymise ? t("persons.seen_as", { pseudonym: p.pseudonym }) : t("persons.not_anonymised")}</span>
            </div>
          </div>
        ))}
        {(everyone.data?.n ?? 0) > 0 && (
          <div className="row" style={{ flexDirection: "column", alignItems: "stretch", gap: 6 }}>
            <NativeSelect aria-label={t("persons.person_to_link")} value={pick} onChange={(e) => setPick(e.target.value)}>
              <option value="">{free.length === 0 ? t("persons.all_linked_here") : t("persons.choose_person")}</option>
              {free.map((p) => (
                <option key={p.person_id} value={p.person_id}>
                  {p.name}
                </option>
              ))}
            </NativeSelect>
            <div className="r8" style={{ flexWrap: "nowrap" }}>
              <NativeSelect aria-label={t("persons.role_to_give")} style={{ width: 124, flex: "none" }} value={role} onChange={(e) => setRole(e.target.value as InterviewRole)}>
                {ROLES.map((r) => (
                  <option key={r} value={r}>
                    {roleLabel(r)}
                  </option>
                ))}
              </NativeSelect>
              <Button
                variant="outline"
                disabled={!pick || add.isPending}
                onClick={() => {
                  add.mutate({ pid: pick, role })
                  setPick("")
                }}
              >
                {t("persons.link")}
              </Button>
            </div>
          </div>
        )}
        {error && (
          <div className="px-3 pb-2">
            <Alerts messages={[error]} />
          </div>
        )}
      </div>
    </>
  )
}
