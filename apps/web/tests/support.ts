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
export async function chooseOption(page: Page, label: string, option: string) {
  await page.getByRole("combobox", { name: label, exact: true }).click();
  await page.getByRole("option", { name: option, exact: true }).click();
}
