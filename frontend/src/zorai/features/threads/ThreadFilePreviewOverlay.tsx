import { useEffect, useMemo, useState } from "react";
import { fetchFilePreview, fetchGitDiff } from "@/lib/agentWorkContext";
import { shortenHomePath } from "@/lib/workspaceStore";
import {
  previewRequestsForWorkContextEntry,
  threadContextEntryDisplayPath,
  threadContextEntryKey,
} from "./threadContextPreview";
import { useThreadFilePreview } from "./ThreadFilePreviewContext";

type PreviewSection = {
  title: string;
  kind: "git-diff" | "file-preview";
  text: string;
};

export function ThreadFilePreviewOverlay() {
  const { previewTarget, closeThreadFilePreview } = useThreadFilePreview();
  const entry = previewTarget?.entry ?? null;
  const [sections, setSections] = useState<PreviewSection[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const entryKey = useMemo(() => entry ? threadContextEntryKey(entry) : "", [entry]);

  useEffect(() => {
    if (!entry) {
      setSections([]);
      setLoading(false);
      setError(null);
      return;
    }

    let cancelled = false;
    setLoading(true);
    setError(null);
    setSections([]);

    const requests = previewRequestsForWorkContextEntry(entry);
    void Promise.all(requests.map(async (request): Promise<PreviewSection | null> => {
      if (request.type === "git-diff") {
        const diff = await fetchGitDiff(request.repoRoot, request.filePath);
        return diff.trim()
          ? { title: "Git diff", kind: "git-diff", text: diff }
          : null;
      }

      const preview = await fetchFilePreview(request.path);
      if (!preview) return null;
      const text = !preview.isText
        ? "Binary file preview is not available."
        : preview.truncated
          ? `${preview.content}\n\n[Preview truncated]`
          : preview.content;
      return text.trim()
        ? { title: "File preview", kind: "file-preview", text }
        : null;
    }))
      .then((nextSections) => {
        if (!cancelled) {
          setSections(nextSections.filter((section): section is PreviewSection => Boolean(section)));
        }
      })
      .catch((reason: unknown) => {
        if (!cancelled) {
          setError(reason instanceof Error ? reason.message : String(reason));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [entry, entryKey]);

  useEffect(() => {
    if (!entry) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        closeThreadFilePreview();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [closeThreadFilePreview, entry]);

  if (!entry) return null;

  return (
    <section className="zorai-file-preview-overlay" aria-label="File preview">
      <header className="zorai-file-preview-overlay__header">
        <div>
          <div className="zorai-kicker">Files</div>
          <h2>{threadContextEntryDisplayPath(entry, shortenHomePath)}</h2>
          <span>Type: {entry.kind ?? "file"}</span>
        </div>
        <button type="button" className="zorai-ghost-button" onClick={closeThreadFilePreview}>
          [x] Close preview
        </button>
      </header>

      <div className="zorai-file-preview-overlay__body">
        {error ? <div className="zorai-empty zorai-empty--danger">{error}</div> : null}
        {loading ? <div className="zorai-empty">Loading preview...</div> : null}
        {!loading && !error && sections.length === 0 ? (
          <div className="zorai-empty">No preview available for the selected file.</div>
        ) : null}
        {!loading && sections.map((section) => (
          <article key={`${section.title}:${section.kind}`} className="zorai-file-preview-overlay__section">
            <div className="zorai-section-label">{section.title}</div>
            <PreviewText text={section.text} kind={section.kind} />
          </article>
        ))}
      </div>
    </section>
  );
}

function PreviewText({ text, kind }: { text: string; kind: "git-diff" | "file-preview" }) {
  if (kind !== "git-diff") {
    return <pre className="zorai-file-preview-overlay__pre">{text}</pre>;
  }

  return (
    <pre className="zorai-file-preview-overlay__pre">
      {text.split("\n").map((line, index) => {
        const lineClass = line.startsWith("+") && !line.startsWith("+++")
          ? "zorai-diff-line zorai-diff-line--added"
          : line.startsWith("-") && !line.startsWith("---")
            ? "zorai-diff-line zorai-diff-line--removed"
            : line.startsWith("@@")
              ? "zorai-diff-line zorai-diff-line--hunk"
              : "zorai-diff-line";
        return <span key={`${index}:${line}`} className={lineClass}>{line || " "}</span>;
      })}
    </pre>
  );
}
