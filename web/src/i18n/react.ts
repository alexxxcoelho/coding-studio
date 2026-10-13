// React side of the language: a hook that follows the current language.

import { useSyncExternalStore } from "react"

import { getLang, subscribeLang, t, type Lang } from "./index"

/** The current language; the calling component re-renders when it changes. */
export function useLang(): Lang {
  return useSyncExternalStore(subscribeLang, getLang, getLang)
}

/** `t` bound to the current language: the calling component follows language changes. */
export function useT(): typeof t {
  useLang()
  return t
}
