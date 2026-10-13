// Hands a text to the browser as a file to save (the Markdown export).

export function downloadText(filename: string, text: string, type = "text/markdown;charset=utf-8"): boolean {
  try {
    const url = URL.createObjectURL(new Blob([text], { type }))
    const a = document.createElement("a")
    a.href = url
    a.download = filename
    document.body.appendChild(a)
    a.click()
    a.remove()
    URL.revokeObjectURL(url)
    return true
  } catch {
    return false
  }
}
