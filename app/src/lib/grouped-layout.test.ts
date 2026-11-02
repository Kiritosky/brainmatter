import { describe, expect, test } from 'bun:test';
import { assignGroups, groupedLayout, NO_PROJECT, type Circle, type LayoutInput, type LayoutNode, type LayoutResult } from './grouped-layout.ts';

const node = (id: string, ...projects: string[]): LayoutNode => ({ id, projects, x: null, y: null, pinned: false });
const many = (prefix: string, count: number, ...projects: string[]) =>
  Array.from({ length: count }, (_, i) => node(`${prefix}${i}`, ...projects));

function input(partial: Partial<LayoutInput>): LayoutInput {
  return { nodes: [], edges: [], projects: [], root: null, dirty: [], ...partial };
}

const allDirty = (i: LayoutInput): LayoutInput => ({ ...i, dirty: i.nodes.map((n) => n.id) });

function circle(result: LayoutResult, id: string): Circle {
  const found = result.circles.find((c) => c.id === id);
  if (!found) throw new Error(`no circle ${id}`);
  return found;
}

function position(result: LayoutResult, id: string) {
  const found = result.positions.find((p) => p.id === id);
  if (!found) throw new Error(`no position ${id}`);
  return found;
}

const distance = (a: { x: number; y: number }, b: { x: number; y: number }) => Math.hypot(a.x - b.x, a.y - b.y);

function expectApart(a: Circle, b: Circle) {
  expect(distance(a, b)).toBeGreaterThanOrEqual(a.r + b.r);
}

function expectInside(result: LayoutResult, id: string, c: Circle) {
  expect(distance(position(result, id), c)).toBeLessThanOrEqual(c.r);
}

/** Puts a layout's positions back in as the state the next run starts from. */
function settled(i: LayoutInput, result: LayoutResult): LayoutInput {
  return {
    ...i,
    dirty: [],
    nodes: i.nodes.map((n) => {
      const p = position(result, n.id);
      return { ...n, x: p.x, y: p.y };
    }),
  };
}

