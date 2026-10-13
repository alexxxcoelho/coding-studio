// Helpers for component tests: a fetch router that records every call, and a
// render wrapper with a fresh query client.

import type { ReactElement } from "react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render } from "@testing-library/react"
import { vi } from "vitest"

export interface Call {
  method: string
  /** Path with its query string, as sent. */
  path: string
  /** Parsed JSON body, undefined when none. */
  body: unknown
}

export interface Reply {
  status?: number
  body?: unknown
}

type Responder = Reply | ((call: Call, nth: number) => Reply)

interface Rule {
  method: string
  path: string | RegExp
  reply: Responder
  hits: number
}

export interface MockApi {
  calls: Call[]
  /** Registers a reply; the latest matching rule wins. */
  on: (method: string, path: string | RegExp, reply: Responder) => MockApi
  /** Calls made with `method` on `path` (string: exact, with query). */
  to: (method: string, path: string | RegExp) => Call[]
}

/** Installs a global fetch that answers from the registered rules. Unmatched calls answer 404. */
export function mockApi(): MockApi {
  const rules: Rule[] = []
  const calls: Call[] = []
  const matches = (rule: Rule["path"], path: string) => (typeof rule === "string" ? rule === path : rule.test(path))
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input)
      const method = (init?.method ?? "GET").toUpperCase()
      const body = typeof init?.body === "string" && init.body ? JSON.parse(init.body) : undefined
      const call: Call = { method, path, body }
      calls.push(call)
      const rule = [...rules].reverse().find((r) => r.method === method && matches(r.path, path))
      if (!rule) return new Response(JSON.stringify({ error: `unmocked ${method} ${path}`, kind: "not_found" }), { status: 404 })
      const reply = typeof rule.reply === "function" ? rule.reply(call, rule.hits++) : rule.reply
      return new Response(JSON.stringify(reply.body ?? null), { status: reply.status ?? 200 })
    }),
  )
  const api: MockApi = {
    calls,
    on(method, path, reply) {
      rules.push({ method: method.toUpperCase(), path, reply, hits: 0 })
      return api
    },
    to: (method, path) => calls.filter((c) => c.method === method.toUpperCase() && matches(path, c.path)),
  }
  return api
}

export function renderWithClient(ui: ReactElement) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 }, mutations: { retry: false } } })
  return render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>)
}
