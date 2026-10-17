import type { Metadata, Viewport } from "next";
import { Fira_Code, Geist_Mono, Google_Sans } from "next/font/google";
import { VERSION } from "@/lib/release";
import "./globals.css";

// Google Sans for headings and text alike.
const sans = Google_Sans({ subsets: ["latin"], variable: "--font-sans" });
const mono = Geist_Mono({ subsets: ["latin"], variable: "--font-mono" });
const code = Fira_Code({ subsets: ["latin"], weight: ["400", "500"], variable: "--font-code" });

const description =
  "A fast, native Racket IDE that replaces DrRacket. Every language level, a stepper that matches DrRacket step for step, check syntax, a debugger, and a Visualizer that draws your program as it runs. Free for macOS, Windows and Linux.";

export const metadata: Metadata = {
  metadataBase: new URL("https://doctorocket.com"),
  title: "Dr. Rocket: the fast Racket IDE",
  description,
  applicationName: "Dr. Rocket",
  authors: [{ name: "Moiz" }, { name: "John" }],
  keywords: ["Racket", "DrRacket", "IDE", "HtDP", "Beginning Student", "stepper", "Racket editor"],
  openGraph: {
    type: "website",
    url: "/",
    siteName: "Dr. Rocket",
    title: "Dr. Rocket: the fast Racket IDE",
    description: `Everything DrRacket does, in a native editor written in Rust. Free beta, ${VERSION}.`,
  },
  twitter: { card: "summary_large_image" },
};

export const viewport: Viewport = {
  themeColor: "#ffffff",
  colorScheme: "light",
};

// Runs before first paint: picks the visitor's OS so the matching download button shows
// without a flash (see .dl in globals.css), and marks the page as scripted so scroll
// reveals only hide content when they can also show it.
const bootScript = `(function(){var d=document.documentElement;d.classList.add('js');try{var n=navigator,u=n.userAgentData,p=((u&&u.platform)||n.platform||'')+' '+(n.userAgent||'');var t=function(r){return r.test(p)};d.dataset.os=t(/android|iphone|ipad|ipod/i)||(t(/mac/i)&&n.maxTouchPoints>1)?'other':t(/windows|win32|win64/i)?'windows':t(/mac/i)?'mac':t(/linux|x11|cros/i)?'linux':'other'}catch(e){}})()`;

export default function RootLayout({ children }: LayoutProps<"/">) {
  return (
    <html
      lang="en"
      data-os="other"
      className={`${sans.variable} ${mono.variable} ${code.variable}`}
      suppressHydrationWarning
    >
      <head>
        <script dangerouslySetInnerHTML={{ __html: bootScript }} />
      </head>
      <body>{children}</body>
    </html>
  );
}
