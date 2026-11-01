/**
 * Lays the graph out by project: every project is a circle, subprojects sit inside
 * their parent, and no two circles overlap, however the entries are linked.
 *
 * Forces alone can't promise that, since links would pull projects into each other.
 * So each project's own entries are laid out on their own, using only the links among
 * them, and the circles are then packed. Links across projects only decide which
 * circles end up next to each other.
 *
 * Pure and free of the DOM: it runs in a worker, and in tests.
 */
import Graph from 'graphology';
import forceAtlas2 from 'graphology-layout-forceatlas2';

/** The group of entries that are in no project. */
export const NO_PROJECT = '';

export interface LayoutProject {
  id: string;
  parent: string | null;
}

export interface LayoutNode {
  id: string;
  projects: string[];
  x: number | null;
  y: number | null;
  pinned: boolean;
}

export interface LayoutInput {
  nodes: LayoutNode[];
  edges: { src: string; dst: string }[];
  projects: LayoutProject[];
  /** The project being viewed; `null` for everything. It gets no circle of its own. */
  root: string | null;
  /** Entries that are new here. The rest keep their place relative to each other. */
  dirty: string[];
}

export interface Circle {
  id: string;
  parent: string | null;
  depth: number;
  x: number;
  y: number;
  r: number;
}

export interface LayoutResult {
  positions: { id: string; x: number; y: number }[];
  circles: Circle[];
}

const DENSITY = 13;
const MIN_RADIUS = 16;
const NODE_PADDING = 12;
const GOLDEN_ANGLE = 2.39996;

/** Above this share of new entries, a group is laid out from scratch. */
const REDO_SHARE = 0.3;

function subtreeOf(projects: LayoutProject[], root: string): Set<string> {
  const out = new Set([root]);
  let grew = true;
  while (grew) {
    grew = false;
    for (const p of projects) {
      if (p.parent && out.has(p.parent) && !out.has(p.id)) {
        out.add(p.id);
        grew = true;
      }
    }
  }
  return out;
}

/**
 * Which group each entry is drawn in. An entry can be in several projects but can only
 * be drawn once, so it goes where most of its linked entries are. Ties go to the larger
 * project, then to the lower id, so that the answer doesn't change between runs.
 */
export function assignGroups(
  nodes: Pick<LayoutNode, 'id' | 'projects'>[],
  edges: { src: string; dst: string }[],
  projects: LayoutProject[],
  root: string | null,
): Map<string, string> {
  const scope = root ? subtreeOf(projects, root) : new Set(projects.map((p) => p.id));
  const memberships = new Map<string, string[]>();
  const sizes = new Map<string, number>();
  for (const n of nodes) {
    const own = n.projects.filter((p) => scope.has(p));
    memberships.set(n.id, own);
    for (const p of own) sizes.set(p, (sizes.get(p) ?? 0) + 1);
  }

  const shared = nodes.filter((n) => (memberships.get(n.id)?.length ?? 0) > 1);
  const votes = new Map<string, Map<string, number>>();
  if (shared.length) {
    const wanted = new Set(shared.map((n) => n.id));
    const vote = (id: string, other: string) => {
      if (!wanted.has(id)) return;
      const tally = votes.get(id) ?? new Map<string, number>();
      votes.set(id, tally);
      for (const p of memberships.get(other) ?? []) tally.set(p, (tally.get(p) ?? 0) + 1);
    };
    for (const e of edges) {
      vote(e.src, e.dst);
      vote(e.dst, e.src);
    }
  }

  const out = new Map<string, string>();
  for (const n of nodes) {
    const own = memberships.get(n.id) ?? [];
    if (own.length < 2) {
      out.set(n.id, own[0] ?? NO_PROJECT);
      continue;
    }
    const tally = votes.get(n.id);
    const score = (p: string) => tally?.get(p) ?? 0;
    const best = [...own].sort(
      (a, b) => score(b) - score(a) || (sizes.get(b) ?? 0) - (sizes.get(a) ?? 0) || a.localeCompare(b),
    )[0];
    out.set(n.id, best);
  }
  return out;
}

