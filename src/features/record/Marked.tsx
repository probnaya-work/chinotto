/**
 * Marks matched words inside a body.
 *
 * Only ever used for words that literally appear. A match and a guess must not look alike,
 * so inferred results are never marked — the italic and the word "guess" carry those.
 *
 * A phrase is matched across whatever whitespace or punctuation separates its words, so a
 * run that wrapped a line or picked up a comma still highlights as one phrase rather than
 * breaking into pieces.
 */

export function Marked({
  text,
  mark,
  /** Default is Find's ground. A trace uses the quieter `--evidence-ground-trace`. */
  wash = "var(--evidence-ground)",
}: {
  text: string;
  mark?: string | string[] | null;
  wash?: string;
}) {
  const phrases = (Array.isArray(mark) ? mark : [mark])
    .filter((p): p is string => typeof p === "string" && p.trim().length > 0)
    .map((p) => p.trim());

  if (phrases.length === 0) return <>{text}</>;

  let pattern: RegExp;
  try {
    pattern = new RegExp(
      `(${phrases.map((p) => p.split(/\s+/).map(escapeRegExp).join("[\\s\\W]+")).join("|")})`,
      "gi",
    );
  } catch {
    // A query that cannot be compiled marks nothing rather than blanking the row.
    return <>{text}</>;
  }

  const parts = text.split(pattern);
  // An empty alternative would match at every position and loop; split already guards it,
  // but the odd indices are the captures, which is what decides what gets marked.
  return (
    <>
      {parts.map((part, i) =>
        i % 2 === 1 ? (
          <mark key={i} style={{ background: wash, color: "var(--evidence-ink)" }}>
            {part}
          </mark>
        ) : (
          part
        ),
      )}
    </>
  );
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
