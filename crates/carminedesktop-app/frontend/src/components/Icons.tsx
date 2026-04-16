import type { JSX } from 'solid-js';

type IconProps = { size?: number; class?: string };

const base = (size: number | undefined): JSX.SvgSVGAttributes<SVGSVGElement> => ({
  width: size ?? 16,
  height: size ?? 16,
  viewBox: '0 0 24 24',
  fill: 'none',
  stroke: 'currentColor',
  'stroke-width': 2,
  'stroke-linecap': 'round',
  'stroke-linejoin': 'round',
});

export const DashboardIcon = (p: IconProps) => (
  <svg {...base(p.size)} stroke-width={2.5} class={p.class}>
    <rect width="8" height="10" x="2" y="2" rx="1" />
    <rect width="8" height="6" x="14" y="2" rx="1" />
    <rect width="8" height="10" x="14" y="12" rx="1" />
    <rect width="8" height="6" x="2" y="16" rx="1" />
  </svg>
);

export const GearIcon = (p: IconProps) => (
  <svg {...base(p.size)} stroke-width={2.5} class={p.class}>
    <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
    <circle cx="12" cy="12" r="3" />
  </svg>
);

export const DrivesIcon = (p: IconProps) => (
  <svg {...base(p.size)} stroke-width={2.5} class={p.class}>
    <path d="M12 2v8" />
    <path d="m16 6-4 4-4-4" />
    <rect width="20" height="8" x="2" y="14" rx="2" />
    <path d="M6 18h.01" />
    <path d="M10 18h.01" />
  </svg>
);

export const OfflineIcon = (p: IconProps) => (
  <svg {...base(p.size)} stroke-width={2.5} class={p.class}>
    <path d="M12 20h.01" />
    <path d="M8.5 16.429a5 5 0 0 1 7 0" />
    <path d="M5 12.859a10 10 0 0 1 5.17-2.69" />
    <path d="M19 12.859a10 10 0 0 0-2.007-1.523" />
    <path d="M2 8.82a15 15 0 0 1 4.177-2.643" />
    <path d="M22 8.82a15 15 0 0 0-11.288-3.764" />
    <path d="m2 2 20 20" />
  </svg>
);

export const AboutIcon = (p: IconProps) => (
  <svg {...base(p.size)} stroke-width={2.5} class={p.class}>
    <circle cx="12" cy="12" r="10" />
    <path d="M12 16v-4" />
    <path d="M12 8h.01" />
  </svg>
);

export const FolderIcon = (p: IconProps) => (
  <svg {...base(p.size ?? 18)} class={p.class}>
    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
  </svg>
);

export const CloseIcon = (p: IconProps) => (
  <svg {...base(p.size ?? 14)} stroke-width={2.5} class={p.class}>
    <path d="M18 6L6 18M6 6l12 12" />
  </svg>
);

export const EmptyOfflineIcon = (p: IconProps) => (
  <svg {...base(p.size ?? 32)} stroke-width={1.5} class={p.class}>
    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
    <line x1="3" y1="3" x2="21" y2="21" />
  </svg>
);

export const OneDriveIcon = (p: IconProps) => (
  <svg {...base(p.size ?? 18)} class={p.class}>
    <path d="M17.5 19a4.5 4.5 0 1 0-1.4-8.78 6 6 0 0 0-11.6 2.28A4.5 4.5 0 0 0 5.5 19z" />
  </svg>
);

export const SharePointIcon = (p: IconProps) => (
  <svg {...base(p.size ?? 18)} class={p.class}>
    <path d="M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z" />
    <path d="M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z" />
  </svg>
);

export const WarningIcon = (p: IconProps) => (
  <svg {...base(p.size ?? 16)} class={p.class}>
    <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
    <line x1="12" y1="9" x2="12" y2="13" />
    <line x1="12" y1="17" x2="12.01" y2="17" />
  </svg>
);
