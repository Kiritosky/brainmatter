// Reads the binary graph from `crates/bm-core/src/wire.rs`, which describes the layout.
import type { GraphEdge, GraphNode, GraphSnapshot } from './api.ts';

export const MAGIC = 0x31474d42; // "BMG1"
export const VERSION = 1;

export function decodeGraph(data: ArrayBuffer | Uint8Array): GraphSnapshot {
  // Typed arrays need their sections aligned, which a view into a larger buffer may not be.
  const buffer = data instanceof ArrayBuffer ? data : (data.slice().buffer as ArrayBuffer);
  let at = 0;
  const pad = () => (at = Math.ceil(at / 8) * 8);
  const u32 = (count: number) => {
    const out = new Uint32Array(buffer, at, count);
    at += count * 4;
    pad();
    return out;
  };
  const f64 = (count: number) => {
    const out = new Float64Array(buffer, at, count);
    at += count * 8;
    return out;
  };
  const f32 = (count: number) => {
    const out = new Float32Array(buffer, at, count);
    at += count * 4;
    pad();
    return out;
  };

  const [magic, version, nodeCount, edgeCount, stringCount, textBytes, view] = u32(8);
  if (magic !== MAGIC || version !== VERSION) throw new Error(`unknown graph format ${magic.toString(16)} v${version}`);

  const ends = u32(stringCount);
  const text = new TextDecoder().decode(new Uint8Array(buffer, at, textBytes));
  at += textBytes;
  pad();
  const strings = new Array<string>(stringCount);
  let start = 0;
  for (let i = 0; i < stringCount; i++) {
    strings[i] = text.slice(start, ends[i]);
    start = ends[i];
  }

  const xs = f64(nodeCount);
  const ys = f64(nodeCount);
  const updated = f64(nodeCount);
  const ranks = f32(nodeCount);
  const ids = u32(nodeCount);
  const titles = u32(nodeCount);
  const kinds = u32(nodeCount);
  const authors = u32(nodeCount);
  const degrees = u32(nodeCount);
  const clusters = u32(nodeCount);
  const flags = u32(nodeCount);
  const projectEnds = u32(nodeCount);
  const projects = u32(projectEnds[nodeCount - 1] ?? 0);
  const tagEnds = u32(nodeCount);
  const tags = u32(tagEnds[nodeCount - 1] ?? 0);

  const nodes = new Array<GraphNode>(nodeCount);
  let project = 0;
  let tag = 0;
  for (let i = 0; i < nodeCount; i++) {
    const nodeProjects: string[] = [];
    for (; project < projectEnds[i]; project++) nodeProjects.push(strings[projects[project]]);
    const nodeTags: string[] = [];
    for (; tag < tagEnds[i]; tag++) nodeTags.push(strings[tags[tag]]);
    nodes[i] = {
      id: strings[ids[i]],
      title: strings[titles[i]],
      kind: strings[kinds[i]],
      reviewed: (flags[i] & 1) !== 0,
      updated_at: updated[i],
      created_by: strings[authors[i]],
      projects: nodeProjects,
      tags: nodeTags,
      degree: degrees[i],
      rank: ranks[i],
      cluster: clusters[i],
      x: Number.isNaN(xs[i]) ? null : xs[i],
      y: Number.isNaN(ys[i]) ? null : ys[i],
      pinned: (flags[i] & 2) !== 0,
    };
  }

  const edgeIds = u32(edgeCount);
  const relations = u32(edgeCount);
  const origins = u32(edgeCount);
  const sources = u32(edgeCount);
  const targets = u32(edgeCount);
  const edges = new Array<GraphEdge>(edgeCount);
  for (let i = 0; i < edgeCount; i++) {
    edges[i] = {
      id: strings[edgeIds[i]],
      src: nodes[sources[i]].id,
      dst: nodes[targets[i]].id,
      relation: strings[relations[i]],
      origin: strings[origins[i]] as GraphEdge['origin'],
    };
  }
  return { view: strings[view], nodes, edges };
}

/** Writes the same format; the app gets it from Rust, this is for tests and the mock. */
export function encodeGraph(snapshot: GraphSnapshot): ArrayBuffer {
  const index = new Map<string, number>();
  const list: string[] = [];
  const push = (s: string) => list.push(s) - 1;
  const add = (s: string) => {
    let i = index.get(s);
    if (i === undefined) {
      i = push(s);
      index.set(s, i);
    }
    return i;
  };
  const { nodes, edges } = snapshot;
  const position = new Map(nodes.map((n, i) => [n.id, i]));
  const view = add(snapshot.view);
  // Ids and titles are nearly all different: no lookup, like the Rust side.
  const ids = nodes.map((n) => push(n.id));
  const titles = nodes.map((n) => push(n.title));
  const kinds = nodes.map((n) => add(n.kind));
  const authors = nodes.map((n) => add(n.created_by));
  const projects = nodes.flatMap((n) => n.projects.map(add));
  const tags = nodes.flatMap((n) => n.tags.map(add));
  const kept = edges.filter((e) => position.has(e.src) && position.has(e.dst));
  const edgeIds = kept.map((e) => push(e.id));
  const relations = kept.map((e) => add(e.relation));
  const origins = kept.map((e) => add(e.origin));
  const text = new TextEncoder().encode(list.join(''));

  const parts: ArrayBuffer[] = [];
  const padded = (bytes: Uint8Array) => {
    const out = new Uint8Array(Math.ceil(bytes.length / 8) * 8);
    out.set(bytes);
    parts.push(out.buffer);
  };
  const u32 = (values: number[]) => padded(new Uint8Array(new Uint32Array(values).buffer));
  const f64 = (values: number[]) => padded(new Uint8Array(new Float64Array(values).buffer));
  const f32 = (values: number[]) => padded(new Uint8Array(new Float32Array(values).buffer));
  let end = 0;
  const ends = (lengths: number[]) => lengths.map((l) => (end += l));

  u32([MAGIC, VERSION, nodes.length, kept.length, list.length, text.length, view, 0]);
  u32(ends(list.map((s) => s.length)));
  padded(text);
  f64(nodes.map((n) => n.x ?? NaN));
  f64(nodes.map((n) => n.y ?? NaN));
  f64(nodes.map((n) => n.updated_at));
  f32(nodes.map((n) => n.rank));
  u32(ids);
  u32(titles);
  u32(kinds);
  u32(authors);
  u32(nodes.map((n) => n.degree));
  u32(nodes.map((n) => n.cluster));
  u32(nodes.map((n) => (n.reviewed ? 1 : 0) | (n.pinned ? 2 : 0)));
  end = 0;
  u32(ends(nodes.map((n) => n.projects.length)));
  u32(projects);
  end = 0;
  u32(ends(nodes.map((n) => n.tags.length)));
  u32(tags);
  u32(edgeIds);
  u32(relations);
  u32(origins);
  u32(kept.map((e) => position.get(e.src)!));
  u32(kept.map((e) => position.get(e.dst)!));

  const out = new Uint8Array(parts.reduce((sum, p) => sum + p.byteLength, 0));
  let at = 0;
  for (const p of parts) {
    out.set(new Uint8Array(p), at);
    at += p.byteLength;
  }
  return out.buffer;
}
