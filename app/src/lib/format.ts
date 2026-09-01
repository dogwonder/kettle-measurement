// Display formatting for the runner's exact-decimal money strings and
// ISO dates. String surgery only — parseFloat would reintroduce the
// float problem the whole pipeline avoids (never float money).
import type { IsoDate, IsoMonth, Money } from "./types";

const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
] as const;

/** "1046.64" → "£1,046.64"; "-24.99" → "−£24.99" (proper minus). */
export function formatAmount(amount: Money): string {
  const negative = amount.startsWith("-");
  const bare = negative ? amount.slice(1) : amount;
  const [pounds = "", pence] = bare.split(".");
  const grouped = pounds.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  const sterling = pence === undefined ? `£${grouped}` : `£${grouped}.${pence}`;
  return negative ? `−${sterling}` : sterling;
}

/** "2025-01-05" → "5 Jan" — evidence chips, British order, no year. */
export function shortDate(date: IsoDate): string {
  const [, month = "", day = ""] = date.split("-");
  const name = MONTHS[Number(month) - 1] ?? "";
  return `${Number(day)} ${name.slice(0, 3)}`;
}

/** "2025-05" → "May" — price-rise copy. */
export function monthName(month: IsoMonth): string {
  const [, m = ""] = month.split("-");
  return MONTHS[Number(m) - 1] ?? "";
}
