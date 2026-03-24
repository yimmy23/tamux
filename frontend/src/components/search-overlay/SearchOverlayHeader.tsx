import { Badge } from "../ui";

export function SearchOverlayHeader({
  query,
  matchCount,
  currentIndex,
}: {
  query: string;
  matchCount: number;
  currentIndex: number;
}) {
  return (
    <div className="flex items-start justify-between gap-[var(--space-3)]">
      <div className="flex flex-col gap-[var(--space-2)]">
        <div className="flex flex-wrap items-center gap-[var(--space-2)]">
          <Badge variant="mission">Live Search</Badge>
          <Badge variant="default">Buffer Recall</Badge>
        </div>
        <div className="text-[var(--text-sm)] font-semibold text-[var(--text-primary)]">
          Search within the active terminal buffer
        </div>
      </div>
      {query ? (
        <Badge variant={matchCount > 0 ? "accent" : "default"} className="shrink-0">
          {matchCount > 0 ? `${currentIndex + 1}/${matchCount}` : "0/0"}
        </Badge>
      ) : null}
    </div>
  );
}
