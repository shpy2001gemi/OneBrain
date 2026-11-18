import { describe, expect, it } from "vitest";
import { resolveApiBase } from "../src/api/client";

// The built Web is served by the same host as the API (`ku_local_web`,
// `onebrain start --api`), on whatever port the host was started with.
describe("resolveApiBase", () => {
  it("uses the serving origin for a production page loaded over http(s), even with a stale VITE_API_BASE", () => {
    expect(
      resolveApiBase({
        envBase: "http://127.0.0.1:4280",
        dev: false,
        origin: "http://127.0.0.1:4391",
      }),
    ).toBe("http://127.0.0.1:4391");
    expect(
      resolveApiBase({ dev: false, origin: "https://node.example:8443" }),
    ).toBe("https://node.example:8443");
  });

  it("keeps VITE_API_BASE (or the 4280 default) on the Vite dev server", () => {
    expect(
      resolveApiBase({
        envBase: "http://127.0.0.1:4300",
        dev: true,
        origin: "http://localhost:5173",
      }),
    ).toBe("http://127.0.0.1:4300");
    expect(resolveApiBase({ dev: true, origin: "http://localhost:5173" })).toBe(
      "http://127.0.0.1:4280",
    );
  });

  it("lets a split production deployment opt out with VITE_API_SAME_ORIGIN=false", () => {
    expect(
      resolveApiBase({
        envBase: "http://127.0.0.1:4280",
        dev: false,
        sameOrigin: "false",
        origin: "http://localhost:4173",
      }),
    ).toBe("http://127.0.0.1:4280");
  });

  it("falls back to the configured base when there is no usable http(s) origin", () => {
    for (const origin of [undefined, "null", "file://", "tauri://localhost"]) {
      expect(
        resolveApiBase({ envBase: "http://127.0.0.1:4280", dev: false, origin }),
      ).toBe("http://127.0.0.1:4280");
    }
  });
});
