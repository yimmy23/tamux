import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { DatabaseCellEditor } from "./DatabaseView";

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
});
