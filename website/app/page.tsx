import Image from "next/image";
import Download, { HeaderDownload } from "@/components/Download";
import Reveal from "@/components/Reveal";
import { ArrowRight, OsIcon, RocketMark } from "@/components/icons";
import { DOWNLOADS, VERSION } from "@/lib/release";
import s from "./page.module.css";

import checksyntax from "./shots/checksyntax.png";
import debuggerShot from "./shots/debugger.png";
import drracketFiles from "./shots/drracket-files.png";
import editor from "./shots/editor.png";
import languages from "./shots/languages.png";
import light from "./shots/light.png";
import stepper from "./shots/stepper.png";
import terminal from "./shots/terminal.png";
import terminalPanel from "./shots/terminal-panel.png";
import visualizer from "./shots/visualizer.png";

const NAV = [
  ["Features", "#features"],
  ["Visualizer", "#visualizer"],
  ["Languages", "#languages"],
  ["Install", "#install"],
  ["FAQ", "#faq"],
] as const;

const RACKET = "https://racket-lang.org";

export default function Home() {
  const { mac, windows, linux } = DOWNLOADS;
  return (
    <>
      <Reveal />
      <a className={s.skip} href="#main">
        Skip to content
      </a>

      <header className={s.header}>
        <nav className={s.nav} aria-label="Main">
          <a className={s.brand} href="#top" aria-label="Dr. Rocket home">
            <RocketMark size={26} />
            <span>Dr. Rocket</span>
          </a>
          <ul className={s.links}>
            {NAV.map(([label, href]) => (
              <li key={href}>
                <a href={href}>{label}</a>
              </li>
            ))}
          </ul>
          <HeaderDownload />
        </nav>
      </header>

      <main id="main">
        {/* ------------------------------------------------------------------ hero */}
        <section className={s.hero} id="top">
          <div className={`container ${s.heroGrid}`}>
            <div className={s.heroCopy}>
              <p className={s.pill} data-reveal>
                V{VERSION.replace(/-beta\.\d+$/, "")} beta
              </p>
              <h1 className={s.heroTitle} data-reveal style={{ "--reveal-delay": 60 } as React.CSSProperties}>
                Everything DrRacket does, at native speed.
              </h1>
              <p className={s.heroLede} data-reveal style={{ "--reveal-delay": 120 } as React.CSSProperties}>
                Dr. Rocket is a Racket IDE written in Rust. It runs every DrRacket language level, steps through code
                exactly like DrRacket&rsquo;s stepper, and adds a Visualizer that draws your program as it runs.
              </p>
              <div className={s.heroActions} data-reveal style={{ "--reveal-delay": 180 } as React.CSSProperties}>
                <Download />
                <a className={s.ghost} href="#visualizer">
                  See the Visualizer <ArrowRight />
                </a>
              </div>
              <p className={s.heroNote} data-reveal style={{ "--reveal-delay": 220 } as React.CSSProperties}>
                Requires Racket: get it at{" "}
                <a href={RACKET} target="_blank" rel="noopener">
                  racket-lang.org
                </a>
                . Free for macOS, Windows and Linux.
              </p>
            </div>
          </div>

          <div className="container">
            <div className={s.heroPanel} data-reveal style={{ "--reveal-delay": 200 } as React.CSSProperties}>
              <Image
                className={`${s.shot} ${s.heroShot}`}
                src={editor}
                alt="Dr. Rocket with an Intermediate Student program: the file tree on the left, check-expect results passing inline, and the REPL on the right showing the results of a run."
                sizes="(max-width: 1248px) 88vw, 1060px"
                loading="eager"
                fetchPriority="high"
                quality={85}
              />
              <Image
                className={`${s.shot} ${s.heroTerm}`}
                src={terminalPanel}
                alt="An integrated terminal in Dr. Rocket running raco test on an assignment."
                sizes="(max-width: 1248px) 46vw, 560px"
                quality={85}
              />
            </div>
          </div>
        </section>

        {/* ------------------------------------------------------------------ why */}
        <section className={s.why} aria-labelledby="why-title">
          <div className={`container ${s.whyGrid}`}>
            <div data-reveal>
              <p className={s.eyebrow}>Why not DrRacket?</p>
              <h2 id="why-title" className={s.h2}>
                Same Racket underneath. A modern editor on top.
              </h2>
            </div>
            <p className={s.whyText} data-reveal style={{ "--reveal-delay": 80 } as React.CSSProperties}>
              DrRacket is the reference, and Dr. Rocket runs Racket&rsquo;s own stepper, check syntax, debugger and test
              engine under the hood. What&rsquo;s new is everything around them: a GPU-rendered editor, a file tree, tabs,
              a command palette, integrated terminals and themes.
            </p>
          </div>
          <div className="container">
            <dl className={s.stats}>
              {[
                [windows.size ?? "11.5 MB", "Windows download"],
                [mac.size ?? "23 MB", "Universal Mac app, Apple Silicon and Intel"],
                ["11", "language levels, plus any #lang"],
                ["$0", "free, with no account to make"],
              ].map(([value, label], i) => (
                <div key={label} className={s.stat} data-reveal style={{ "--reveal-delay": i * 70 } as React.CSSProperties}>
                  <dt>{label}</dt>
                  <dd>{value}</dd>
                </div>
              ))}
            </dl>
          </div>
        </section>

        {/* ------------------------------------------------------------------ visualizer */}
        <section className={s.viz} id="visualizer" aria-labelledby="viz-title">
          <div className="container">
            <div className={s.vizPanel}>
              <div className={s.vizHead} data-reveal>
                <p className={`${s.eyebrow} ${s.eyebrowOnBlue}`}>
                  The Visualizer <kbd>⇧⌘V</kbd>
                </p>
                <h2 id="viz-title" className={s.vizTitle}>
                  Watch your program think.
                </h2>
                <p className={s.vizLede}>
                  Run a program and replay it one step at a time. Every call gets a frame, trees and lists are drawn as
                  real diagrams, and each step is explained in plain words while its code lights up in the editor.
                </p>
              </div>
              <div className={s.vizStage} data-reveal>
                <Image
                  className={`${s.shot} ${s.vizShot}`}
                  src={visualizer}
                  alt="The Visualizer replaying a binary search tree program: a column of tree-sum call frames on the left, the tree of nodes drawn on the right, and the explanation “tree-sum calls itself (recursion). A new frame goes on the stack.”"
                  sizes="(max-width: 1248px) 90vw, 1100px"
                  quality={85}
                />
              </div>
              <ul className={s.vizPoints}>
                {[
                  ["Frames for every call", "Arguments and locals for each call. Tail calls replace their caller’s frame, as Racket runs them."],
                  ["Trees and lists, drawn", "Structs that point to their own type become trees, cons cells become chains, and both glide as they change."],
                  ["Explained in plain words", "“Is (empty? t) true? Yes.” Each step says what happened and why, so recursion stops being magic."],
                ].map(([title, body], i) => (
                  <li key={title} data-reveal style={{ "--reveal-delay": i * 80 } as React.CSSProperties}>
                    <h3>{title}</h3>
                    <p>{body}</p>
                  </li>
                ))}
              </ul>
            </div>
          </div>
        </section>

        {/* ------------------------------------------------------------------ features */}
        <section className={s.features} id="features" aria-label="Features">
          <div className="container">
            <Feature
              eyebrow="Stepper"
              kbd="⇧⌘E"
              title="A stepper that matches DrRacket, step for step."
              body={
                <>
                  See every reduction before and after, mirrored in your source. Dr. Rocket drives DrRacket&rsquo;s own
                  stepper, so you get the same steps your course notes show, cons chains printed as lists, images drawn
                  inline, all the way to the final value.
                </>
              }
            >
              <div className={s.cardInset}>
                <Image className={`${s.shot} ${s.cardShot}`} src={stepper} alt="The stepper showing a cond expression before and after one reduction, with the matching code highlighted in the editor." sizes="(max-width: 900px) 92vw, 680px" quality={85} />
              </div>
            </Feature>

            <Feature
              eyebrow="Check syntax and debugger"
              kbd="F2"
              title="Binding arrows, rename and a real debugger."
              body={
                <>
                  Check syntax runs in the background: arrows from each binding to its uses, unused bindings, hover info,
                  rename and go to definition across files. Set breakpoints in the gutter and step into, over and out with
                  locals and the call stack. The profiler, test coverage and macro stepper are here too.
                </>
              }
            >
              <div className={s.cardStack}>
                <Image className={`${s.shot} ${s.stackBack}`} src={debuggerShot} alt="The debugger paused at a breakpoint inside tree-sum, with variables and a call stack of recursive calls." sizes="(max-width: 900px) 88vw, 620px" quality={85} />
                <Image className={`${s.shot} ${s.stackFront}`} src={checksyntax} alt="Check syntax arrows from the parameter marks to its three uses, with a tooltip reading 3 bound occurrences." sizes="(max-width: 900px) 60vw, 400px" quality={85} />
              </div>
            </Feature>

            <Feature
              id="drracket"
              eyebrow="DrRacket files"
              title="Your DrRacket files, pasted images and all."
              body={
                <>
                  Files saved by DrRacket open with their images inline, run with real images, and save back in
                  DrRacket&rsquo;s format, so you can switch back at any time. Paste images straight from the clipboard, or
                  use Insert Image.
                </>
              }
            >
              <div className={s.cardInset}>
                <Image className={`${s.shot} ${s.cardShot}`} src={drracketFiles} alt="A Beginning Student file saved by DrRacket with a pasted rocket image, and the REPL showing the images it produced." sizes="(max-width: 900px) 92vw, 680px" quality={85} />
              </div>
            </Feature>

            <Feature
              eyebrow="Terminals and projects"
              kbd="⌃`"
              title="Every course in a tab, a terminal one keystroke away."
              body={
                <>
                  Keep each course open as a project tab with its own files and layout. Run <code>raco</code> in an
                  integrated terminal, search the whole project, jump to any file with <kbd>⌘P</kbd> and reach every command
                  from the palette.
                </>
              }
            >
              <div className={s.cardInset}>
                <Image className={`${s.shot} ${s.cardShot}`} src={terminal} alt="Three project tabs, cs136, math239 and cs135, with a #lang racket file open and an integrated terminal showing raco test results." sizes="(max-width: 900px) 92vw, 680px" quality={85} />
              </div>
            </Feature>

            <Feature
              eyebrow="Themes"
              title="Zed-grade themes, set in Fira Code."
              body={
                <>
                  The default, Moiz One Dark, matches Zed&rsquo;s One Dark Darker exactly. One Light, GitHub Light,
                  Catppuccin, Gruvbox, Tokyo Night and DrRacket&rsquo;s own colours are built in, and every Zed theme you
                  have installed shows up too. Code is set in Fira Code, with ligatures and rainbow brackets.
                </>
              }
              code
            >
              <div className={s.cardStack}>
                <Image className={`${s.shot} ${s.themeBack}`} src={editor} alt="" sizes="(max-width: 900px) 70vw, 480px" quality={85} />
                <Image className={`${s.shot} ${s.themeFront}`} src={light} alt="The same program in the One Light theme." sizes="(max-width: 900px) 80vw, 540px" quality={85} />
              </div>
            </Feature>
          </div>
        </section>

        {/* ------------------------------------------------------------------ languages */}
        <section className={s.langs} id="languages" aria-labelledby="langs-title">
          <div className={`container ${s.langGrid}`}>
            <div className={s.langCopy}>
              <div data-reveal>
                <p className={s.eyebrow}>
                  Language levels <kbd>⌘L</kbd>
                </p>
                <h2 id="langs-title" className={s.h2}>
                  Every language level your course uses.
                </h2>
                <p className={s.lede}>
                  Pick a level just as you would in DrRacket. Teaching-language files keep DrRacket&rsquo;s header, so
                  they open in either IDE, and output style, fraction style, tracing and teachpacks come along.
                </p>
              </div>
              <div className={s.langGroups}>
                {[
                  ["How to Design Programs", ["Beginning Student", "Beginning Student with List Abbreviations", "Intermediate Student", "Intermediate Student with lambda", "Advanced Student"]],
                  ["DeinProgramm", ["Anfänger", "Schreibe Dein Programm!", "fortgeschritten"]],
                  ["Racket", ["#lang racket, and any #lang", "R5RS", "Pretty Big"]],
                ].map(([group, items], i) => (
                  <div key={group as string} className={s.langGroup} data-reveal style={{ "--reveal-delay": i * 80 } as React.CSSProperties}>
                    <h3>{group}</h3>
                    <ul>
                      {(items as string[]).map((item) => (
                        <li key={item}>{item.startsWith("#lang") ? <code>{item}</code> : item}</li>
                      ))}
                    </ul>
                  </div>
                ))}
              </div>
            </div>
            <div className={s.langVisual} data-reveal>
              <Image className={`${s.shot} ${s.langShot}`} src={languages} alt="The Choose Language dialog listing the Racket language, the How to Design Programs teaching languages and DeinProgramm, with output and fraction settings for Beginning Student." sizes="(max-width: 900px) 92vw, 600px" quality={85} />
            </div>
          </div>
        </section>

        {/* ------------------------------------------------------------------ bento */}
        <section className={s.bento} aria-labelledby="start-title">
          <div className="container">
            <div className={s.bentoPanel}>
              <h2 id="start-title" className={`${s.h2} ${s.center}`} data-reveal>
                Get going in a few minutes
              </h2>
              <div className={s.bentoGrid}>
                <a className={`${s.card} ${s.cardBig}`} href="#install" data-reveal>
                  <span className={s.cardTag}>
                    <span>{"{"}</span> Guide <span>{"}"}</span>
                  </span>
                  <span className={s.cardArt} aria-hidden="true">
                    <RocketMark size={92} tone="light" />
                  </span>
                  <span className={s.cardTitle}>Install Dr. Rocket</span>
                  <span className={s.cardBody}>Racket first, then the app. Three short steps on macOS, Windows or Linux.</span>
                  <span className={s.cardLink}>
                    Read the guide <ArrowRight />
                  </span>
                </a>
                <a className={s.card} href="#beta" data-reveal style={{ "--reveal-delay": 80 } as React.CSSProperties}>
                  <span className={s.cardTag}>
                    <span>{"{"}</span> Beta <span>{"}"}</span>
                  </span>
                  <span className={s.cardTitle}>What to try this week</span>
                  <span className={s.cardBody}>Your assignments, the stepper, the Visualizer, and files with pasted images.</span>
                  <span className={s.cardIcon} aria-hidden="true">
                    β
                  </span>
                </a>
                <a className={s.card} href="#report" data-reveal style={{ "--reveal-delay": 140 } as React.CSSProperties}>
                  <span className={s.cardTag}>
                    <span>{"{"}</span> Help <span>{"}"}</span>
                  </span>
                  <span className={s.cardTitle}>Report a problem</span>
                  <span className={s.cardBody}>One menu item copies your version, system and the latest crash report.</span>
                  <span className={s.cardIcon} aria-hidden="true">
                    !
                  </span>
                </a>
              </div>
            </div>
          </div>
        </section>

        {/* ------------------------------------------------------------------ install */}
        <section className={s.install} id="install" aria-labelledby="install-title">
          <div className="container">
            <div className={s.installHead} data-reveal>
              <p className={s.eyebrow}>Install guide</p>
              <h2 id="install-title" className={s.h2}>
                Racket first, then Dr. Rocket.
              </h2>
              <p className={s.lede}>
                Dr. Rocket is the editor; Racket runs your programs. Any recent Racket works and we test with 9.3. If you
                have DrRacket, you already have it. Otherwise,{" "}
                <a href={RACKET} target="_blank" rel="noopener">
                  get Racket at racket-lang.org
                </a>
                .
              </p>
            </div>
            <ol className={s.osCards}>
              <OsCard os="mac" download={mac}>
                <li>
                  Open <code>{mac.filename}</code> and drag Dr. Rocket into Applications.
                </li>
                <li>
                  The beta isn&rsquo;t signed with an Apple certificate yet, so the first time, right-click the app and
                  choose <b>Open</b>, then <b>Open</b> again.
                </li>
                <li>
                  If macOS only offers <b>Move to Trash</b>, open System Settings, then Privacy &amp; Security, and click{" "}
                  <b>Open Anyway</b> next to Dr. Rocket.
                </li>
              </OsCard>
              <OsCard os="windows" download={windows}>
                <li>
                  Unzip <code>{windows.filename}</code>.
                </li>
                <li>
                  Run <code>Dr. Rocket.exe</code> from the <code>Dr. Rocket</code> folder.
                </li>
                <li>
                  If Windows says &ldquo;Windows protected your PC&rdquo;, the beta isn&rsquo;t signed yet: click{" "}
                  <b>More info</b>, then <b>Run anyway</b>.
                </li>
              </OsCard>
              <OsCard os="linux" download={linux}>
                <li>
                  Extract it: <code>tar xzf {linux.filename}</code>
                </li>
                <li>
                  Run <code>&quot;Dr. Rocket/drrocket&quot;</code>.
                </li>
                <li>
                  For your app launcher, put <code>drrocket</code> on your <code>PATH</code> and copy{" "}
                  <code>drrocket.desktop</code> to <code>~/.local/share/applications</code>.
                </li>
              </OsCard>
            </ol>
            <p className={s.installFoot} data-reveal>
              Dr. Rocket finds Racket on its own. If it says &ldquo;Racket not found&rdquo;, set the path in Settings, then
              Racket.
            </p>

            <div className={s.betaGrid} id="beta">
              <div className={s.betaCol} data-reveal>
                <h3>What to try</h3>
                <p>Use it for your real work for a week, the way you&rsquo;d use DrRacket. In particular:</p>
                <ul>
                  <li>
                    Assignments in Beginning or Intermediate Student, with <kbd>⌘R</kbd> and check-expect
                  </li>
                  <li>
                    The stepper (<kbd>⇧⌘E</kbd>): does it match DrRacket?
                  </li>
                  <li>
                    The Visualizer (<kbd>⇧⌘V</kbd>) on a recursive function, a list or a tree
                  </li>
                  <li>DrRacket files with pasted images: open, run, edit, save, reopen in DrRacket</li>
                  <li>The debugger, check syntax (hover, rename with F2) and the docs (F1)</li>
                </ul>
                <p className={s.betaSmall}>On Windows and Linux, read Ctrl for ⌘.</p>
              </div>
              <div className={s.betaCol} data-reveal style={{ "--reveal-delay": 80 } as React.CSSProperties}>
                <h3>Known limitations</h3>
                <ul>
                  <li>
                    XML boxes, Racket boxes, test-case boxes and large letters from DrRacket open read-only, so those files
                    are never damaged.
                  </li>
                  <li>
                    Non-Racket <code>#lang</code>s, like Scribble, are coloured as Racket code.
                  </li>
                  <li>No custom key bindings or spell checking yet.</li>
                  <li>DrRacket plugins, such as a course&rsquo;s handin Submit button, don&rsquo;t run.</li>
                </ul>
              </div>
              <div className={s.betaCol} id="report" data-reveal style={{ "--reveal-delay": 160 } as React.CSSProperties}>
                <h3>Reporting a problem</h3>
                <ol>
                  <li>
                    Choose <b>Report a Problem…</b> from the ☰ menu or the command palette (<kbd>⇧⌘P</kbd>). It copies
                    your version, system and the latest crash report, if any.
                  </li>
                  <li>Paste it into a message to us, and add what you were doing and, if you can, the file.</li>
                </ol>
                <p className={s.betaSmall}>If Dr. Rocket crashes, it saves a report and reminds you on the next launch.</p>
              </div>
            </div>
          </div>
        </section>

        {/* ------------------------------------------------------------------ faq */}
        <section className={s.faq} id="faq" aria-labelledby="faq-title">
          <div className={`container ${s.faqInner}`}>
            <h2 id="faq-title" className={`${s.h2} ${s.center}`} data-reveal>
              Common questions
            </h2>
            <div className={s.faqList} data-reveal>
              {FAQ.map(([q, a]) => (
                <details key={q} name="faq" className={s.qa}>
                  <summary>
                    <span>{q}</span>
                    <span className={s.plus} aria-hidden="true" />
                  </summary>
                  <div className={s.answer}>{a}</div>
                </details>
              ))}
            </div>
          </div>
        </section>

        {/* ------------------------------------------------------------------ cta */}
        <section className={s.cta} aria-labelledby="cta-title">
          <div className={s.ctaPanel}>
            <div className={s.ctaInner}>
              <h2 id="cta-title" className={s.ctaTitle}>
                Your next assignment, at rocket speed.
              </h2>
              <p className={s.ctaLede}>Free for macOS, Windows and Linux.</p>
              <div className={s.ctaActions}>
                <Download tone="inverse" menu="up" />
                <a className={s.ghostInverse} href="#install">
                  Install guide <ArrowRight />
                </a>
              </div>
              <p className={s.ctaNote}>
                Requires Racket: get it at{" "}
                <a href={RACKET} target="_blank" rel="noopener">
                  racket-lang.org
                </a>
              </p>
            </div>
            <div className={s.ctaRocket} aria-hidden="true">
              <RocketMark size={210} tone="light" />
            </div>
          </div>
        </section>
      </main>

      <footer className={s.footer}>
        <div className={`container ${s.footerInner}`}>
          <a className={s.brand} href="#top">
            <RocketMark size={22} />
            <span>Dr. Rocket</span>
          </a>
          <p>
            Made by Moiz and John. Version {VERSION}.
          </p>
          <ul>
            <li>
              <a href="#install">Install</a>
            </li>
            <li>
              <a href="#faq">FAQ</a>
            </li>
            <li>
              <a href={RACKET} target="_blank" rel="noopener">
                racket-lang.org
              </a>
            </li>
          </ul>
        </div>
      </footer>
    </>
  );
}

