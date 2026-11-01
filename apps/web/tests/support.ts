import { expect, type Page } from "@playwright/test";
export const ready = async (page: Page) => {
  await page.goto("/");
  await expect(
    page.getByRole("heading", {
      name: "Atlas / developer platform",
      exact: true,
    }),
  ).toBeVisible();
};
