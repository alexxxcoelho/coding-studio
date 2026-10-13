import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react"

import { AudioCapsule } from "@/components/coder/audio-capsule"
import { AudioProvider, useAudio } from "@/lib/audio-player"
import { mockApi, renderWithClient, type MockApi } from "@/test-utils"

const MAP = {
  interview_id: "INT-02", has_audio: true, files: [{ rel_path: "a.wav", size: 10, mime: "audio/wav", url: "/api/interviews/INT-02/audio/file" }],
  offset: { offset_seconds: 0, note: "", set: false, updated_at: null, updated_by: null }, timed_units: 1, warnings: [],
  units: [{ unit_id: "INT-02-U001", seq: 1, role: "respondent", timestamp: "00:04", start_seconds: 4, audio_seconds: 4, end_seconds: 9 }],
}

let mock: MockApi
const fake = { currentTime: 0, paused: true, playbackRate: 1, duration: 60, preload: "", addEventListener() {}, removeEventListener() {}, pause() {}, play: vi.fn(async () => {}) }
beforeEach(() => {
  mock = mockApi()
  vi.stubGlobal("Audio", function () { return fake })
})
afterEach(() => cleanup())

function Probe() {
  const a = useAudio()
  return <button onClick={() => a?.seek("INT-02-U001")}>seek</button>
}

describe("audio capsule", () => {
  it("shows nothing without audio", async () => {
    mock.on("GET", "/api/interviews/INT-02/audio", { body: { ...MAP, has_audio: false, files: [], units: [] } })
    renderWithClient(<AudioProvider interviewId="INT-02"><AudioCapsule /></AudioProvider>)
    await waitFor(() => expect(mock.to("GET", "/api/interviews/INT-02/audio")).toHaveLength(1))
    expect(screen.queryByRole("group", { name: "Lecteur audio" })).toBeNull()
  })
  it("plays, changes speed and seeks to a unit", async () => {
    mock.on("GET", "/api/interviews/INT-02/audio", { body: MAP })
    renderWithClient(<AudioProvider interviewId="INT-02"><AudioCapsule /><Probe /></AudioProvider>)
    fireEvent.click(await screen.findByRole("button", { name: "Lecture" }))
    expect(fake.play).toHaveBeenCalled()
    fireEvent.click(screen.getByRole("button", { name: /Vitesse 1x/ }))
    expect(fake.playbackRate).toBe(1.25)
    fireEvent.click(screen.getByText("seek"))
    expect(fake.currentTime).toBe(4)
  })
})
