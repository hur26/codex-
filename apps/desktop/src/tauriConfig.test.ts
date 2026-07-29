import { describe, expect, it } from "vitest";

import config from "../src-tauri/tauri.conf.json";

describe("Tauri window configuration", () => {
  it("disables Tauri native file drop so WebView DOM drag-and-drop works", () => {
    expect(config.app.windows[0].dragDropEnabled).toBe(false);
  });
});
