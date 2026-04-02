import ReactMarkdown from "react-markdown";
import type { Components } from "react-markdown";
import remarkGfm from "remark-gfm";

const markdownComponents: Components = {
  p: ({ children }) => <p>{children}</p>,
  a: ({ href, children }) => (
    <a
      href={href}
      target="_blank"
      rel="noopener noreferrer"
      style={{ color: "#5ee7df", textDecoration: "underline", textUnderlineOffset: 2 }}
    >
      {children}
    </a>
  ),
  pre: ({ children }) => (
    <pre
      style={{
        margin: "6px 0",
        padding: "10px 12px",
        background: "rgba(0, 0, 0, 0.35)",
        borderRadius: "var(--radius-md)",
        overflowX: "auto",
        fontSize: "var(--text-xs)",
        lineHeight: 1.5,
        border: "1px solid rgba(255,255,255,0.08)",
      }}
    >
      {children}
    </pre>
  ),
  code: ({ className, children }) => {
    const isBlock = className?.startsWith("language-");
    if (isBlock) {
      return (
        <code style={{ fontFamily: "var(--font-mono)", fontSize: "inherit" }}>
          {children}
        </code>
      );
    }
    return (
      <code
        style={{
          fontFamily: "var(--font-mono)",
          background: "rgba(255, 255, 255, 0.08)",
          padding: "1px 5px",
          borderRadius: 3,
          fontSize: "0.9em",
        }}
      >
        {children}
      </code>
    );
  },
  ul: ({ children }) => <ul style={{ margin: "4px 0", paddingLeft: 20 }}>{children}</ul>,
  ol: ({ children }) => <ol style={{ margin: "4px 0", paddingLeft: 20 }}>{children}</ol>,
  li: ({ children }) => <li style={{ margin: "2px 0" }}>{children}</li>,
  h1: ({ children }) => <h4 style={{ margin: "8px 0 4px", fontSize: "1.1em", fontWeight: 700 }}>{children}</h4>,
  h2: ({ children }) => <h5 style={{ margin: "8px 0 4px", fontSize: "1.05em", fontWeight: 700 }}>{children}</h5>,
  h3: ({ children }) => <h6 style={{ margin: "6px 0 4px", fontSize: "1em", fontWeight: 600 }}>{children}</h6>,
  h4: ({ children }) => <h6 style={{ margin: "6px 0 4px", fontSize: "0.95em", fontWeight: 600 }}>{children}</h6>,
  h5: ({ children }) => <h6 style={{ margin: "4px 0 2px", fontSize: "0.9em", fontWeight: 600 }}>{children}</h6>,
  h6: ({ children }) => <h6 style={{ margin: "4px 0 2px", fontSize: "0.85em", fontWeight: 600 }}>{children}</h6>,
  blockquote: ({ children }) => (
    <blockquote
      style={{
        margin: "6px 0",
        paddingLeft: 12,
        borderLeft: "3px solid rgba(94, 231, 223, 0.4)",
        color: "var(--text-secondary)",
        fontStyle: "italic",
      }}
    >
      {children}
    </blockquote>
  ),
  table: ({ children }) => (
    <div style={{ overflowX: "auto", margin: "6px 0" }}>
      <table
        style={{
          width: "100%",
          borderCollapse: "collapse",
          fontSize: "var(--text-xs)",
        }}
      >
        {children}
      </table>
    </div>
  ),
  th: ({ children }) => (
    <th
      style={{
        textAlign: "left",
        padding: "4px 8px",
        borderBottom: "1px solid rgba(255,255,255,0.15)",
        fontWeight: 600,
      }}
    >
      {children}
    </th>
  ),
  td: ({ children }) => (
    <td
      style={{
        padding: "4px 8px",
        borderBottom: "1px solid rgba(255,255,255,0.06)",
      }}
    >
      {children}
    </td>
  ),
  hr: () => (
    <hr
      style={{
        border: "none",
        borderTop: "1px solid rgba(255,255,255,0.1)",
        margin: "8px 0",
      }}
    />
  ),
};

export function MarkdownContent({ content }: { content: string }) {
  return (
    <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents}>
      {content}
    </ReactMarkdown>
  );
}
