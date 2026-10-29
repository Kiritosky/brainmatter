// Spike 3: can WKWebView + Sigma render Brainmatter-sized graphs smoothly?
import Graph from 'graphology';
import Sigma from 'sigma';
import forceAtlas2 from 'graphology-layout-forceatlas2';
import FA2Layout from 'graphology-layout-forceatlas2/worker';

export interface Scenario {
  name: string;
  nodes: number;
  edges: number;
  /** Run ForceAtlas2 in a web worker while measuring. */
  liveLayout: boolean;
  hideEdgesOnMove: boolean;
}

export interface ScenarioResult {
  name: string;
  nodes: number;
  edges: number;
  buildMs: number;
  firstRenderMs: number;
  fps: number;
  frameP95Ms: number;
  frameMaxMs: number;
}

const PALETTE = [
  '#7c9cff', '#ff8a7a', '#5fd3a5', '#f5c451', '#c38bff',
  '#4cc9f0', '#ff6fb5', '#9bd65a', '#ffa24c', '#8f9bb3',
];

function mulberry32(seed: number) {
  return () => {
    seed |= 0;
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Clustered random graph that resembles a topic-structured knowledge base. */
export function clusteredGraph(n: number, m: number, clusters = 40, seed = 1): Graph {
  const rand = mulberry32(seed);
  const gauss = () => Math.sqrt(-2 * Math.log(rand() || 1e-9)) * Math.cos(2 * Math.PI * rand());
  const graph = new Graph({ type: 'undirected' });
  const members: string[][] = Array.from({ length: clusters }, () => []);
  const centers = Array.from({ length: clusters }, () => [gauss() * 1000, gauss() * 1000]);

  for (let i = 0; i < n; i++) {
    const c = Math.floor(rand() * clusters);
    const key = String(i);
    members[c].push(key);
    graph.addNode(key, {
      x: centers[c][0] + gauss() * 120,
      y: centers[c][1] + gauss() * 120,
      size: 1.5 + rand() * 3,
      color: PALETTE[c % PALETTE.length],
      label: `Eintrag ${i}`,
      cluster: c,
    });
  }

  let added = 0;
  while (added < m) {
    const u = String(Math.floor(rand() * n));
    const own = members[graph.getNodeAttribute(u, 'cluster') as number];
    const v = rand() < 0.85 ? own[Math.floor(rand() * own.length)] : String(Math.floor(rand() * n));
    if (u === v || graph.hasEdge(u, v)) continue;
    graph.addEdge(u, v, { size: 0.5, color: '#3a4256' });
    added++;
  }
  return graph;
}

const nextFrame = () => new Promise<number>((r) => requestAnimationFrame(r));

async function measureFrames(durationMs: number) {
  const deltas: number[] = [];
  let last = await nextFrame();
  const end = last + durationMs;
  while (last < end) {
    const now = await nextFrame();
    deltas.push(now - last);
    last = now;
  }
  deltas.sort((a, b) => a - b);
  const total = deltas.reduce((s, d) => s + d, 0);
  return {
    fps: Math.round((deltas.length / total) * 1000 * 10) / 10,
    frameP95Ms: Math.round(deltas[Math.floor(deltas.length * 0.95)] * 10) / 10,
    frameMaxMs: Math.round(deltas[deltas.length - 1] * 10) / 10,
  };
}

/** Display refresh rate as seen by requestAnimationFrame with nothing rendering. */
export async function baselineFps(): Promise<number> {
  return (await measureFrames(1000)).fps;
}

export async function runScenario(container: HTMLElement, s: Scenario): Promise<ScenarioResult> {
  let t = performance.now();
  const graph = clusteredGraph(s.nodes, s.edges);
  const buildMs = performance.now() - t;

  t = performance.now();
  const renderer = new Sigma(graph, container, {
    settings: { hideEdgesOnMove: s.hideEdgesOnMove, renderLabels: true },
  });
  await nextFrame();
  await nextFrame();
  const firstRenderMs = performance.now() - t;

  let layout: FA2Layout | undefined;
  if (s.liveLayout) {
    layout = new FA2Layout(graph, {
      settings: { ...forceAtlas2.inferSettings(graph), barnesHutOptimize: true },
    });
    layout.start();
  }

  // Continuous pan/zoom while measuring: the interaction users actually do.
  const camera = renderer.getCamera();
  let animating = true;
  (async () => {
    const states = [
      { ratio: 0.25, x: 0.35, y: 0.6 },
      { ratio: 1.2, x: 0.5, y: 0.5 },
      { ratio: 0.5, x: 0.7, y: 0.3 },
    ];
    for (let i = 0; animating; i++) {
      await camera.animate(states[i % states.length], { duration: 1200 });
    }
  })();

  const frames = await measureFrames(6000);
  animating = false;
  layout?.kill();
  renderer.kill();

  return {
    name: s.name,
    nodes: s.nodes,
    edges: s.edges,
    buildMs: Math.round(buildMs),
    firstRenderMs: Math.round(firstRenderMs),
    ...frames,
  };
}

export const SCENARIOS: Scenario[] = [
  { name: '10k static', nodes: 10_000, edges: 30_000, liveLayout: false, hideEdgesOnMove: false },
  { name: '10k live FA2', nodes: 10_000, edges: 30_000, liveLayout: true, hideEdgesOnMove: false },
  { name: '50k static', nodes: 50_000, edges: 150_000, liveLayout: false, hideEdgesOnMove: false },
  { name: '50k hide-edges-on-move', nodes: 50_000, edges: 150_000, liveLayout: false, hideEdgesOnMove: true },
  { name: '100k static', nodes: 100_000, edges: 300_000, liveLayout: false, hideEdgesOnMove: false },
];
