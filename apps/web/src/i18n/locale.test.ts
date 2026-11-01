import { expect, it } from "vitest";
import { formatMoney, translate } from "./locale";
it("formats exact large minor units and currency exponents", () => {
  expect(formatMoney("900719925474099199", "USD", "en-US")).toBe(
    "$9,007,199,254,740,991.99",
  );
  expect(formatMoney("123", "JPY", "en-US")).toBe("¥123");
  expect(formatMoney("1234", "KWD", "en-US")).toContain("1.234");
  expect(formatMoney("-1", "USD", "en-US")).toContain("0.01");
  expect(formatMoney(null, "USD", "en-US")).toBe("Unknown");
  expect(formatMoney("1.1", "USD", "en-US")).toBe("Unavailable");
});
it("supports long pseudolocalized navigation without inventing translations", () => {
  expect(translate("results-activity", "qps-rtl").length).toBeGreaterThan(
    translate("results-activity", "en-US").length,
  );
});
