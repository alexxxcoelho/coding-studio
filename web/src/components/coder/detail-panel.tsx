// The Détail tab: the selected passage (its text, its codes, its note, the
// text around it) and what can be done with it: remove a code, write a note
// for Plus tard, copy it as a quotation, delete it.

import { useState, type CSSProperties } from "react"
import { X } from "lucide-react"
import { toast } from "sonner"

import { shortId } from "@/components/coder/turn"
import { TEXTAREA } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { codeColor, LATER_ID } from "@/lib/code-tree"
import { dateTime } from "@/lib/format"
import type { CodingActions } from "@/lib/hooks"
import type { Passage } from "@/lib/types"

export function quotation(p: Passage): string {
  return `« ${p.text.trim()} » (${shortId(p.unit_id)}, ${p.interview_id})`
}

function LaterNote({ passage, onSave }: { passage: Passage; onSave: (note: string) => void }) {
  const coding = passage.codings.find((c) => c.system || c.code_id === LATER_ID)
  const savedNote = coding?.note ?? ""
  const [draft, setDraft] = useState(savedNote)
  const [seen, setSeen] = useState(`${passage.passage_id}:${savedNote}`)
  if (seen !== `${passage.passage_id}:${savedNote}`) {
    setSeen(`${passage.passage_id}:${savedNote}`)
    setDraft(savedNote)
  }
  if (!coding) return null
  return (
    <>
      <label className="hs" htmlFor="later-note">
        Note pour Plus tard
      </label>
      <textarea
        id="later-note"
        className={TEXTAREA}
        rows={2}
        value={draft}
        placeholder="Pourquoi ce passage est à revoir"
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => draft !== coding.note && onSave(draft)}
      />
    </>
  )
}

export function DetailPanel({
  passage,
  unitLabel,
  unitPassages,
  actions,
  onSelectPassage,
  onNote,
}: {
  passage: Passage | null
  unitLabel: string | null
  unitPassages: Passage[]
  actions: CodingActions
  onSelectPassage: (p: Passage) => void
  onNote: (p: Passage, note: string) => void
}) {
  if (!passage) {
    return (
      <div className="insp-in">
        <div className="hs">Passage sélectionné</div>
        <p className="small l2" style={{ margin: 0 }}>
          {unitLabel
            ? `Cliquez sur un passage codé ou surligné de ${unitLabel} pour voir son détail.`
            : "Sélectionnez un tour, puis un passage."}
        </p>
        {unitPassages.length > 0 && (
          <div className="grp">
            {unitPassages.map((p) => (
              <button key={p.passage_id} type="button" className="row" style={{ width: "100%", textAlign: "left", background: "transparent", border: 0, font: "inherit", color: "inherit", cursor: "pointer" }} onClick={() => onSelectPassage(p)}>
                <span className="k">
                  {p.text.length > 60 ? `${p.text.slice(0, 59)}…` : p.text}
                  <small>{p.codings.length ? p.codings.map((c) => c.code_name).join(", ") : "surlignage"}</small>
                </span>
              </button>
            ))}
          </div>
        )}
      </div>
    )
  }
  return (
    <div className="insp-in">
      <div className="hs">Passage sélectionné</div>
      <div className="small l2">
        <span className="mono">{shortId(passage.unit_id)}</span>
        {passage.whole_unit ? ", tour entier" : `, caractères ${passage.start} à ${passage.end}`}
      </div>
      {passage.stale && (
        <p className="note-warn" role="note">
          Le texte du tour a changé depuis la sélection ({passage.stale_reason ?? "ancrage obsolète"}). Le passage est signalé, il n'est pas déplacé.
        </p>
      )}
      <blockquote style={{ margin: 0, borderLeft: "3px solid var(--accent-c)", paddingLeft: 10, fontSize: 13, lineHeight: 1.5 }}>
        {passage.before && <span className="l3">{passage.before}</span>}
        <mark style={{ background: "color-mix(in srgb, var(--yellow) 45%, transparent)", color: "inherit" }}>{passage.text}</mark>
        {passage.after && <span className="l3">{passage.after}</span>}
      </blockquote>

      <div className="hs">Codes</div>
      <div className="r8" style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
        {passage.codings.length === 0 && <span className="small l2">Surlignage sans code</span>}
        {passage.codings.map((c) => (
          <span key={c.coding_id} className="tag k" style={{ "--k": codeColor(c.code_id) } as CSSProperties}>
            <span className="sw" />
            {c.code_name}
            <button
              type="button"
              aria-label={`Retirer ${c.code_name}`}
              title="Retirer ce code"
              style={{ border: 0, background: "transparent", padding: 0, cursor: "pointer", color: "var(--label2)", display: "inline-flex" }}
              onClick={() => actions.removeCode(passage, c.code_id, c.code_name)}
            >
              <X className="size-3" aria-hidden />
            </button>
          </span>
        ))}
      </div>

      <LaterNote passage={passage} onSave={(note) => onNote(passage, note)} />

      <div className="hs">Citation</div>
      <div className="r8" style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
        <Button
          variant="outline"
          onClick={async () => {
            try {
              await navigator.clipboard.writeText(quotation(passage))
              toast.success("Citation copiée")
            } catch {
              toast.error("La citation n'a pas pu être copiée")
            }
          }}
        >
          Copier la citation
        </Button>
        <Button variant="outline" onClick={() => actions.deletePassage(passage)}>
          Supprimer le passage
        </Button>
      </div>
      <p className="small l2" style={{ margin: 0 }}>
        Créé par {passage.created_by || "…"} le {dateTime(passage.created_at) || "…"}. Un surlignage reste visible après le codage et sert de citation candidate.
      </p>
    </div>
  )
}