function Feature({
  id,
  eyebrow,
  kbd,
  title,
  body,
  children,
  code,
}: {
  id?: string;
  eyebrow: string;
  kbd?: string;
  title: string;
  body: React.ReactNode;
  children: React.ReactNode;
  code?: boolean;
}) {
  return (
    <article className={s.feature} id={id}>
      <div className={s.featureCopy} data-reveal>
        <p className={s.eyebrow}>
          {eyebrow} {kbd && <kbd>{kbd}</kbd>}
        </p>
        <h3 className={s.featureTitle}>{title}</h3>
        <p className={s.featureBody}>{body}</p>
        {code && (
          <p className={s.ligatures} aria-label="Fira Code ligatures">
            <code>
              {"->"} {"=>"} {">="} {"<="} {"!="} {"λ"}
            </code>
          </p>
        )}
      </div>
      <div className={s.featureCard} data-reveal style={{ "--reveal-delay": 100 } as React.CSSProperties}>
        {children}
      </div>
    </article>
  );
}

function OsCard({
  os,
  download,
  children,
}: {
  os: "mac" | "windows" | "linux";
  download: (typeof DOWNLOADS)["mac"];
  children: React.ReactNode;
}) {
  return (
    <li className={s.osCard} data-reveal>
      <div className={s.osTop}>
        <span className={s.osBadge}>
          <OsIcon os={os} size={22} />
        </span>
        <div>
          <h3>{download.name}</h3>
          <p>
            {download.kind}
            {download.size && ` · ${download.size}`}
          </p>
        </div>
      </div>
      <ol className={s.steps}>{children}</ol>
      <a className={s.osLink} href={download.href} download={download.filename}>
        Download for {download.name} <ArrowRight />
      </a>
    </li>
  );
}

