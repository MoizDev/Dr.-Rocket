// Inline icons. Platform marks use currentColor so they follow the button they sit in.
import type { Os } from "@/lib/release";

type P = { className?: string; size?: number };

export function AppleIcon({ className, size = 18 }: P) {
  return (
    <svg className={className} width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" fill="currentColor">
      <path d="M12.15 6.9c-.95 0-2.42-1.08-3.96-1.04-2.04.03-3.91 1.18-4.96 3.01-2.12 3.68-.55 9.1 1.52 12.09 1.01 1.45 2.2 3.09 3.79 3.04 1.52-.07 2.09-.99 3.93-.99 1.83 0 2.35.99 3.96.95 1.64-.03 2.68-1.48 3.68-2.95 1.16-1.69 1.64-3.33 1.66-3.42-.04-.01-3.18-1.22-3.22-4.86-.03-3.04 2.48-4.49 2.6-4.56-1.43-2.09-3.62-2.32-4.39-2.38-2-.16-3.68 1.09-4.61 1.09Zm3.38-3.07c.84-1.01 1.4-2.43 1.25-3.83-1.21.05-2.66.8-3.53 1.82-.78.9-1.45 2.34-1.27 3.71 1.34.1 2.71-.69 3.55-1.7Z" />
    </svg>
  );
}

export function WindowsIcon({ className, size = 18 }: P) {
  return (
    <svg className={className} width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" fill="currentColor">
      <path d="M1.5 4.3 10 3.1v8.2H1.5V4.3Zm9.6-1.35L22.5 1.3v10H11.1V2.95ZM1.5 12.7H10v8.2l-8.5-1.2v-7Zm9.6 0h11.4v10l-11.4-1.6v-8.4Z" />
    </svg>
  );
}

export function LinuxIcon({ className, size = 18 }: P) {
  // A small Tux: the belly and eyes take --tux-belly so it reads on light and solid buttons.
  return (
    <svg className={className} width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
      <path
        fill="currentColor"
        d="M12 1.2c-2.9 0-4.4 2.3-4.4 5 0 1.3.3 2.2-.4 3.4-1.4 2.3-3.1 4.6-3.1 7.4 0 3.2 3.4 5.6 7.9 5.6s7.9-2.4 7.9-5.6c0-2.8-1.7-5.1-3.1-7.4-.7-1.2-.4-2.1-.4-3.4 0-2.7-1.5-5-4.4-5Z"
      />
      <g fill="var(--tux-belly, #fff)">
        <ellipse cx="12" cy="16.2" rx="4.1" ry="5" />
        <circle cx="10.4" cy="5.6" r="1.25" />
        <circle cx="13.6" cy="5.6" r="1.25" />
      </g>
      <g fill="currentColor">
        <circle cx="10.6" cy="5.8" r=".55" />
        <circle cx="13.4" cy="5.8" r=".55" />
      </g>
      <path fill="#F5B301" d="M10.3 7.6c.5-.5 1.1-.7 1.7-.7s1.2.2 1.7.7c-.4.7-1 1.2-1.7 1.2s-1.3-.5-1.7-1.2Zm-5 13.6c.4-1.2 1.4-2 2.6-1.8 1 .2 1.6 1.1 1.6 2.1 0 .9-1 1.5-2.4 1.5-1.2 0-2.1-.7-1.8-1.8Zm13.4 0c-.4-1.2-1.4-2-2.6-1.8-1 .2-1.6 1.1-1.6 2.1 0 .9 1 1.5 2.4 1.5 1.2 0 2.1-.7 1.8-1.8Z" />
    </svg>
  );
}

export function OsIcon({ os, className, size }: P & { os: Os }) {
  if (os === "mac") return <AppleIcon className={className} size={size} />;
  if (os === "windows") return <WindowsIcon className={className} size={size} />;
  return <LinuxIcon className={className} size={size} />;
}

export function DownloadIcon({ className, size = 18 }: P) {
  return (
    <svg className={className} width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M12 3.5v12M7 11l5 5 5-5M4.5 20.5h15" />
    </svg>
  );
}

export function ChevronDown({ className, size = 16 }: P) {
  return (
    <svg className={className} width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
      <path d="m6 9.5 6 6 6-6" />
    </svg>
  );
}

