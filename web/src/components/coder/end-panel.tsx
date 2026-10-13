// "Fin d'entretien": the study checklist, shown when every respondent turn is
// examined. The item `end_memo` carries the note of the end-of-interview memo.
// The list is never a lock (docs/guide/fr/02_coder.md, action 11).

import { forwardRef, useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"

import { TEXTAREA } from "@/components/settings/fields"
import { t } from "@/i18n"
import { api } from "@/lib/api"
import { checklistMissing, END_MEMO_ITEM } from "@/lib/coder-model"
import { keys, useChecklist } from "@/lib/hooks"
import { reportError } from "@/lib/notify"
import { interviewHref } from "@/lib/router"

export const EndPanel = forwardRef<HTMLElement, { interviewId: string }>(function EndPanel({ interviewId }, ref) {
  const qc = useQueryClient()
  const list = useChecklist(interviewId)
  const savedNote = list.data?.note ?? ""
  const [note, setNote] = useState(savedNote)
  const [seenNote, setSeenNote] = useState(savedNote)
  if (seenNote !== savedNote) {
    setSeenNote(savedNote)
    setNote(savedNote)
  }

  const save = useMutation({
    mutationFn: (vars: { checked?: Record<string, boolean>; note?: string }) => api.setChecklist(interviewId, vars.checked ?? {}, vars.note),
    onSuccess: (v) => {
      qc.setQueryData(keys.checklist(interviewId), v)
      qc.invalidateQueries({ queryKey: ["chain"] })
    },
    onError: (e) => reportError(e, t("coder.end_panel.liste_non_enregistree")),
  })

  const data = list.data
  const done = data ? data.items.filter((i) => i.checked).length : 0
  return (
    <section ref={ref} aria-labelledby="end-title" className="grp" style={{ margin: "14px 0 4px", padding: "14px 16px" }}>
      <h2 id="end-title" style={{ fontSize: 14, fontWeight: 600, margin: 0 }}>
        {t("common.fin_d_entretien")}
      </h2>
      <p className="small l2" style={{ margin: "2px 0 10px" }}>
        {t("coder.end_panel.tous_les_tours_du_repondant")}
      </p>
      {!data ? (
        <p className="small l2">{t("coder.end_panel.chargement")}</p>
      ) : (
        <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: 8 }}>
          {data.items.map((item) => (
            <li key={item.id}>
              <label className="chk" style={{ display: "flex", gap: 8, alignItems: "flex-start" }}>
                <input
                  type="checkbox"
                  style={{ marginTop: 3, accentColor: "var(--accent-fill)" }}
                  checked={item.checked}
                  onChange={(e) => save.mutate({ checked: { [item.id]: e.target.checked } })}
                />
                <span>{item.label}</span>
              </label>
              {item.id === END_MEMO_ITEM && (
                <div style={{ marginLeft: 22, marginTop: 6 }}>
                  <label className="hs" htmlFor="end-note">
                    {t("common.memo_de_fin_d_entretien")}
                  </label>
                  <textarea
                    id="end-note"
                    className={TEXTAREA}
                    rows={4}
                    value={note}
                    placeholder={t("coder.end_panel.position_du_repondant_mecanisme_cent")}
                    onChange={(e) => setNote(e.target.value)}
                    onBlur={() => note !== (data.note ?? "") && save.mutate({ note })}
                  />
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
      {data && (
        <p className="small l2" style={{ margin: "10px 0 0" }} role="status">
          {t("coder.end_panel.done_sur_length_points_coches", { done, length: data.items.length })}
          {done < data.items.length && t("coder.end_panel.a_voir_join", { join: checklistMissing(data.items).join(" ; ") })}{t("coder.end_panel.la_liste_n_est_jamais")}{" "}
          <a href={interviewHref(interviewId, "review")}>{t("coder.end_panel.passer_a_revoir")}</a>
        </p>
      )}
    </section>
  )
})
