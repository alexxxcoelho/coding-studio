// Client-side undo of the coding actions: each action leaves the call that
// reverses it (docs: Cmd/Ctrl+Z calls the delete endpoint of what was just
// created). The server keeps the audit trail of both.

export interface UndoEntry {
  /** What the action did, in the words of the screen ("Cost burden retiré"). */
  label: string
  /** Reverses the action. */
  run: () => Promise<unknown>
}

export class UndoStack {
  private entries: UndoEntry[] = []
  private readonly limit: number
  constructor(limit = 50) {
    this.limit = limit
  }

  get size(): number {
    return this.entries.length
  }

  push(entry: UndoEntry): void {
    this.entries.push(entry)
    if (this.entries.length > this.limit) this.entries.shift()
  }

  peek(): UndoEntry | undefined {
    return this.entries[this.entries.length - 1]
  }

  /** Runs the latest reversal. If it fails the entry is kept so that the user can try again. */
  async undo(): Promise<UndoEntry | null> {
    const entry = this.entries.pop()
    if (!entry) return null
    try {
      await entry.run()
    } catch (e) {
      this.entries.push(entry)
      throw e
    }
    return entry
  }

  clear(): void {
    this.entries = []
  }
}
