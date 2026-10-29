import { highlightParts } from "@/lib/highlight";
export function HighlightText({
  text,
  query,
}: {
  text: string;
  query: string;
}) {
  return (
    <>
      {highlightParts(text, query).map((part, index) =>
        part.match ? (
          <mark className="lm-mark" key={index}>
            {part.text}
          </mark>
        ) : (
          part.text
        ),
      )}
    </>
  );
}
