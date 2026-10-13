// Line icons of design/macos (16px grid, 1.35 stroke), shared by the sidebar
// and the toolbar. Decorative: always aria-hidden.

import type { ReactNode, SVGProps } from "react"

function Svg({ children, size = 16, weight = 1.35, ...rest }: { children: ReactNode; size?: number; weight?: number } & SVGProps<SVGSVGElement>) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={weight}
      strokeLinejoin="round"
      strokeLinecap="round"
      aria-hidden="true"
      {...rest}
    >
      {children}
    </svg>
  )
}

type P = { size?: number; weight?: number }

export const LibraryIcon = (p: P) => (
  <Svg {...p}>
    <path d="M2.5 2.5h3v11h-3zM6.8 2.5h3v11h-3z" />
    <path d="M11 3.3l2.3-.6 2 10.3-2.3.6z" />
  </Svg>
)
export const CodebookIcon = (p: P) => (
  <Svg {...p}>
    <path d="M2.5 2.5h5l6 6-5 5-6-6z" />
    <circle cx="5.3" cy="5.3" r=".9" />
  </Svg>
)
export const AnalyserIcon = (p: P) => (
  <Svg {...p}>
    <rect x="2" y="2" width="12" height="12" rx="2.5" />
    <path d="M2 6.5h12M6.5 6.5V14" />
  </Svg>
)
export const RedigerIcon = (p: P) => (
  <Svg {...p}>
    <path d="M10.8 2.7l2.5 2.5-7.8 7.8-3.2.7.7-3.2z" />
  </Svg>
)
export const StudyIcon = (p: P) => (
  <Svg {...p}>
    <path d="M4 1.8h5.2L12.5 5v9.2H4z" />
    <path d="M6.2 8h4M6.2 10.5h4" />
  </Svg>
)
export const GuideIcon = (p: P) => (
  <Svg {...p}>
    <path d="M8 4c-1.4-1-3.4-1.4-5.5-1.2v9.6c2.1-.2 4.1.2 5.5 1.2 1.4-1 3.4-1.4 5.5-1.2V2.8C11.4 2.6 9.4 3 8 4zM8 4v9.6" />
  </Svg>
)
export const ShieldIcon = (p: P) => (
  <Svg {...p}>
    <path d="M8 1.5l5.5 2v4c0 3.5-2.5 5.8-5.5 7-3-1.2-5.5-3.5-5.5-7v-4z" />
  </Svg>
)
export const GearIcon = (p: P) => (
  <Svg {...p}>
    <circle cx="8" cy="8" r="2.2" />
    <path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4" />
  </Svg>
)
export const ProviderIcon = (p: P) => (
  <Svg {...p} weight={p.weight ?? 1.2}>
    <rect x="2" y="3" width="12" height="9" rx="2" />
    <path d="M5.5 14h5M8 12v2M5.5 7.5h.01M10.5 7.5h.01" />
  </Svg>
)
export const SlidersIcon = (p: P) => (
  <Svg {...p} weight={p.weight ?? 1.2}>
    <path d="M2.5 4.5h7M12 4.5h1.5M2.5 11.5h2M7 11.5h6.5" />
    <circle cx="10.5" cy="4.5" r="1.5" />
    <circle cx="5.5" cy="11.5" r="1.5" />
  </Svg>
)
export const SidebarIcon = (p: P) => (
  <Svg {...p} weight={p.weight ?? 1.3}>
    <rect x="1.8" y="2.8" width="12.4" height="10.4" rx="2.2" />
    <path d="M6 2.8v10.4" />
  </Svg>
)
export const SearchIcon = (p: P) => (
  <Svg {...p} size={p.size ?? 14} weight={p.weight ?? 1.5}>
    <circle cx="7" cy="7" r="4.5" />
    <path d="M10.5 10.5l3.5 3.5" />
  </Svg>
)

export const RescanIcon = (p: P) => (
  <Svg {...p} weight={1.4}>
    <path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9" />
    <path d="M13.5 2v3h-3" />
  </Svg>
)
