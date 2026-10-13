// The audio player of the Coder toolbar: play and pause, time, speed, and the
// offset between the audio and the transcript. Shown only when the interview
// has an audio file.

import { useState } from "react"
import { Pause, Play } from "lucide-react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import { t } from "@/i18n"
import { useAudio } from "@/lib/audio-player"
import { reportError } from "@/lib/notify"
import { clock, parseOffset } from "@/lib/v2-text"

export function AudioCapsule() {
  const audio = useAudio()
  const [open, setOpen] = useState(false)
  const [text, setText] = useState("")
  if (!audio) return null
  const parsed = parseOffset(text)
  const save = async () => {
    if (parsed === null) return
    try {
      await audio.setOffset(parsed)
      toast.success(t("coder.audio_capsule.decalage_de_parsed_s_enregistre", { parsed }))
      setOpen(false)
    } catch (e) {
      reportError(e, t("coder.audio_capsule.decalage_non_enregistre"))
    }
  }
  return (
    <div className="cap" role="group" aria-label={t("coder.audio_capsule.lecteur_audio")}>
      <button type="button" className="ci" onClick={audio.toggle} aria-label={audio.playing ? "Pause" : "Lecture"} title={audio.playing ? "Pause" : "Lecture"}>
        {audio.playing ? <Pause size={13} aria-hidden="true" /> : <Play size={13} aria-hidden="true" />}
      </button>
      <span className="ci mono" aria-label={t("coder.audio_capsule.position")} style={{ cursor: "default" }}>
        {clock(audio.time)}
        {audio.duration !== null && ` / ${clock(audio.duration)}`}
      </span>
      <button type="button" className="ci" onClick={audio.cycleRate} aria-label={t("coder.audio_capsule.vitesse_rate_x_changer", { rate: audio.rate })} title={t("coder.audio_capsule.changer_la_vitesse")}>
        {audio.rate}x
      </button>
      <Popover
        open={open}
        onOpenChange={(o) => {
          setOpen(o)
          if (o) setText(String(audio.map.offset.offset_seconds))
        }}
      >
        <PopoverTrigger asChild>
          <button type="button" className="ci" title={t("coder.audio_capsule.decalage_entre_l_audio_et")}>
            {t("coder.audio_capsule.decalage")}
          </button>
        </PopoverTrigger>
        <PopoverContent align="end" className="w-72" aria-label={t("coder.audio_capsule.decalage_de_l_audio")}>
          <span className="small l2">
            {t("coder.audio_capsule.secondes_a_ajouter_aux_horodatages")}
          </span>
          <Input aria-label={t("coder.audio_capsule.decalage_en_secondes")} value={text} onChange={(e) => setText(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void save()} />
          {audio.map.warnings.map((w) => (
            <span key={w} className="small l2">
              {w}
            </span>
          ))}
          <Button size="sm" disabled={parsed === null} onClick={() => void save()}>
            {t("coder.audio_capsule.enregistrer_le_decalage")}
          </Button>
        </PopoverContent>
      </Popover>
    </div>
  )
}