/** Something to place: a subproject, or a project's own entries. */
interface Item {
  key: string;
  r: number;
  /** Relative to the item's centre. */
  nodes: { id: string; x: number; y: number }[];
  circles: Circle[];
  /** Where it was before, if it has been here before. */
  before: { x: number; y: number } | null;
  x: number;
  y: number;
}

function centroid(points: { x: number; y: number }[]) {
  let x = 0;
  let y = 0;
  for (const p of points) {
    x += p.x;
    y += p.y;
  }
  return { x: x / points.length, y: y / points.length };
}

function radiusFor(count: number) {
  return Math.max(MIN_RADIUS, DENSITY * Math.sqrt(count));
}

/** Exact forces cost the square of the entries, so larger groups use an approximation. */
const APPROXIMATE_FROM = 150;

function iterationsFor(count: number) {
  if (count < 50) return 300;
  if (count < APPROXIMATE_FROM) return 200;
  return count < 1000 ? 60 : 40;
}

/** A project's own entries, using only the links among them. */
function layOutEntries(
  ids: string[],
  byId: Map<string, LayoutNode>,
  edges: { src: string; dst: string }[],
  dirty: Set<string>,
): Pick<Item, 'nodes' | 'r' | 'before'> {
  const settled = ids.filter((id) => !dirty.has(id) && byId.get(id)?.x != null);
  const placed = (id: string) => {
    const n = byId.get(id)!;
    return { x: n.x as number, y: n.y as number };
  };
  const before = settled.length ? centroid(settled.map(placed)) : null;

  if (ids.length === 1) {
    return { nodes: [{ id: ids[0], x: 0, y: 0 }], r: MIN_RADIUS, before };
  }

  // Nothing new: keep it exactly as it is.
  if (settled.length === ids.length) {
    const nodes = ids.map((id) => {
      const p = placed(id);
      return { id, x: p.x - before!.x, y: p.y - before!.y };
    });
    const reach = Math.max(...nodes.map((n) => Math.hypot(n.x, n.y)));
    return { nodes, r: Math.max(MIN_RADIUS, reach + NODE_PADDING), before };
  }

  const redo = settled.length < ids.length * (1 - REDO_SHARE);
  const graph = new Graph({ type: 'undirected', multi: false });
  ids.forEach((id, i) => {
    const n = byId.get(id)!;
    const keep = !redo && n.x != null && n.y != null;
    // A sunflower: spread evenly, and the same on every run.
    const spread = DENSITY * Math.sqrt(i + 1);
    graph.addNode(id, {
      x: keep ? (n.x as number) : spread * Math.cos(i * GOLDEN_ANGLE),
      y: keep ? (n.y as number) : spread * Math.sin(i * GOLDEN_ANGLE),
      fixed: !redo && n.pinned,
    });
  });
  for (const e of edges) {
    if (e.src !== e.dst && graph.hasNode(e.src) && graph.hasNode(e.dst)) graph.mergeEdge(e.src, e.dst);
  }
  forceAtlas2.assign(graph, {
    iterations: redo ? iterationsFor(ids.length) : 40,
    settings: {
      ...forceAtlas2.inferSettings(graph),
      barnesHutOptimize: ids.length >= APPROXIMATE_FROM,
      // Keeps entries without links in the disc rather than drifting off.
      strongGravityMode: true,
      gravity: 0.6,
      slowDown: redo ? 2 : 8,
    },
  });

  const raw = ids.map((id) => {
    const a = graph.getNodeAttributes(id);
    return { id, x: a.x as number, y: a.y as number };
  });
  const middle = centroid(raw);
  const reach = Math.max(...raw.map((n) => Math.hypot(n.x - middle.x, n.y - middle.y)));
  const target = radiusFor(ids.length);
  const scale = reach > 0 ? target / reach : 1;
  return {
    nodes: raw.map((n) => ({ id: n.id, x: (n.x - middle.x) * scale, y: (n.y - middle.y) * scale })),
    r: target + NODE_PADDING,
    before,
  };
}

