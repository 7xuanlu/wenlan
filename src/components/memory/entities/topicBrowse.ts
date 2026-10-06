// SPDX-License-Identifier: AGPL-3.0-only
import { queryEntities, type Entity } from "../../../lib/tauri";
import { ENTITIES_PAGE_SIZE, entityListRequest, type EntityFilters } from "./entitiesViewModel";

type ActiveStatus = "detected" | "established";
interface TopicStream {
  readonly buffer: readonly Entity[];
  readonly offset: number;
  readonly total: number;
  readonly exhausted: boolean;
}
export type TopicCursor = Readonly<Record<ActiveStatus, TopicStream>>;
const STATUSES: readonly ActiveStatus[] = ["detected", "established"];
const emptyStream = (): TopicStream => ({ buffer: [], offset: 0, total: 0, exhausted: false });

// Keep one row per ID while preserving existing list positions. Lifecycle
// reads can observe different versions of the same topic during confirmation.
export function mergeTopicRows(current: readonly Entity[], incoming: readonly Entity[]): Entity[] {
  const rows = new Map<string, Entity>();
  for (const entity of [...current, ...incoming]) {
    const previous = rows.get(entity.id);
    if (!previous || entity.updated_at > previous.updated_at
      || (entity.updated_at === previous.updated_at
        && entity.status === "established" && previous.status !== "established")) {
      rows.set(entity.id, entity);
    }
  }
  return [...rows.values()];
}

/** Read-only union of the two active lifecycles. The API's omitted status
 * includes archived topics, so it cannot represent the ordinary browse view.
 * At most two bounded requests per batch; neither read confirms a topic. */
export async function loadActiveTopicsPage(filters: EntityFilters, cursor?: TopicCursor) {
  const previous = cursor ?? { detected: emptyStream(), established: emptyStream() };
  const streams = await Promise.all(STATUSES.map(async (status) => {
    const stream = previous[status];
    if (stream.exhausted || stream.buffer.length >= ENTITIES_PAGE_SIZE) return stream;
    const response = await queryEntities(entityListRequest(status, filters, stream.offset));
    const offset = stream.offset + response.entities.length;
    return {
      buffer: [...stream.buffer, ...response.entities],
      offset,
      total: response.total,
      exhausted: response.entities.length === 0 || offset >= response.total,
    } satisfies TopicStream;
  }));
  // Dedupe the whole bounded buffer before taking a page, so stale versions
  // cannot consume a unique row slot or survive in the cursor after emission.
  const entities = mergeTopicRows([], [...streams[0].buffer, ...streams[1].buffer])
    .sort((left, right) => right.updated_at - left.updated_at
      || (left.id < right.id ? -1 : left.id > right.id ? 1 : 0))
    .slice(0, ENTITIES_PAGE_SIZE);
  const emittedIds = new Set(entities.map(entity => entity.id));
  const next: TopicCursor = {
    detected: { ...streams[0], buffer: streams[0].buffer.filter(entity => !emittedIds.has(entity.id)) },
    established: { ...streams[1], buffer: streams[1].buffer.filter(entity => !emittedIds.has(entity.id)) },
  };
  return {
    entities,
    total: streams[0].total + streams[1].total,
    cursor: next,
    hasMore: STATUSES.some(status => next[status].buffer.length > 0 || !next[status].exhausted),
  };
}
