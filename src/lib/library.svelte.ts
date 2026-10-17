// Paged view of the library for the virtual gallery: pages are fetched when
// they scroll into view and dropped when the query changes. Answers to an
// outdated query are ignored.

import { SvelteMap } from "svelte/reactivity";
import { queryMedia } from "./api";
import type { MediaQuery, MediaView } from "./types";

export type LibraryQuery = Omit<MediaQuery, "offset" | "limit">;

export class PagedLibrary {
  readonly pageSize: number;
  readonly pages = new SvelteMap<number, MediaView[]>();
  total = $state(0);
  /** True until the first page of the current query has arrived. */
  loading = $state(true);
  #query: LibraryQuery | null = null;
  #generation = 0;
  #inFlight = new Set<number>();

  constructor(pageSize = 120) {
    this.pageSize = pageSize;
  }

  /** Starts a new query; the first page loads right away. */
  async reset(query: LibraryQuery): Promise<void> {
    this.#query = query;
    this.#generation += 1;
    this.#inFlight.clear();
    this.loading = true;
    await this.#fetch(0, true);
  }

  /** Makes sure items `start..end` are loaded (or loading). */
  ensure(start: number, end: number): void {
    if (!this.#query || end <= start) return;
    const first = Math.floor(start / this.pageSize);
    const last = Math.floor(Math.max(start, end - 1) / this.pageSize);
    for (let p = first; p <= last; p += 1) {
      if (p * this.pageSize >= this.total) break;
      if (!this.pages.has(p) && !this.#inFlight.has(p)) void this.#fetch(p, false);
    }
  }

  item(index: number): MediaView | undefined {
    return this.pages.get(Math.floor(index / this.pageSize))?.[index % this.pageSize];
  }

  find(id: number): MediaView | undefined {
    for (const page of this.pages.values()) {
      const hit = page.find((m) => m.id === id);
      if (hit) return hit;
    }
    return undefined;
  }

  /** Reloads the pages already on screen (after a scan or an edit elsewhere). */
  async refreshLoaded(): Promise<void> {
    const loaded = [...this.pages.keys()];
    if (loaded.length === 0) return this.reset(this.#query ?? { section: "all", kind: "all", search: "", sort: "date" });
    await Promise.all(loaded.map((p) => this.#fetch(p, false, true)));
  }

  /** Puts edited rows in place without refetching. */
  update(rows: MediaView[]): void {
    const byId = new Map(rows.map((r) => [r.id, r]));
    for (const [p, page] of this.pages) {
      if (page.some((m) => byId.has(m.id))) this.pages.set(p, page.map((m) => byId.get(m.id) ?? m));
    }
  }

  async #fetch(page: number, replaceAll: boolean, force = false): Promise<void> {
    if (!this.#query || (!force && this.#inFlight.has(page))) return;
    const generation = this.#generation;
    this.#inFlight.add(page);
    try {
      const result = await queryMedia({ ...this.#query, offset: page * this.pageSize, limit: this.pageSize });
      if (generation !== this.#generation) return;
      if (replaceAll) this.pages.clear();
      this.total = result.total;
      this.pages.set(page, result.items);
    } finally {
      if (generation === this.#generation) {
        this.#inFlight.delete(page);
        if (page === 0) this.loading = false;
      }
    }
  }
}
