import { DOWNLOADS, OSES } from "@/lib/release";
import DownloadMenu from "./DownloadMenu";
import { DownloadIcon, OsIcon } from "./icons";
import styles from "./Download.module.css";

type Props = {
  /** "solid" on white, "inverse" (white button) on the blue CTA panel. */
  tone?: "solid" | "inverse";
  /** Which way the other-platforms menu opens. */
  menu?: "down" | "up";
};

/**
 * The OS-aware download button: one link per platform, of which CSS shows the visitor's
 * (html[data-os], set before paint), plus a menu with every platform.
 */
export default function Download({ tone = "solid", menu = "down" }: Props) {
  return (
    <div className={`${styles.split} ${tone === "inverse" ? styles.inverse : ""}`}>
      {OSES.map((os) => {
        const d = DOWNLOADS[os];
        return (
          <a
            key={os}
            data-for={os}
            className={styles.main}
            href={d.href}
            download={d.filename}
            aria-label={`Download Dr. Rocket for ${d.name}${d.size ? `, ${d.size}` : ""}`}
          >
            <OsIcon os={os} className={styles.osIcon} size={19} />
            <span>Download for {d.name}</span>
            <DownloadIcon className={styles.arrow} size={17} />
          </a>
        );
      })}
      <a data-for="other" className={styles.main} href="#install">
        <DownloadIcon className={styles.osIcon} size={18} />
        <span>Download Dr. Rocket</span>
      </a>
      <DownloadMenu downloads={DOWNLOADS} placement={menu} />
    </div>
  );
}

/** The compact button in the header: same OS detection, no menu. */
export function HeaderDownload() {
  return (
    <>
      {OSES.map((os) => (
        <a
          key={os}
          data-for={os}
          className={styles.header}
          href={DOWNLOADS[os].href}
          download={DOWNLOADS[os].filename}
          aria-label={`Download Dr. Rocket for ${DOWNLOADS[os].name}`}
        >
          <OsIcon os={os} size={15} />
          Download
        </a>
      ))}
      <a data-for="other" className={styles.header} href="#install">
        Download
      </a>
    </>
  );
}
