// The audio player of the Coder toolbar: play and pause, time, speed, and the
// offset between the audio and the transcript. Shown only when the interview
// has an audio file.

import { useState } from "react"
import { Pause, Play } from "lucide-react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
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
      toast.success(`Décalage de ${parsed} s enregistré`)
      setOpen(false)
    } catch (e) {
      reportError(e, "Décalage non enregistré")
    }
  }
  return (
    <div className="cap" role="group" aria-label="Lecteur audio">
      <button type="button" className="ci" onClick={audio.toggle} aria-label={audio.playing ? "Pause" : "Lecture"} title={audio.playing ? "Pause" : "Lecture"}>
        {audio.playing ? <Pause size={13} aria-hidden="true" /> : <Play size={13} aria-hidden="true" />}
      </button>
      <span className="ci mono" aria-label="Position" style={{ cursor: "default" }}>
        {clock(audio.time)}
        {audio.duration !== null && ` / ${clock(audio.duration)}`}
      </span>
      <button type="button" className="ci" onClick={audio.cycleRate} aria-label={`Vitesse ${audio.rate}x, changer`} title="Changer la vitesse">
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
          <button type="button" className="ci" title="Décalage entre l'audio et le texte">
            Décalage
          </button>
        </PopoverTrigger>
        <PopoverContent align="end" className="w-72" aria-label="Décalage de l'audio">
          <span className="small l2">
            Secondes à ajouter aux horodatages du texte pour trouver le bon moment de l'audio. Négatif pour avancer. Exemple : 2,5 ou 1:30.
          </span>
          <Input aria-label="Décalage en secondes" value={text} onChange={(e) => setText(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void save()} />
          {audio.map.warnings.map((w) => (
            <span key={w} className="small l2">
              {w}
            </span>
          ))}
          <Button size="sm" disabled={parsed === null} onClick={() => void save()}>
            Enregistrer le décalage
          </Button>
        </PopoverContent>
      </Popover>
    </div>
  )
}