export function ArrowRight({ className, size = 16 }: P) {
  return (
    <svg className={className} width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M4.5 12h15M13.5 6l6 6-6 6" />
    </svg>
  );
}

export function ArrowUpRight({ className, size = 14 }: P) {
  return (
    <svg className={className} width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M7 17 17 7M8 7h9v9" />
    </svg>
  );
}

/** The Dr. Rocket mark. `tone="dark"` is the dark-bodied variant for light backgrounds. */
export function RocketMark({ className, size = 28, tone = "dark" }: P & { tone?: "dark" | "light" }) {
  const id = `rocket-${tone}`;
  const [top, bottom, flame] = tone === "dark" ? ["#4b5263", "#1b1f27", "#2b5cff"] : ["#ffffff", "#c9d1de", "#61afef"];
  return (
    <svg className={className} height={size} width={(size * 94.53) / 145.79} viewBox="0 0 94.53 145.79" aria-hidden="true">
      <defs>
        <linearGradient id={id} x1="0" y1="0" x2="0" y2="145.79" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor={top} />
          <stop offset="1" stopColor={bottom} />
        </linearGradient>
      </defs>
      <path
        fill={`url(#${id})`}
        d="M92.15,67.94l-6.89-7.15c-2.12-2.19-3.3-5.12-3.3-8.18v-18.99c0-1.7-.68-3.34-1.89-4.54L53.72,2.68c-3.56-3.57-9.35-3.57-12.91,0L14.46,29.08c-1.21,1.2-1.88,2.84-1.88,4.54v18.99c0,3.05-1.19,5.99-3.31,8.19l-6.89,7.14c-1.53,1.58-2.38,3.7-2.38,5.9v34.41c0,7.43,8.78,11.37,14.33,6.43l23.35-20.81c5.47-4.87,13.71-4.87,19.17,0l23.35,20.8c5.54,4.94,14.33,1.01,14.33-6.42v-34.41c0-2.2-.86-4.32-2.38-5.9ZM67.98,47.09l-7.55,2.22c-4.3,1.26-7.66,4.62-8.92,8.91l-2.22,7.56c-.59,2.03-3.47,2.03-4.06,0l-2.22-7.56c-1.26-4.29-4.62-7.65-8.92-8.91l-7.55-2.22c-2.03-.6-2.03-3.47,0-4.07l7.55-2.21c4.3-1.27,7.66-4.62,8.92-8.92l2.22-7.56c.59-2.03,3.47-2.03,4.06,0l2.22,7.56c1.26,4.3,4.62,7.65,8.92,8.92l7.55,2.21c2.03.6,2.03,3.47,0,4.07Z"
      />
      <path
        fill={flame}
        d="M64.31,123.89l-12.77,19.58c-2.01,3.09-6.54,3.09-8.55,0l-12.77-19.58c-2.02-3.09-1.55-7.18,1.11-9.74l11.48-11.02c2.49-2.39,6.42-2.39,8.91,0l11.48,11.02c2.66,2.56,3.13,6.64,1.11,9.74Z"
      />
    </svg>
  );
}

export function GitHubIcon({ className, size = 18 }: P) {
  return (
    <svg className={className} width={size} height={size} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M12 .5a11.5 11.5 0 0 0-3.64 22.41c.58.1.79-.25.79-.56v-2c-3.2.7-3.88-1.37-3.88-1.37-.52-1.33-1.28-1.69-1.28-1.69-1.05-.71.08-.7.08-.7 1.16.08 1.77 1.19 1.77 1.19 1.03 1.77 2.7 1.26 3.36.96.1-.75.4-1.26.73-1.55-2.55-.29-5.24-1.28-5.24-5.69 0-1.26.45-2.29 1.19-3.1-.12-.29-.52-1.46.11-3.05 0 0 .97-.31 3.17 1.18a11 11 0 0 1 5.77 0c2.2-1.49 3.17-1.18 3.17-1.18.63 1.59.23 2.76.11 3.05.74.81 1.19 1.84 1.19 3.1 0 4.42-2.69 5.4-5.26 5.68.41.36.78 1.06.78 2.14v3.17c0 .31.21.67.8.56A11.5 11.5 0 0 0 12 .5Z" />
    </svg>
  );
}
