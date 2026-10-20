import { describe, expect, it } from "vitest"
import { isTypingTarget, resolveKey } from "./coder-keys"

const free = { typing: false }

describe("resolveKey", () => {
  it("applies the codes 1 to 9", () => {
    expect(resolveKey({ key: "1" }, free)).toEqual({ type: "code", n: 1 })
    expect(resolveKey({ key: "9" }, free)).toEqual({ type: "code", n: 9 })
    expect(resolveKey({ key: "0" }, free)).toBeNull()
  })
  it("maps N, R, H, D and the slash", () => {
    expect(resolveKey({ key: "n" }, free)).toEqual({ type: "notRelevant" })
    expect(resolveKey({ key: "N" }, free)).toEqual({ type: "notRelevant" })
    expect(resolveKey({ key: "r" }, free)).toEqual({ type: "review" })
    expect(resolveKey({ key: "h" }, free)).toEqual({ type: "highlight" })
    expect(resolveKey({ key: "d" }, free)).toEqual({ type: "detail" })
    expect(resolveKey({ key: "/" }, free)).toEqual({ type: "search" })
  })
  it("opens code search with C only outside fields and dialogs", () => {
    expect(resolveKey({ key: "c" }, free)).toEqual({ type: "codeSearch" })
    expect(resolveKey({ key: "C" }, free)).toEqual({ type: "codeSearch" })
    expect(resolveKey({ key: "c" }, { typing: true })).toBeNull()
    expect(resolveKey({ key: "c" }, { typing: false, modalOpen: true })).toBeNull()
    expect(resolveKey({ key: "c", ctrlKey: true }, free)).toBeNull()
  })
  it("moves with the arrows, and Enter goes to the next turn", () => {
    expect(resolveKey({ key: "ArrowDown" }, free)).toEqual({ type: "next" })
    expect(resolveKey({ key: "ArrowUp" }, free)).toEqual({ type: "prev" })
    expect(resolveKey({ key: "Enter" }, free)).toEqual({ type: "next" })
  })
  it("undoes with Cmd+Z and Ctrl+Z, not with Shift", () => {
    expect(resolveKey({ key: "z", metaKey: true }, free)).toEqual({ type: "undo" })
    expect(resolveKey({ key: "Z", ctrlKey: true }, free)).toEqual({ type: "undo" })
    expect(resolveKey({ key: "z", metaKey: true, shiftKey: true }, free)).toBeNull()
  })
  it("makes a code with Cmd+N", () => {
    expect(resolveKey({ key: "n", metaKey: true }, free)).toEqual({ type: "newCode" })
  })
  it("leaves other shortcuts of the system alone", () => {
    expect(resolveKey({ key: "r", metaKey: true }, free)).toBeNull()
    expect(resolveKey({ key: "1", ctrlKey: true }, free)).toBeNull()
    expect(resolveKey({ key: "h", altKey: true }, free)).toBeNull()
    expect(resolveKey({ key: ",", metaKey: true }, free)).toBeNull()
  })
  it("does not type codes into a field", () => {
    const typing = { typing: true }
    expect(resolveKey({ key: "1" }, typing)).toBeNull()
    expect(resolveKey({ key: "n" }, typing)).toBeNull()
    expect(resolveKey({ key: "ArrowDown" }, typing)).toBeNull()
    expect(resolveKey({ key: "Enter" }, typing)).toBeNull()
  })
  it("still closes with Escape from a field", () => {
    expect(resolveKey({ key: "Escape" }, { typing: true })).toEqual({ type: "escape" })
  })
  it("leaves the undo of a text field to the field, and never undoes over a dialog", () => {
    expect(resolveKey({ key: "z", metaKey: true }, { typing: true })).toBeNull()
    expect(resolveKey({ key: "z", metaKey: true }, { typing: false, modalOpen: true })).toBeNull()
  })
  it("ignores keys while a dialog is open, except Escape", () => {
    const modal = { typing: false, modalOpen: true }
    expect(resolveKey({ key: "1" }, modal)).toBeNull()
    expect(resolveKey({ key: "Escape" }, modal)).toEqual({ type: "escape" })
  })
  it("ignores composition and handled events", () => {
    expect(resolveKey({ key: "1", isComposing: true }, free)).toBeNull()
    expect(resolveKey({ key: "1", defaultPrevented: true }, free)).toBeNull()
  })
  it("ignores shifted letters and unknown keys", () => {
    expect(resolveKey({ key: "H", shiftKey: true }, free)).toBeNull()
    expect(resolveKey({ key: "x" }, free)).toBeNull()
  })
})

describe("isTypingTarget", () => {
  it("knows fields", () => {
    expect(isTypingTarget(document.createElement("input"))).toBe(true)
    expect(isTypingTarget(document.createElement("textarea"))).toBe(true)
    expect(isTypingTarget(document.createElement("select"))).toBe(true)
    expect(isTypingTarget(document.createElement("button"))).toBe(false)
    expect(isTypingTarget(document.body)).toBe(false)
    expect(isTypingTarget(null)).toBe(false)
  })
})