/** How many free spots a new group chooses between. */
const COMPACT_TRIES = 24;

const gapBetween = (a: number, b: number) => 8 + 0.12 * Math.min(a, b);

/**
 * Sets `x` and `y` of each item. What was here before stays as near to where it was as
 * possible. Anything new goes next to what it is linked with.
 */
function pack(items: Item[], links: Map<string, Map<string, number>>) {
  const placed: Item[] = [];
  const fits = (item: Item, x: number, y: number) =>
    placed.every((o) => Math.hypot(o.x - x, o.y - y) >= o.r + item.r + gapBetween(o.r, item.r));
  /** Walks outwards from the wanted spot. Of the first free spots, `cost` picks one. */
  const place = (item: Item, wantX: number, wantY: number, tries: number, cost: (x: number, y: number) => number) => {
    const step = Math.max(3, item.r * 0.2);
    let best: { x: number; y: number; cost: number } | null = null;
    let found = 0;
    for (let k = 0; found < tries; k++) {
      const distance = step * Math.sqrt(k);
      const x = wantX + distance * Math.cos(k * GOLDEN_ANGLE);
      const y = wantY + distance * Math.sin(k * GOLDEN_ANGLE);
      if (!fits(item, x, y)) continue;
      found++;
      const c = cost(x, y);
      if (!best || c < best.cost) best = { x, y, cost: c };
    }
    item.x = best!.x;
    item.y = best!.y;
    placed.push(item);
  };

  const bySize = (a: Item, b: Item) => b.r - a.r || a.key.localeCompare(b.key);
  const known = items.filter((i) => i.before).sort(bySize);
  const origin = known.length ? centroid(known.map((i) => i.before!)) : { x: 0, y: 0 };
  for (const item of known) place(item, item.before!.x - origin.x, item.before!.y - origin.y, 1, () => 0);

  const fresh = items.filter((i) => !i.before).sort(bySize);
  while (fresh.length) {
    let pick = 0;
    let most = -1;
    fresh.forEach((item, i) => {
      const own = links.get(item.key);
      const weight = placed.reduce((sum, o) => sum + (own?.get(o.key) ?? 0), 0);
      if (weight > most) {
        most = weight;
        pick = i;
      }
    });
    const [item] = fresh.splice(pick, 1);
    const own = links.get(item.key);
    let x = 0;
    let y = 0;
    let weight = 0;
    for (const o of placed) {
      const w = own?.get(o.key) ?? 0;
      x += o.x * w;
      y += o.y * w;
      weight += w;
    }
    const wantX = weight ? x / weight : 0;
    const wantY = weight ? y / weight : 0;
    // Near what it is linked with, but towards the middle of the others: taking the
    // nearest free spot alone strings groups out in a line, around a lot of empty room.
    const middle = placed.length ? centroid(placed) : { x: 0, y: 0 };
    place(item, wantX, wantY, COMPACT_TRIES, (px, py) => Math.hypot(px - middle.x, py - middle.y) + 0.35 * Math.hypot(px - wantX, py - wantY));
  }
}

/** A small circle around circles: walk towards whichever is furthest out. */
function enclose(items: Item[]) {
  let x = items[0].x;
  let y = items[0].y;
  for (let i = 1; i <= 60; i++) {
    let far = items[0];
    let reach = -1;
    for (const item of items) {
      const d = Math.hypot(item.x - x, item.y - y) + item.r;
      if (d > reach) {
        reach = d;
        far = item;
      }
    }
    x += (far.x - x) / (i + 1);
    y += (far.y - y) / (i + 1);
  }
  const r = Math.max(...items.map((item) => Math.hypot(item.x - x, item.y - y) + item.r));
  return { x, y, r };
}

