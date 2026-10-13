// The audio of an interview (when the library holds one): one controller for the
// toolbar player and for the turns, which seek on a click of their time and show
// which one is being heard.

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"

import { v2, type AudioMap } from "./v2-api"
import { unitAt } from "./v2-text"

export const RATES = [1, 1.25, 1.5] as const
export const audioKey = (id: string) => ["audio", id] as const

export interface AudioController {
  map: AudioMap
  playing: boolean
  time: number
  duration: number | null
  rate: number
  /** The unit being heard, null before the first play. */
  current: string | null
  toggle: () => void
  cycleRate: () => void
  /** Moves to the start of a unit and plays; false when the unit has no time. */
  seek: (unitId: string) => boolean
  canSeek: (unitId: string) => boolean
  setOffset: (seconds: number, note?: string) => Promise<void>
}

const Ctx = createContext<AudioController | null>(null)

/** The controller of the interview being coded, or null without audio. */
export function useAudio(): AudioController | null {
  return useContext(Ctx)
}

export function AudioProvider({ interviewId, children }: { interviewId: string; children: ReactNode }) {
  const qc = useQueryClient()
  const q = useQuery({ queryKey: audioKey(interviewId), queryFn: () => v2.audio(interviewId), staleTime: 30_000 })
  const map = q.data && q.data.has_audio && q.data.files.length > 0 ? q.data : null
  const url = map?.files[0].url ?? null
  const el = useRef<HTMLAudioElement | null>(null)
  const [playing, setPlaying] = useState(false)
  const [time, setTime] = useState(0)
  const [duration, setDuration] = useState<number | null>(null)
  const [rate, setRate] = useState(1)
  const [started, setStarted] = useState(false)

  useEffect(() => {
    if (!url || typeof Audio === "undefined") return
    const a = new Audio(url)
    a.preload = "metadata"
    el.current = a
    const on = {
      play: () => setPlaying(true),
      pause: () => setPlaying(false),
      ended: () => setPlaying(false),
      timeupdate: () => setTime(a.currentTime),
      seeked: () => setTime(a.currentTime),
      loadedmetadata: () => setDuration(Number.isFinite(a.duration) ? a.duration : null),
    }
    for (const [k, f] of Object.entries(on)) a.addEventListener(k, f)
    return () => {
      for (const [k, f] of Object.entries(on)) a.removeEventListener(k, f)
      a.pause()
      el.current = null
      setPlaying(false)
      setTime(0)
      setStarted(false)
    }
  }, [url])

  const play = useCallback(() => {
    const a = el.current
    if (!a) return
    setStarted(true)
    a.playbackRate = rate
    void a.play().catch(() => setPlaying(false))
  }, [rate])

  const value = useMemo<AudioController | null>(() => {
    if (!map) return null
    const timed = new Map(map.units.filter((u) => u.audio_seconds !== null).map((u) => [u.unit_id, u.audio_seconds as number]))
    return {
      map,
      playing,
      time,
      duration,
      rate,
      current: started ? unitAt(map.units, time) : null,
      toggle: () => {
        const a = el.current
        if (!a) return
        if (a.paused) play()
        else a.pause()
      },
      cycleRate: () => {
        const next = RATES[(RATES.indexOf(rate as (typeof RATES)[number]) + 1) % RATES.length]
        setRate(next)
        if (el.current) el.current.playbackRate = next
      },
      seek: (unitId) => {
        const at = timed.get(unitId)
        const a = el.current
        if (at === undefined || !a) return false
        a.currentTime = Math.max(0, at)
        setTime(a.currentTime)
        play()
        return true
      },
      canSeek: (unitId) => timed.has(unitId),
      setOffset: async (seconds, note) => {
        await v2.setAudioOffset(interviewId, seconds, note)
        await qc.invalidateQueries({ queryKey: audioKey(interviewId) })
      },
    }
  }, [map, playing, time, duration, rate, started, play, interviewId, qc])

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>
}
