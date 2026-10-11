import { toast } from "sonner"

import { ApiError } from "./api"

/** Shows a server error as a toast, with the validation issues if any. */
export function reportError(err: unknown, prefix?: string): void {
  if (err instanceof ApiError) {
    const description = err.issues.length
      ? err.issues.slice(0, 4).map((i) => `${i.location}: ${i.message}`).join("\n")
      : undefined
    toast.error(prefix ? `${prefix}: ${err.message}` : err.message, { description })
    return
  }
  toast.error(prefix ?? (err instanceof Error ? err.message : String(err)))
}
