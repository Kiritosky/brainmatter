import { describe, expect, test } from 'bun:test';
import type { GraphNode, GraphSnapshot } from './api.ts';
import { decodeGraph, encodeGraph, MAGIC } from './graph-wire.ts';

const node = (id: string, title: string, extra: Partial<GraphNode> = {}): GraphNode => ({
  id,
  title,
  kind: 'note',
  reviewed: true,
  updated_at: 1_793_600_000_000,
  created_by: 'me',
  projects: ['p'],
  tags: [],
  degree: 1,
  rank: 0.5,
  cluster: 0,
  x: 1.5,
  y: null,
  pinned: false,
  ...extra,
});

const snapshot: GraphSnapshot = {
  view: 'all',
  nodes: [node('a', 'Über 😀'), node('b', 'note', { tags: ['x', 'y'], projects: [], pinned: true, reviewed: false, x: null, y: -2.25, cluster: 3, rank: 0.25 })],
  edges: [
    { id: 'e', src: 'a', dst: 'b', relation: 'related_to', origin: 'manual' },
    { id: 'gone', src: 'a', dst: 'elsewhere', relation: 'related_to', origin: 'ai' },
  ],
};

describe('graph wire', () => {
  test('reads back what was written, without links that lead outside', () => {
    const back = decodeGraph(encodeGraph(snapshot));
    expect(back).toEqual({ ...snapshot, edges: [snapshot.edges[0]] });
  });

  test('matches the layout the Rust side writes', () => {
    // The same snapshot as the test in crates/bm-core/src/wire.rs, with the same numbers.
    const same: GraphSnapshot = {
      view: 'all',
      nodes: [node('a', 'Über 😀'), node('b', 'note')],
      edges: [
        { id: 'e', src: 'a', dst: 'b', relation: 'related_to', origin: 'manual' },
        { id: 'x', src: 'a', dst: 'gone', relation: 'related_to', origin: 'manual' },
      ],
    };
    const bytes = encodeGraph(same);
    const header = new Uint32Array(bytes, 0, 8);
    expect(header[0]).toBe(MAGIC);
    expect([header[2], header[3], header[4], header[5]]).toEqual([2, 1, 11, new TextEncoder().encode('allabÜber 😀notenotemeperelated_tomanual').length]);
    // The emoji takes two UTF-16 units, so the fourth string ends at 3 + 1 + 1 + 7.
    expect(new Uint32Array(bytes, 32, 4)[3]).toBe(12);
    expect(bytes.byteLength % 8).toBe(0);
    // The Rust test checks the same.
    let hash = 0;
    for (const b of new Uint8Array(bytes)) hash = (hash * 31 + b) >>> 0;
    expect([bytes.byteLength, hash]).toEqual([304, 533_818_328]);
  });

  test('an empty view, and data that is not aligned', () => {
    const empty = encodeGraph({ view: 'p', nodes: [], edges: [] });
    expect(decodeGraph(empty)).toEqual({ view: 'p', nodes: [], edges: [] });
    const bytes = new Uint8Array(encodeGraph(snapshot));
    const shifted = new Uint8Array(bytes.length + 3);
    shifted.set(bytes, 3);
    expect(decodeGraph(shifted.subarray(3)).nodes[0].title).toBe('Über 😀');
  });

  test('rejects anything else', () => {
    expect(() => decodeGraph(new Uint32Array(8).buffer)).toThrow();
  });
});
