import "@testing-library/jest-dom/vitest"

// jsdom lacks these; cmdk and Radix rely on them.
class RO {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= RO as unknown as typeof ResizeObserver
Element.prototype.scrollIntoView ??= function () {}