describe('groupedLayout', () => {
  const projects = [
    { id: 'a', parent: null },
    { id: 'b', parent: null },
    { id: 'c', parent: null },
  ];

  test('separate projects never overlap, even when heavily linked', () => {
    const nodes = [...many('a', 30, 'a'), ...many('b', 30, 'b'), ...many('c', 5, 'c')];
    // Every entry of a is linked to one of b: forces alone would merge them.
    const edges = Array.from({ length: 30 }, (_, i) => ({ src: `a${i}`, dst: `b${i}` }));
    const result = groupedLayout(allDirty(input({ nodes, edges, projects })));
    const [a, b, c] = ['a', 'b', 'c'].map((id) => circle(result, id));
    expectApart(a, b);
    expectApart(a, c);
    expectApart(b, c);
    for (const n of nodes) expectInside(result, n.id, circle(result, n.projects[0]));
  });

  test('linked projects end up nearer than unlinked ones', () => {
    const nodes = [...many('a', 20, 'a'), ...many('b', 20, 'b'), ...many('c', 20, 'c'), ...many('d', 20, 'd')];
    const edges = Array.from({ length: 20 }, (_, i) => ({ src: `a${i}`, dst: `d${i}` }));
    const result = groupedLayout(allDirty(input({ nodes, edges, projects: [...projects, { id: 'd', parent: null }] })));
    const near = distance(circle(result, 'a'), circle(result, 'd'));
    const far = Math.max(distance(circle(result, 'a'), circle(result, 'b')), distance(circle(result, 'a'), circle(result, 'c')));
    expect(near).toBeLessThan(far);
  });

  test('subprojects sit inside their parent and apart from each other', () => {
    const tree = [
      { id: 'root', parent: null },
      { id: 'one', parent: 'root' },
      { id: 'two', parent: 'root' },
      { id: 'deep', parent: 'two' },
      { id: 'other', parent: null },
    ];
    const nodes = [
      ...many('r', 3, 'root'),
      ...many('o', 12, 'one'),
      ...many('t', 12, 'two'),
      ...many('d', 6, 'deep'),
      ...many('x', 8, 'other'),
    ];
    const result = groupedLayout(allDirty(input({ nodes, projects: tree })));
    const root = circle(result, 'root');
    const one = circle(result, 'one');
    const two = circle(result, 'two');
    const deep = circle(result, 'deep');
    expect(distance(one, root) + one.r).toBeLessThanOrEqual(root.r);
    expect(distance(two, root) + two.r).toBeLessThanOrEqual(root.r);
    expect(distance(deep, two) + deep.r).toBeLessThanOrEqual(two.r);
    expectApart(one, two);
    expectApart(root, circle(result, 'other'));
    expect(one.parent).toBe('root');
    expect(deep.parent).toBe('two');
    expect(root.parent).toBeNull();
    expect(deep.depth).toBe(2);
  });

  test('a parent circle does not waste much room', () => {
    const tree = [
      { id: 'root', parent: null },
      { id: 'one', parent: 'root' },
      { id: 'two', parent: 'root' },
      { id: 'three', parent: 'root' },
    ];
    const nodes = [...many('r', 2, 'root'), ...many('o', 16, 'one'), ...many('t', 16, 'two'), ...many('h', 16, 'three')];
    // All hanging off the parent's own entries, which used to string them out in a line.
    const edges = [
      { src: 'r0', dst: 'o0' },
      { src: 'r0', dst: 't0' },
      { src: 'r1', dst: 'h0' },
    ];
    const result = groupedLayout(allDirty(input({ nodes, edges, projects: tree })));
    const root = circle(result, 'root');
    const inner = ['one', 'two', 'three'].map((id) => circle(result, id).r ** 2).reduce((a, b) => a + b);
    // Measured 0.37, with the gaps, the parent's own entries and room for the name.
    expect(inner / root.r ** 2).toBeGreaterThan(0.33);
  });

  test('entries without a project form their own group', () => {
    const nodes = [...many('a', 5, 'a'), ...many('n', 5)];
    const result = groupedLayout(allDirty(input({ nodes, projects: projects.slice(0, 1) })));
    const none = circle(result, NO_PROJECT);
    expectApart(circle(result, 'a'), none);
    expectInside(result, 'n0', none);
  });

  test('viewing a project draws its subprojects, not the project itself', () => {
    const tree = [
      { id: 'root', parent: null },
      { id: 'one', parent: 'root' },
      { id: 'two', parent: 'root' },
    ];
    const nodes = [...many('r', 4, 'root'), ...many('o', 4, 'one'), ...many('t', 4, 'two')];
    const result = groupedLayout(allDirty(input({ nodes, projects: tree, root: 'root' })));
    expect(result.circles.map((c) => c.id).sort()).toEqual(['one', 'two']);
    expect(result.positions.length).toBe(12);
    expect(circle(result, 'one').parent).toBeNull();
    expect(circle(result, 'one').depth).toBe(0);
  });

  test('a project without subprojects needs no circle when viewed alone', () => {
    const nodes = many('a', 6, 'a');
    const result = groupedLayout(allDirty(input({ nodes, projects: projects.slice(0, 1), root: 'a' })));
    expect(result.circles).toEqual([]);
    expect(result.positions.length).toBe(6);
  });

  test('without changes, entries keep their places relative to each other', () => {
    const nodes = [...many('a', 15, 'a'), ...many('b', 15, 'b')];
    const edges = [{ src: 'a0', dst: 'a1' }];
    const first = groupedLayout(allDirty(input({ nodes, edges, projects })));
    const again = groupedLayout(settled(input({ nodes, edges, projects }), first));
    expect(distance(position(again, 'a0'), position(again, 'a5'))).toBeCloseTo(distance(position(first, 'a0'), position(first, 'a5')), 5);
    expect(distance(position(again, 'a0'), position(again, 'b5'))).toBeCloseTo(distance(position(first, 'a0'), position(first, 'b5')), 5);
  });

  test('opening a view again moves nothing, with nested projects too', () => {
    const tree = [
      { id: 'a', parent: null },
      { id: 'a1', parent: 'a' },
      { id: 'a2', parent: 'a' },
      { id: 'a3', parent: 'a1' },
      { id: 'b', parent: null },
      { id: 'c', parent: null },
    ];
    const nodes = [...many('a', 8, 'a'), ...many('x', 12, 'a1'), ...many('y', 30, 'a2'), ...many('z', 5, 'a3'), ...many('b', 40, 'b'), ...many('c', 3, 'c')];
    const base = input({ nodes, edges: [{ src: 'a0', dst: 'b0' }], projects: tree });
    let result = groupedLayout(allDirty(base));
    // Once to arrive at what it keeps, and from then on nothing may change.
    result = groupedLayout(settled(base, result));
    for (let round = 0; round < 3; round++) {
      const next = groupedLayout(settled(base, result));
      for (const p of next.positions) expect(distance(p, position(result, p.id))).toBeLessThan(1e-3);
      for (const c of next.circles) expect(distance(c, circle(result, c.id))).toBeLessThan(1e-3);
      result = next;
    }
  });

  test('a new entry leaves the other projects where they were', () => {
    const base = input({ nodes: [...many('a', 15, 'a'), ...many('b', 15, 'b')], projects });
    const first = groupedLayout(allDirty(base));
    const next = settled(base, first);
    next.nodes.push(node('new', 'a'));
    next.dirty = ['new'];
    const after = groupedLayout(next);
    expect(distance(position(after, 'b0'), position(after, 'b9'))).toBeCloseTo(distance(position(first, 'b0'), position(first, 'b9')), 5);
    expectInside(after, 'new', circle(after, 'a'));
    expectApart(circle(after, 'a'), circle(after, 'b'));
  });

  test('the same input gives the same layout', () => {
    const i = allDirty(input({ nodes: [...many('a', 20, 'a'), ...many('b', 20, 'b')], edges: [{ src: 'a0', dst: 'b0' }], projects }));
    expect(groupedLayout(i)).toEqual(groupedLayout(i));
  });

  test('nothing at all is fine', () => {
    expect(groupedLayout(input({}))).toEqual({ positions: [], circles: [] });
  });
});

