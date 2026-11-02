import { describe, expect, test } from 'bun:test';
import type { GraphEdge, GraphNode, GraphSnapshot } from './api.ts';
import { mergePart } from './graph-part.ts';

const node = (id: string, extra: Partial<GraphNode> = {}): GraphNode => ({
  id,
  title: id,
  kind: 'note',
  reviewed: true,
  updated_at: 0,
  created_by: 'me',
  projects: [],
  tags: [],
  degree: 0,
  rank: 0.2,
  cluster: 1,
  x: 0,
  y: 0,
  pinned: false,
  ...extra,
});
const edge = (id: string, src: string, dst: string): GraphEdge => ({ id, src, dst, relation: 'related_to', origin: 'manual' });

const graph: GraphSnapshot = {
  view: 'all',
  nodes: [node('a', { degree: 2 }), node('b', { degree: 1, cluster: 2 }), node('c', { degree: 1, cluster: 2 }), node('d')],
  edges: [edge('ab', 'a', 'b'), edge('ac', 'a', 'c')],
};
const ids = (s: GraphSnapshot) => s.nodes.map((n) => n.id).sort();
const edgeIds = (s: GraphSnapshot) => s.edges.map((e) => e.id).sort();
const degrees = (s: GraphSnapshot) => Object.fromEntries(s.nodes.map((n) => [n.id, n.degree]));

describe('mergePart', () => {
  test('a changed entry keeps its rank and cluster, and gets its new links', () => {
    const part: GraphSnapshot = { view: 'all', nodes: [node('a', { title: 'A!', rank: 0, cluster: 0 })], edges: [edge('ab', 'a', 'b'), edge('ad', 'a', 'd')] };
    const merged = mergePart(graph, part, ['a']);
    const a = merged.nodes.find((n) => n.id === 'a')!;
    expect([a.title, a.rank, a.cluster]).toEqual(['A!', 0.2, 1]);
    expect(edgeIds(merged)).toEqual(['ab', 'ad']);
    expect(degrees(merged)).toEqual({ a: 2, b: 1, c: 0, d: 1 });
  });

  test('a touched entry missing from the part has left, with its links', () => {
    const merged = mergePart(graph, { view: 'all', nodes: [], edges: [] }, ['a']);
    expect(ids(merged)).toEqual(['b', 'c', 'd']);
    expect(merged.edges).toEqual([]);
    expect(degrees(merged)).toEqual({ b: 0, c: 0, d: 0 });
  });

  test('a new entry joins the cluster of most of its neighbours at an average size', () => {
    const part: GraphSnapshot = {
      view: 'all',
      nodes: [node('e', { rank: 0, cluster: 0 }), node('lonely', { rank: 0, cluster: 0 })],
      edges: [edge('eb', 'e', 'b'), edge('ec', 'e', 'c'), edge('ea', 'e', 'a')],
    };
    const merged = mergePart(graph, part, ['e', 'lonely']);
    const e = merged.nodes.find((n) => n.id === 'e')!;
    expect([e.cluster, e.rank, e.degree]).toEqual([2, 1 / 6, 3]);
    expect(merged.nodes.find((n) => n.id === 'lonely')!.cluster).toBe(0xffffffff);
    expect(degrees(merged)).toMatchObject({ a: 3, b: 2, c: 2 });
  });

  test('links to entries outside the view are left out, and untouched entries stay as they were', () => {
    const part: GraphSnapshot = { view: 'all', nodes: [node('d', { rank: 0 })], edges: [edge('dx', 'd', 'elsewhere')] };
    const merged = mergePart(graph, part, ['d']);
    expect(edgeIds(merged)).toEqual(['ab', 'ac']);
    expect(merged.nodes.find((n) => n.id === 'b')).toBe(graph.nodes[1]);
  });
});