const FAQ: [string, React.ReactNode][] = [
  [
    "Do I need Racket installed?",
    <p key="a">
      Yes. Dr. Rocket is the editor, and Racket runs your programs, the stepper and check syntax. Any recent version
      works; we test with 9.3. If you have DrRacket, you already have Racket. Dr. Rocket looks for it on your{" "}
      <code>PATH</code>, in Homebrew, in <code>/Applications/Racket*</code> and in <code>C:\Program Files\Racket</code>.
      Otherwise, get it at{" "}
      <a href={RACKET} target="_blank" rel="noopener">
        racket-lang.org
      </a>
      .
    </p>,
  ],
  [
    "Does it open my DrRacket files?",
    <p key="a">
      Yes, including files with pasted images and comment boxes. They run with real images and save back in
      DrRacket&rsquo;s format, so you can reopen them in DrRacket. A few rare boxes (XML boxes, Racket boxes, test-case
      boxes and large letters) make a file open read-only, so it&rsquo;s never damaged.
    </p>,
  ],
  [
    "Is it free?",
    <p key="a">Yes. Dr. Rocket is free to download and use. It&rsquo;s made by Moiz and John.</p>,
  ],
  [
    "Does it run on Windows and Linux?",
    <p key="a">
      Yes: there&rsquo;s a 64-bit Windows zip, a Linux x64 tarball and a universal Mac app. The full test suite runs on
      all three in CI. Most day-to-day use so far has been on macOS, so reports from Windows and Linux are especially
      welcome.
    </p>,
  ],
  [
    "Why is macOS asking me to confirm, or Windows showing SmartScreen?",
    <p key="a">
      Because the beta isn&rsquo;t code-signed yet. On macOS, right-click the app and choose Open the first time (or use
      Open Anyway in System Settings, Privacy &amp; Security). On Windows, click More info, then Run anyway. You only
      need to do this once.
    </p>,
  ],
  [
    "Does it work with my course’s language level?",
    <p key="a">
      If your course uses DrRacket&rsquo;s teaching languages (Beginning Student through Advanced Student, or
      DeinProgramm) or <code>#lang</code>, yes: every level is there, with the same output and fraction settings and
      teachpacks. One caveat: DrRacket plugins, like a course&rsquo;s handin Submit button, don&rsquo;t run in Dr. Rocket,
      so submit the way your course allows outside DrRacket, or from DrRacket itself.
    </p>,
  ],
  [
    "Is the stepper really the same as DrRacket’s?",
    <p key="a">
      It drives DrRacket&rsquo;s own stepper, and we check it against DrRacket on the same files: step for step,
      including cons chains shown as lists and the closing step. As in DrRacket, Advanced Student, the advanced
      DeinProgramm level, R5RS and Pretty Big have no stepper.
    </p>,
  ],
];
