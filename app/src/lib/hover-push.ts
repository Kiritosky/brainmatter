// How entries make room around the one under the pointer. Pure, in graph units.

export interface Point {
  x: number;
  y: number;
}

/**
 * Offsets that push entries away from `anchor`, most for the nearest. Below
 * `strength = radius * 2 / 3` entries never overtake each other on their way out.
 * Only the `limit` nearest move, so that a dense graph stays cheap.
 */
export function pushAround(
  anchor: Point,
  nodes: readonly (Point & { id: string })[],
  radius: number,
  strength: number,
  limit: number,
): Map<string, Point> {
  const near: { id: string; dx: number; dy: number; d: number }[] = [];
  for (const n of nodes) {
    const dx = n.x - anchor.x;
    const dy = n.y - anchor.y;
    const d = Math.hypot(dx, dy);
    if (d < radius) near.push({ id: n.id, dx, dy, d });
  }
  near.sort((a, b) => a.d - b.d);

  const offsets = new Map<string, Point>();
  near.slice(0, limit).forEach(({ id, dx, dy, d }, i) => {
    const push = strength * (1 - d / radius) ** 1.5;
    // Exactly on top of the anchor there is no direction, so spread them evenly.
    const angle = d > 0 ? Math.atan2(dy, dx) : i * 2.399963;
    offsets.set(id, { x: Math.cos(angle) * push, y: Math.sin(angle) * push });
  });
  return offsets;
}

/** Shortens `offset` so that `home + offset` stays within `room`. */
export function keepInside(home: Point, offset: Point, room: Point & { r: number }): Point {
  const x = home.x + offset.x - room.x;
  const y = home.y + offset.y - room.y;
  const d = Math.hypot(x, y);
  if (d <= room.r) return offset;
  const scale = room.r / d;
  return { x: room.x + x * scale - home.x, y: room.y + y * scale - home.y };
}