export function groupedLayout(input: LayoutInput): LayoutResult {
  const { nodes, edges, projects, root } = input;
  const dirty = new Set(input.dirty);
  const groups = assignGroups(nodes, edges, projects, root);
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const scope = root ? subtreeOf(projects, root) : new Set(projects.map((p) => p.id));

  const own = new Map<string, string[]>();
  for (const n of nodes) {
    const group = groups.get(n.id)!;
    const list = own.get(group);
    if (list) list.push(n.id);
    else own.set(group, [n.id]);
  }
  const children = new Map<string | null, string[]>();
  for (const p of projects) {
    if (!scope.has(p.id) || p.id === root) continue;
    const parent = p.parent && scope.has(p.parent) ? p.parent : null;
    const key = parent === root ? null : parent;
    children.set(key, [...(children.get(key) ?? []), p.id]);
  }

  /** For packing: links between the items of one level, found through their entries. */
  function linksBetween(items: Item[]) {
    const itemOf = new Map<string, string>();
    for (const item of items) for (const n of item.nodes) itemOf.set(n.id, item.key);
    const out = new Map<string, Map<string, number>>();
    const add = (a: string, b: string) => {
      const row = out.get(a) ?? new Map<string, number>();
      out.set(a, row);
      row.set(b, (row.get(b) ?? 0) + 1);
    };
    for (const e of edges) {
      const a = itemOf.get(e.src);
      const b = itemOf.get(e.dst);
      if (a && b && a !== b) {
        add(a, b);
        add(b, a);
      }
    }
    return out;
  }

  function entriesItem(key: string, ids: string[]): Item {
    return { key, circles: [], x: 0, y: 0, ...layOutEntries(ids, byId, edges, dirty) };
  }

  /** `level` is `null` at the top, where the view itself draws no circle. */
  function solve(level: string | null, depth: number): Item | null {
    const items: Item[] = [];
    for (const child of children.get(level) ?? []) {
      const item = solve(child, depth + 1);
      if (item) items.push(item);
    }
    const ownKey = level ?? root ?? NO_PROJECT;
    const ownIds = own.get(ownKey) ?? [];
    if (ownIds.length) {
      const item = entriesItem(`own:${ownKey}`, ownIds);
      // At the top, entries outside any project are a group like the others.
      if (level === null && !root) {
        item.circles.push({ id: NO_PROJECT, parent: null, depth: 0, x: 0, y: 0, r: item.r + 8 });
        item.r += 8;
      }
      items.push(item);
    }
    if (!items.length) return null;

    if (items.length > 1) pack(items, linksBetween(items));
    else {
      items[0].x = 0;
      items[0].y = 0;
    }
    const box = items.length > 1 ? enclose(items) : { x: 0, y: 0, r: items[0].r };

    const nodesOut = items.flatMap((item) =>
      item.nodes.map((n) => ({ id: n.id, x: n.x + item.x - box.x, y: n.y + item.y - box.y })),
    );
    const circlesOut = items.flatMap((item) =>
      item.circles.map((c) => ({ ...c, x: c.x + item.x - box.x, y: c.y + item.y - box.y })),
    );
    const known = items.filter((i) => i.before).map((i) => i.before!);
    const before = known.length ? centroid(known) : null;

    if (level === null) {
      return { key: 'top', r: box.r, nodes: nodesOut, circles: circlesOut, before, x: 0, y: 0 };
    }
    // Room for the name at the top of the circle.
    const r = box.r + 8 + 0.04 * box.r;
    const parent = projects.find((p) => p.id === level)?.parent ?? null;
    circlesOut.push({
      id: level,
      parent: parent && scope.has(parent) && parent !== root ? parent : null,
      depth,
      x: 0,
      y: 0,
      r,
    });
    return { key: level, r, nodes: nodesOut, circles: circlesOut, before, x: 0, y: 0 };
  }

  const top = solve(null, -1);
  return { positions: top?.nodes ?? [], circles: top?.circles ?? [] };
}
