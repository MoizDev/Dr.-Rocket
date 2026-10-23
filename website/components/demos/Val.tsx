// A printed Racket value, coloured by the shared lexer (display only).
import { styleSpans } from "@/lib/demos/spans";
import s from "./Ide.module.css";

export default function Val({ text }: { text: string }) {
  const spans = styleSpans(text);
  const out: React.ReactNode[] = [];
  let at = 0;
  for (const sp of spans) {
    if (sp.from > at) out.push(text.slice(at, sp.from));
    out.push(
      <span key={sp.from} className={sp.cls}>
        {text.slice(sp.from, sp.to)}
      </span>,
    );
    at = sp.to;
  }
  if (at < text.length) out.push(text.slice(at));
  return <span className={s.val}>{out}</span>;
}