describe('assignGroups', () => {
  const projects = [
    { id: 'small', parent: null },
    { id: 'big', parent: null },
  ];

  test('an entry in two projects goes where its links are', () => {
    const nodes = [node('both', 'big', 'small'), node('s', 'small'), ...many('b', 3, 'big')];
    const groups = assignGroups(nodes, [{ src: 'both', dst: 's' }], projects, null);
    expect(groups.get('both')).toBe('small');
  });

  test('without links it goes to the larger project', () => {
    const nodes = [node('both', 'small', 'big'), ...many('b', 3, 'big')];
    expect(assignGroups(nodes, [], projects, null).get('both')).toBe('big');
  });

  test('projects outside the view do not count', () => {
    const tree = [...projects, { id: 'inner', parent: 'small' }];
    const nodes = [node('n', 'big', 'inner')];
    expect(assignGroups(nodes, [], tree, 'small').get('n')).toBe('inner');
  });
});

describe('scale', () => {
  test('a large graph stays apart and takes reasonable time', () => {
    const projects = Array.from({ length: 12 }, (_, i) => ({ id: `p${i}`, parent: i > 3 ? `p${i % 4}` : null }));
    const nodes = Array.from({ length: 5000 }, (_, i) => node(`n${i}`, `p${i % 12}`));
    const edges = Array.from({ length: 7000 }, (_, i) => ({ src: `n${i % 5000}`, dst: `n${(i * 7 + 13) % 5000}` }));
    const started = performance.now();
    const result = groupedLayout(allDirty(input({ nodes, edges, projects })));
    const elapsed = performance.now() - started;
    console.log(`5000 entries in 12 projects: ${Math.round(elapsed)} ms`);
    expect(result.positions.length).toBe(5000);
    const tops = result.circles.filter((c) => c.parent === null);
    for (const a of tops) for (const b of tops) if (a !== b) expectApart(a, b);
    // About 7 s on a Mac; CI runners are slower, and this only guards against quadratic work.
    expect(elapsed).toBeLessThan(40_000);
  }, 60_000);
});
