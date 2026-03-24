import { useState } from "react";
import type { CSSProperties } from "react";
import type { Snippet } from "../../lib/snippetStore";
import { actionBtnStyle, closeBtnStyle, headerStyle, inputStyle, labelStyle, modalStyle, type SnippetFormData } from "./shared";

export function SnippetForm({
    style,
    className,
    snippet,
    categories,
    onSave,
    onCancel,
}: {
    style?: CSSProperties;
    className?: string;
    snippet: Snippet | null;
    categories: string[];
    onSave: (data: SnippetFormData) => void;
    onCancel: () => void;
}) {
    const [name, setName] = useState(snippet?.name ?? "");
    const [content, setContent] = useState(snippet?.content ?? "");
    const [category, setCategory] = useState(snippet?.category ?? "General");
    const [description, setDescription] = useState(snippet?.description ?? "");
    const [tags, setTags] = useState(snippet?.tags.join(", ") ?? "");

    return (
        <div style={style ?? modalStyle} className={className}>
            <div style={headerStyle}>
                <span>{snippet ? "Edit Snippet" : "New Snippet"}</span>
                <button onClick={onCancel} style={closeBtnStyle}>✕</button>
            </div>
            <div style={{ padding: "12px 16px", display: "flex", flexDirection: "column", gap: 8 }}>
                <label style={labelStyle}>Name</label>
                <input type="text" value={name} onChange={(event) => setName(event.target.value)} style={inputStyle} autoFocus />

                <label style={labelStyle}>Content (command)</label>
                <textarea
                    value={content}
                    onChange={(event) => setContent(event.target.value)}
                    rows={4}
                    style={{ ...inputStyle, fontFamily: "var(--font-mono)", resize: "vertical" }}
                />

                <label style={labelStyle}>Category</label>
                <div style={{ display: "flex", gap: 6 }}>
                    <select value={category} onChange={(event) => setCategory(event.target.value)} style={{ ...inputStyle, width: 150 }}>
                        {[...new Set([...categories, "General", "Git", "Docker", "System", "Network"])].map((value) => (
                            <option key={value} value={value}>{value}</option>
                        ))}
                    </select>
                    <input
                        type="text"
                        value={category}
                        onChange={(event) => setCategory(event.target.value)}
                        placeholder="or type new..."
                        style={{ ...inputStyle, flex: 1 }}
                    />
                </div>

                <label style={labelStyle}>Description</label>
                <input type="text" value={description} onChange={(event) => setDescription(event.target.value)} style={inputStyle} />

                <label style={labelStyle}>Tags (comma-separated)</label>
                <input type="text" value={tags} onChange={(event) => setTags(event.target.value)} style={inputStyle} />

                <div style={{ display: "flex", gap: 8, marginTop: 4 }}>
                    <button
                        onClick={() => {
                            if (!name.trim() || !content.trim()) return;
                            onSave({
                                name,
                                content,
                                category,
                                description,
                                tags: tags.split(",").map((tag) => tag.trim()).filter(Boolean),
                            });
                        }}
                        style={actionBtnStyle}
                    >
                        {snippet ? "Save" : "Create"}
                    </button>
                    <button onClick={onCancel} style={{ ...actionBtnStyle, background: "var(--bg-primary)" }}>Cancel</button>
                </div>
            </div>
        </div>
    );
}
