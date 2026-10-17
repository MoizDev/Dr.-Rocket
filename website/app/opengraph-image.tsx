// The link preview (Discord, iMessage, Slack, X): headline and logo beside a real screenshot.
import { ImageResponse } from "next/og";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import sharp from "sharp";
import { VERSION_LABEL } from "@/lib/release";

export const alt = "Dr. Rocket: everything DrRacket does, at native speed. A Racket IDE for macOS, Windows and Linux.";
export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

// Same mark as components/icons.tsx, dark body for a white background.
const rocket = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 94.53 145.79"><defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="145.79" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#4b5263"/><stop offset="1" stop-color="#1b1f27"/></linearGradient></defs><path fill="url(#g)" d="M92.15,67.94l-6.89-7.15c-2.12-2.19-3.3-5.12-3.3-8.18v-18.99c0-1.7-.68-3.34-1.89-4.54L53.72,2.68c-3.56-3.57-9.350-3.57-12.91,0L14.46,29.08c-1.21,1.2-1.88,2.84-1.88,4.54v18.99c0,3.050-1.19,5.99-3.31,8.19l-6.89,7.14c-1.53,1.58-2.380,3.7-2.380,5.9v34.41c0,7.43,8.78,11.37,14.33,6.430l23.35-20.81c5.47-4.87,13.71-4.87,19.17,0l23.35,20.8c5.54,4.94,14.33,1.01,14.33-6.42v-34.41c0-2.2-.86-4.32-2.38-5.9ZM67.98,47.09l-7.55,2.22c-4.3,1.26-7.66,4.62-8.92,8.91l-2.22,7.56c-.59,2.03-3.47,2.03-4.06,0l-2.22-7.56c-1.26-4.29-4.62-7.65-8.92-8.91l-7.55-2.22c-2.03-.6-2.03-3.47,0-4.07l7.55-2.21c4.3-1.27,7.66-4.62,8.92-8.92l2.22-7.56c.59-2.03,3.47-2.03,4.06,0l2.22,7.56c1.26,4.3,4.62,7.65,8.92,8.92l7.55,2.21c2.03.6,2.03,3.47,0,4.07Z"/><path fill="#2b5cff" d="M64.31,123.89l-12.77,19.58c-2.01,3.09-6.54,3.09-8.55,0l-12.77-19.58c-2.02-3.09-1.55-7.18,1.11-9.74l11.48-11.02c2.49-2.39,6.42-2.39,8.91,0l11.48,11.02c2.66,2.56,3.13,6.64,1.11,9.74Z"/></svg>`;

export default async function Image() {
  const [display, mono, shot] = await Promise.all([
    readFile(join(process.cwd(), "assets", "fonts", "GoogleSans-SemiBold.ttf")),
    readFile(join(process.cwd(), "assets", "fonts", "GeistMono-Medium.ttf")),
    // The screenshots are Display P3; convert to sRGB, which is all a PNG preview can show.
    sharp(join(process.cwd(), "app", "shots", "editor.png")).resize(1240).toColourspace("srgb").jpeg({ quality: 88 }).toBuffer(),
  ]);

  return new ImageResponse(
    (
      <div style={{ width: "100%", height: "100%", display: "flex", background: "#ffffff", position: "relative" }}>
        <div style={{ display: "flex", flexDirection: "column", padding: "64px 0 60px 72px", width: 600, height: "100%" }}>
          <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
            <img src={`data:image/svg+xml;base64,${Buffer.from(rocket).toString("base64")}`} width={31} height={48} alt="" />
            <div style={{ fontFamily: "Display", fontSize: 34, color: "#0a1433", letterSpacing: -1 }}>Dr. Rocket</div>
          </div>
          <div style={{ display: "flex", gap: 8, marginTop: 52 }}>
            <div style={{ fontFamily: "Mono", fontSize: 18, color: "#fff", background: "#2b5cff", padding: "7px 14px", borderRadius: 9 }}>
              {VERSION_LABEL}
            </div>
            <div style={{ fontFamily: "Mono", fontSize: 18, color: "#2b5cff", background: "rgba(43,92,255,0.1)", padding: "7px 14px", borderRadius: 9 }}>
              OSS
            </div>
          </div>
          <div style={{ fontFamily: "Display", fontSize: 66, lineHeight: 1.02, letterSpacing: -2.6, color: "#0a1433", marginTop: 26 }}>
            Everything DrRacket does, at native speed.
          </div>
          <div style={{ fontFamily: "Mono", fontSize: 19, color: "#5a6687", marginTop: "auto", letterSpacing: 0.5 }}>
            doctorocket.com · macOS · Windows · Linux
          </div>
        </div>
        <div
          style={{
            position: "absolute",
            left: 640,
            top: 64,
            width: 640,
            height: 640,
            display: "flex",
            borderRadius: 36,
            background: "linear-gradient(160deg, #9fb8ff, #4d78ff)",
            padding: "44px 0 0 44px",
            overflow: "hidden",
          }}
        >
          <img
            src={`data:image/jpeg;base64,${shot.toString("base64")}`}
            width={1240 * 0.62}
            height={778 * 0.62}
            alt=""
            style={{ borderRadius: 14, boxShadow: "0 30px 60px -12px rgba(10,20,51,0.55)" }}
          />
        </div>
      </div>
    ),
    {
      ...size,
      fonts: [
        { name: "Display", data: display, style: "normal", weight: 600 },
        { name: "Mono", data: mono, style: "normal", weight: 500 },
      ],
    },
  );
}
