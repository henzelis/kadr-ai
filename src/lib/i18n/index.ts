// The UI language is chosen once, before the page renders: the user's
// choice (Settings → Interface language) or else the system language.
// Changing it reloads the window.

import { en } from "./en";
import { uk, type Strings } from "./uk";
import type { Note } from "$lib/types";

export type Lang = "uk" | "en";
const KEY = "kadr.lang";

function systemLang(): Lang {
  const langs = typeof navigator === "undefined" ? [] : navigator.languages ?? [navigator.language];
  return langs.some((l) => /^(uk|ru|be)\b/i.test(l)) ? "uk" : "en";
}

export function storedLangChoice(): "auto" | Lang {
  try {
    const v = localStorage.getItem(KEY);
    return v === "uk" || v === "en" ? v : "auto";
  } catch {
    return "auto";
  }
}

export function setLangChoice(choice: "auto" | Lang): void {
  try {
    if (choice === "auto") localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, choice);
  } catch {
    // storage unavailable: the choice lasts until the window closes
  }
}

const choice = storedLangChoice();
export const lang: Lang = choice === "auto" ? systemLang() : choice;
export const t: Strings = lang === "en" ? en : uk;

if (typeof document !== "undefined") document.documentElement.lang = lang;

/** Text for a note sent by the core (code + parameters). */
export function noteText(note: Note): string {
  return t.notes[note.code]?.(note.args ?? {}) ?? note.args?.text ?? note.code;
}
