const p = {
  width: 16,
  height: 16,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.8,
  strokeLinecap: "round",
  strokeLinejoin: "round",
} as const;

export const ServerIcon = () => (
  <svg {...p}>
    <rect x="3" y="4" width="18" height="7" rx="2" />
    <rect x="3" y="13" width="18" height="7" rx="2" />
    <path d="M7 7.5h.01M7 16.5h.01" strokeWidth="2.4" />
  </svg>
);

export const TerminalIcon = () => (
  <svg {...p}>
    <rect x="3" y="4" width="18" height="16" rx="3" />
    <path d="m7 9 3 3-3 3M12.5 15H17" />
  </svg>
);

export const FolderIcon = () => (
  <svg {...p}>
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2.5h8a2 2 0 0 1 2 2V17a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z" />
  </svg>
);

export const ArrowsIcon = () => (
  <svg {...p}>
    <path d="M7 8h13m0 0-3-3m3 3-3 3M17 16H4m0 0 3-3m-3 3 3 3" />
  </svg>
);

export const PulseIcon = () => (
  <svg {...p}>
    <path d="M3 12h4l2.5-6 4 12L16 12h5" />
  </svg>
);

export const JobsIcon = () => (
  <svg {...p}>
    <circle cx="12" cy="12" r="8.5" />
    <path d="M12 7.5V12l3 2" />
  </svg>
);

export const GearIcon = () => (
  <svg {...p}>
    <circle cx="12" cy="12" r="3.2" />
    <path d="M12 3.5v2.2M12 18.3v2.2M3.5 12h2.2M18.3 12h2.2M6 6l1.6 1.6M16.4 16.4 18 18M18 6l-1.6 1.6M7.6 16.4 6 18" />
  </svg>
);

export const PlugIcon = () => (
  <svg {...p}>
    <path d="M9 7V3.5M15 7V3.5M7 7h10v4a5 5 0 0 1-10 0V7ZM12 16v4.5" />
  </svg>
);

export const FileIcon = () => (
  <svg {...p}>
    <path d="M6 3.5h8L19 8.5V20.5H6z" />
    <path d="M13.5 3.5V9H19" />
  </svg>
);
