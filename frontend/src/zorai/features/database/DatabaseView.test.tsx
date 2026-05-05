import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { DatabaseCellEditor, DatabaseSqlConsole } from "./DatabaseView";

describe("DatabaseView editing controls", () => {
  it("renders row values with textarea editors", () => {
    const html = renderToStaticMarkup(
      <table>
        <tbody>
          <tr>
            <DatabaseCellEditor
              editable
              loading={false}
              isDirty={false}
              isSelected={false}
              value={"line one\nline two"}
              onChange={() => {}}
            />
          </tr>
        </tbody>
      </table>,
    );

    expect(html).toContain("<textarea");
    expect(html).toContain("line one");
    expect(html).toContain("line two");
    expect(html).not.toContain("<input");
  });

  it("marks selected cells for bulk database edits", () => {
    const html = renderToStaticMarkup(
      <table>
        <tbody>
          <tr>
            <DatabaseCellEditor
              editable
              loading={false}
              isDirty={false}
              isSelected
              value={"selected"}
              onChange={() => {}}
            />
          </tr>
        </tbody>
      </table>,
    );

    expect(html).toContain("zorai-database-cell--selected");
    expect(html).toContain("aria-selected=\"true\"");
  });
});

describe("Database SQL console", () => {
  it("renders a growing SQL textarea and run button", () => {
    const html = renderToStaticMarkup(
      <DatabaseSqlConsole
        sql={"SELECT *\nFROM agent_messages"}
        running={false}
        result={null}
        error={null}
        onSqlChange={() => {}}
        onRun={() => {}}
        onStop={() => {}}
      />,
    );

    expect(html).toContain("zorai-database-sql-textarea");
    expect(html).toContain("SELECT *");
    expect(html).toContain("Run");
  });

  it("shows stop while SQL is executing", () => {
    const html = renderToStaticMarkup(
      <DatabaseSqlConsole
        sql={"SELECT 1"}
        running
        result={null}
        error={null}
        onSqlChange={() => {}}
        onRun={() => {}}
        onStop={() => {}}
      />,
    );

    expect(html).toContain("Stop");
    expect(html).not.toContain(">Run<");
  });
});
