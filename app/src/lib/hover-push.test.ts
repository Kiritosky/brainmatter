import { describe, expect, test } from 'bun:test';
import { keepInside, pushAround } from './hover-push.ts';

const at = (id: string, x: number, y: number) => ({ id, x, y });

describe('pushAround', () => {
  const anchor = { x: 0, y: 0 };

  test('moves entries straight away from the anchor, nearer ones further', () => {
    const offsets = pushAround(anchor, [at('near', 10, 0), at('far', 0, 60)], 100, 40, 10);
    const near = offsets.get('near')!;
    const far = offsets.get('far')!;
    expect(near.x).toBeGreaterThan(0);
    expect(near.y).toBeCloseTo(0, 8);
    expect(far.y).toBeGreaterThan(0);
    expect(far.x).toBeCloseTo(0, 8);
    expect(near.x).toBeGreaterThan(far.y);
  });

  test('leaves everything outside the radius alone', () => {
    expect(pushAround(anchor, [at('out', 100, 0), at('further', 500, 0)], 100, 40, 10).size).toBe(0);
  });

  test('moves only the nearest when there are too many', () => {
    const nodes = [at('c', 30, 0), at('a', 10, 0), at('b', 20, 0)];
    expect([...pushAround(anchor, nodes, 100, 40, 2).keys()]).toEqual(['a', 'b']);
  });

  test('keeps entries in order on their way out', () => {
    const nodes = Array.from({ length: 99 }, (_, i) => at(`n${i}`, i + 1, 0));
    const offsets = pushAround(anchor, nodes, 100, 65, 99);
    let last = 0;
    for (const n of nodes) {
      const moved = n.x + offsets.get(n.id)!.x;
      expect(moved).toBeGreaterThan(last);
      last = moved;
    }
  });

  test('spreads entries lying exactly on the anchor', () => {
    const offsets = pushAround(anchor, [at('a', 0, 0), at('b', 0, 0)], 100, 40, 10);
    const [a, b] = [offsets.get('a')!, offsets.get('b')!];
    expect(Math.hypot(a.x - b.x, a.y - b.y)).toBeGreaterThan(1);
  });
});

describe('keepInside', () => {
  const room = { x: 0, y: 0, r: 100 };

  test('leaves an offset that stays inside', () => {
    expect(keepInside({ x: 50, y: 0 }, { x: 10, y: 0 }, room)).toEqual({ x: 10, y: 0 });
  });

  test('shortens one that would leave', () => {
    const offset = keepInside({ x: 90, y: 0 }, { x: 40, y: 0 }, room);
    expect(90 + offset.x).toBeCloseTo(100, 8);
  });
});
