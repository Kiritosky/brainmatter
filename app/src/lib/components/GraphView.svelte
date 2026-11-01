<script lang="ts">
  import { onDestroy, onMount, untrack } from 'svelte';
  import Graph from 'graphology';
  import Sigma, { DEFAULT_STYLES } from 'sigma';
  import { Maximize, Minus, Plus } from '@lucide/svelte';
  import { api, createdAt, type GraphNode, type GraphSnapshot, type Id } from '#lib/api.ts';
  import { assignGroups, NO_PROJECT, type Circle, type LayoutInput, type LayoutResult } from '#lib/grouped-layout.ts';
  import LayoutWorker from '#lib/grouped-layout.worker.ts?worker';
  import { keepInside, pushAround, type Point } from '#lib/hover-push.ts';
  import { motionAllowed, prefs } from '#lib/prefs.svelte.ts';
  import { theme } from '#lib/theme.svelte.ts';

  let {
    snapshot,
    selected,
    focusTick,
    suggested,
    until,
    hidden,
    pulsing,
    projects,
    colorFor,
    onselect,
    onopen,
    onopenproject,
    onlink,
  }: {
    projects: { id: Id; parent: Id | null; name: string; color: string }[];
    onopenproject: (id: Id) => void;
    /** Dragged from one entry to another with ⌥. */
    onlink: (src: Id, dst: Id) => void;
    /** Entries the filters hide. They keep their place in the layout. */
    hidden: Set<Id>;
    /** Entries to point out for a moment, because an AI just changed them. */
    pulsing: Set<Id>;
    /** Time slider: show only what existed at this time (ms). */
    until: number | null;
    snapshot: GraphSnapshot | null;
    selected: Id | null;
    focusTick: number;
    /** Entries suggested as links for the selection, drawn as ghost edges. */
    suggested: Id[];
    colorFor: (node: GraphNode) => string;
    onselect: (id: Id | null) => void;
    onopen: (id: Id) => void;
  } = $props();

  let container: HTMLDivElement;
  let renderer: Sigma | undefined;
  let currentView: string | null = null;
  let hovered: string | null = null;
  /** The line being drawn with ⌥ from one entry to another, in viewport coordinates. */
  let linking = $state.raw<{ from: string; x: number; y: number; toX: number; toY: number } | null>(null);
  let resizeObserver: ResizeObserver | undefined;
  const graph = new Graph({ type: 'directed', multi: true });

  // WebGL needs concrete colours, so they are read from the theme.
  const css = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  const readColors = () => ({
    dimNode: css('--color-line'),
    dimLabel: css('--color-faint'),
    label: css('--color-text'),
    labelAi: css('--color-ai'),
    edge: css('--color-line'),
    edgeDim: css('--color-line-soft'),
    edgeActive: css('--color-muted'),
    edgeGhost: css('--color-accent'),
    edgeBridge: css('--color-faint'),
    plate: css('--color-bg'),
  });
  let colors = readColors();
  const GHOST = 'ghost:';
  // Read by the styles below; a plain variable, so a drag on the slider costs
  // one refresh and not a reactive update per node.
  let visibleUntil = Infinity;
  // Likewise, and not node attributes: Sigma would have to repaint nodes the sync
  // just added and hasn't processed yet, and throws.
  let filteredOut: Set<Id> = new Set();
  // Likewise read by the styles.
  const NODE_SCALE = { small: 0.75, medium: 1, large: 1.35 };
  const EDGE_SCALE = { small: 0.7, medium: 1, large: 1.6 };
  const LABELS = {
    few: { labelDensity: 0.4, labelGridCellSize: 140 },
    normal: { labelDensity: 0.8, labelGridCellSize: 90 },
    many: { labelDensity: 2, labelGridCellSize: 60 },
  };
  let nodeScale = 1;
  let edgeScale = 1;

  // ---------------------------------------------------- pointer ---
  /** Around the entry under the pointer, on screen. */
  const PUSH_RADIUS_PX = 150;
  const PUSH_PX = 85;
  const MAX_PUSHED = 80;
  const HOVER_GROW = 1.45;
  /** Time constant of the movement: nearly there after three of these. */
  const HOVER_TAU_MS = 50;
  const ROOM_MARGIN = 3;

  interface Lift {
    x: number;
    y: number;
    toX: number;
    toY: number;
    scale: number;
    toScale: number;
  }
  /**
   * What the pointer changes on screen. It is applied by the styles and never
   * written to the graph, whose positions are the ones that get saved and laid out.
   */
  const lifted = new Map<string, Lift>();
  let hoverFrame = 0;
  let hoverStepAt = 0;

  function liftOf(id: string) {
    let lift = lifted.get(id);
    if (!lift) {
      lift = { x: 0, y: 0, toX: 0, toY: 0, scale: 1, toScale: 1 };
      lifted.set(id, lift);
    }
    return lift;
  }

  function pixelsPerUnit() {
    const zero = renderer!.graphToViewport({ x: 0, y: 0 });
    const one = renderer!.graphToViewport({ x: 1, y: 0 });
    return Math.hypot(one.x - zero.x, one.y - zero.y) || 1;
  }

  function hoverOver(node: string | null) {
    for (const lift of lifted.values()) {
      lift.toX = lift.toY = 0;
      lift.toScale = 1;
    }
    if (renderer && node && graph.hasNode(node)) {
      const lift = liftOf(node);
      // It stays where it is on screen, or moving it could take it away from the
      // pointer, which would end the hover and bring it back.
      lift.toX = lift.x;
      lift.toY = lift.y;
      lift.toScale = HOVER_GROW;
      if (prefs.values.hoverPush && motionAllowed() && graph.order <= MAX_ANIMATED) {
        const a = graph.getNodeAttributes(node);
        const anchor = { x: a.x + lift.x, y: a.y + lift.y };
        const perUnit = pixelsPerUnit();
        const others: (Point & { id: string })[] = [];
        graph.forEachNode((id, n) => {
          if (id !== node && !filteredOut.has(id) && (n.born as number) <= visibleUntil) others.push({ id, x: n.x, y: n.y });
        });
        const pushes = pushAround(anchor, others, PUSH_RADIUS_PX / perUnit, PUSH_PX / perUnit, MAX_PUSHED);
        const rooms = new Map(circles.map((c) => [c.id, c]));
        for (const [id, push] of pushes) {
          const { x, y } = graph.getNodeAttributes(id);
          const home = { x, y };
          const room = rooms.get(groupOf.get(id) ?? '');
          // Pushed along, but never out of its project.
          const offset = room ? keepInside(home, push, { x: room.x, y: room.y, r: room.r - ROOM_MARGIN }) : push;
          const other = liftOf(id);
          other.toX = offset.x;
          other.toY = offset.y;
        }
      }
    }
    if (!hoverFrame) {
      hoverStepAt = performance.now();
      hoverFrame = requestAnimationFrame(hoverStep);
    }
  }

  function hoverStep(now: number) {
    hoverFrame = 0;
    if (!renderer) return;
    const k = motionAllowed() ? 1 - Math.exp(-(now - hoverStepAt) / HOVER_TAU_MS) : 1;
    hoverStepAt = now;
    const closeEnough = 0.3 / pixelsPerUnit();
    const changed: string[] = [];
    let moving = false;
    for (const [id, lift] of lifted) {
      if (!graph.hasNode(id)) {
        lifted.delete(id);
        continue;
      }
      lift.x += (lift.toX - lift.x) * k;
      lift.y += (lift.toY - lift.y) * k;
      lift.scale += (lift.toScale - lift.scale) * k;
      const settled = Math.abs(lift.toScale - lift.scale) < 0.004 && Math.hypot(lift.toX - lift.x, lift.toY - lift.y) < closeEnough;
      if (settled) {
        lift.x = lift.toX;
        lift.y = lift.toY;
        lift.scale = lift.toScale;
        if (!lift.x && !lift.y && lift.scale === 1) lifted.delete(id);
      } else {
        moving = true;
      }
      changed.push(id);
    }
    if (changed.length) renderer.refresh({ partialGraph: { nodes: changed }, skipIndexation: false });
    if (moving) hoverFrame = requestAnimationFrame(hoverStep);
  }

  /** For when positions are about to change for real. */
  function dropLifts() {
    cancelAnimationFrame(hoverFrame);
    hoverFrame = 0;
    lifted.clear();
  }

  /** A burst of this many says as much as a thousand, and stays cheap. */
  const MAX_RINGS = 30;
  // Plain elements above the canvas: animating node sizes would redraw the graph
  // every frame. The render callback keeps them on their nodes during layout and panning.
  let rings = $state.raw<{ id: Id; x: number; y: number }[]>([]);

  function placeRings() {
    if (!renderer || !pulsing.size) {
      if (rings.length) rings = [];
      return;
    }
    const placed = [];
    for (const id of pulsing) {
      if (placed.length === MAX_RINGS) break;
      if (!graph.hasNode(id) || filteredOut.has(id)) continue;
      const { x, y } = graph.getNodeAttributes(id);
      placed.push({ id, ...renderer.graphToViewport({ x, y }) });
    }
    rings = placed;
  }

  // ------------------------------------------------- project groups ---

  /** Saved positions from before grouping would mix the projects, so each view starts over once. */
  const GROUPED_KEY = 'bm.groupedViews';
  /** Beyond this many, moving entries smoothly costs more than it shows. */
  const MAX_ANIMATED = 3000;
  const MOVE_MS = 480;
  /** Too small on screen to read a name in. */
  const MIN_LABELLED_PX = 42;
  const MIN_DRAWN_PX = 10;
  const FRAME_MS = 300;
  /** One entry alone would otherwise fill the screen. */
  const MIN_FRAME_RATIO = 0.15;

  const worker = new LayoutWorker();
  let layoutRun = 0;
  let moveFrame = 0;
  let groupOf = new Map<Id, string>();
  let structure = '';
  let circles: Circle[] = [];
  /** Where the last layout put each entry; the graph holds in-between places during a move. */
  let placed = new Map<string, { x: number; y: number }>();
  let parents = new Map<string, string | null>();
  let framed = '';
  let frameNext = false;
  /** Groups with at least one entry showing, and their ancestors. */
  let shownGroups = new Set<string>();
  let onScreen = $state.raw<{ id: string; x: number; y: number; r: number; depth: number; color: string; name: string; labelled: boolean }[]>([]);

  const isBridge = (a: Record<string, unknown>) =>
    !!a.src && groupOf.get(a.src as Id) !== groupOf.get(a.dst as Id);

  function placeCircles() {
    if (!renderer) return;
    const byId = new Map(projects.map((p) => [p.id, p]));
    const next: typeof onScreen = [];
    // Outer circles first, so inner ones are drawn on top.
    for (const c of circles) {
      if (!shownGroups.has(c.id)) continue;
      const centre = renderer.graphToViewport({ x: c.x, y: c.y });
      const edge = renderer.graphToViewport({ x: c.x + c.r, y: c.y });
      const r = Math.hypot(edge.x - centre.x, edge.y - centre.y);
      if (r < MIN_DRAWN_PX) continue;
      const project = byId.get(c.id);
      next.push({
        id: c.id,
        x: centre.x,
        y: centre.y,
        r,
        depth: c.depth,
        color: project?.color ?? 'var(--color-faint)',
        name: project?.name ?? 'Ohne Projekt',
        labelled: r >= MIN_LABELLED_PX,
      });
    }
    onScreen = next;
  }

  function updateShownGroups() {
    const up = new Map(circles.map((c) => [c.id, c.parent]));
    const shown = new Set<string>();
    const holdsEntries = new Set<string>();
    const lostSome = new Set<string>();
    const climb = (from: string | undefined, into: Set<string>) => {
      let group: string | null | undefined = from;
      while (group !== undefined && group !== null && !into.has(group)) {
        into.add(group);
        group = up.get(group);
      }
    };
    graph.forEachNode((id, a) => {
      const group = groupOf.get(id);
      if (filteredOut.has(id)) climb(group, lostSome);
      else if ((a.born as number) <= visibleUntil) {
        if (group !== undefined) holdsEntries.add(group);
        climb(group, shown);
      }
    });
    // A parent emptied by hiding, around one project left, would only be a big empty ring.
    const shownChildren = new Map<string, number>();
    for (const c of circles) {
      if (c.parent && shown.has(c.id)) shownChildren.set(c.parent, (shownChildren.get(c.parent) ?? 0) + 1);
    }
    for (const c of circles) {
      if (lostSome.has(c.id) && !holdsEntries.has(c.id) && shownChildren.get(c.id) === 1) shown.delete(c.id);
    }
    shownGroups = shown;
    placeCircles();
  }

  /** The box around top-level circles and the view's own entries, which sit outside any circle. */
  function boxAround(tops: Circle[], entries: { x: number; y: number }[]) {
    const box = { x: [Infinity, -Infinity] as [number, number], y: [Infinity, -Infinity] as [number, number] };
    const grow = (x: number, y: number, r: number) => {
      box.x = [Math.min(box.x[0], x - r), Math.max(box.x[1], x + r)];
      box.y = [Math.min(box.y[0], y - r), Math.max(box.y[1], y + r)];
    };
    for (const c of tops) grow(c.x, c.y, c.r);
    for (const e of entries) grow(e.x, e.y, 0);
    return box;
  }

  /** A fixed frame, so that entries moved for a moment never rescale the whole picture. */
  function fitToCircles() {
    if (!renderer) return;
    const box = boxAround(circles.filter((c) => c.parent === null), [...placed.values()]);
    const flat = !(box.x[1] > box.x[0]) && !(box.y[1] > box.y[0]);
    renderer.setCustomBBox(flat ? null : box);
    // Only a refresh rescales; `frameShown` converts coordinates right after this.
    renderer.refresh();
  }

  /** The outermost circles still drawn, which need not be top-level ones. */
  function shownTops() {
    const up = new Map(circles.map((c) => [c.id, c.parent]));
    const insideShown = (c: Circle) => {
      for (let p = c.parent; p; p = up.get(p) ?? null) if (shownGroups.has(p)) return true;
      return false;
    };
    return circles.filter((c) => shownGroups.has(c.id) && !insideShown(c));
  }

  /** Looks at what is showing, so hiding a project leaves no hole in the picture. */
  function frameShown() {
    if (!renderer) return;
    const camera = renderer.getCamera();
    const full = renderer.getCustomBBox();
    const circled = new Set(circles.map((c) => c.id));
    const entries: { x: number; y: number }[] = [];
    graph.forEachNode((id, a) => {
      if (!filteredOut.has(id) && (a.born as number) <= visibleUntil && !circled.has(groupOf.get(id) ?? '')) {
        entries.push(placed.get(id) ?? { x: a.x, y: a.y });
      }
    });
    const tops = shownTops();
    if (!full || (!tops.length && !entries.length)) {
      camera.reset({ duration: FRAME_MS });
      return;
    }
    const box = boxAround(tops, entries);
    const extent = (b: { x: [number, number]; y: [number, number] }) => Math.max(b.x[1] - b.x[0], b.y[1] - b.y[0]);
    const centre = renderer.viewportToFramedGraph(
      renderer.graphToViewport({ x: (box.x[0] + box.x[1]) / 2, y: (box.y[0] + box.y[1]) / 2 }),
    );
    const ratio = Math.min(1, Math.max(MIN_FRAME_RATIO, extent(box) / extent(full)));
    camera.animate({ ...centre, ratio }, { duration: FRAME_MS });
  }

  /** Which top-level circles show, to tell a real change from a refresh of the same filter. */
  const shownKey = () => shownTops().map((c) => c.id).sort().join();

  function requestLayout(snap: GraphSnapshot, dirty: string[], fresh: boolean) {
    const input: LayoutInput = {
      nodes: snap.nodes
        .filter((n) => graph.hasNode(n.id))
        .map((n) => {
          const a = graph.getNodeAttributes(n.id);
          return { id: n.id, projects: n.projects, x: a.x, y: a.y, pinned: !fresh && !!a.fixed };
        }),
      edges: snap.edges.map((e) => ({ src: e.src, dst: e.dst })),
      projects: projects.map((p) => ({ id: p.id, parent: p.parent })),
      root: snap.view === 'all' ? null : snap.view,
      dirty,
    };
    worker.postMessage({ run: ++layoutRun, input });
    pendingFresh = pendingFresh || fresh;
    pendingView = snap.view;
  }
  let pendingFresh = false;
  let pendingView = '';

  worker.onmessage = (e: MessageEvent<{ run: number; result: LayoutResult }>) => {
    if (e.data.run !== layoutRun) return;
    applyLayout(e.data.result);
  };

  function markGrouped(view: string) {
    const views = new Set<string>(JSON.parse(localStorage.getItem(GROUPED_KEY) ?? '[]'));
    if (views.has(view)) return;
    views.add(view);
    localStorage.setItem(GROUPED_KEY, JSON.stringify([...views]));
  }

  function applyLayout({ positions, circles: next }: LayoutResult) {
    const fresh = pendingFresh;
    pendingFresh = false;
    circles = [...next].sort((a, b) => a.depth - b.depth);
    cancelAnimationFrame(moveFrame);

    const target = new Map(positions.map((p) => [p.id, p]));
    placed = target;
    dropLifts();
    const finish = () => {
      markGrouped(pendingView);
      saveLayout();
      updateShownGroups();
    };
    fitToCircles();
    updateShownGroups();
    const reframe = fresh || frameNext;
    frameNext = false;
    framed = shownKey();
    if (reframe) frameShown();

    const start = new Map<string, { x: number; y: number }>();
    graph.forEachNode((id, a) => {
      if (target.has(id)) start.set(id, { x: a.x, y: a.y });
    });
    const motion = motionAllowed() ? MOVE_MS : 0;
    if (!motion || start.size > MAX_ANIMATED) {
      graph.updateEachNodeAttributes((id, a) => {
        const p = target.get(id);
        return p ? { ...a, x: p.x, y: p.y } : a;
      });
      finish();
      return;
    }
    const began = performance.now();
    const step = (now: number) => {
      const t = Math.min(1, (now - began) / motion);
      const eased = 1 - (1 - t) ** 3;
      graph.updateEachNodeAttributes((id, a) => {
        const from = start.get(id);
        const to = target.get(id);
        if (!from || !to) return a;
        return { ...a, x: from.x + (to.x - from.x) * eased, y: from.y + (to.y - from.y) * eased };
      });
      if (t < 1) moveFrame = requestAnimationFrame(step);
      else finish();
    };
    moveFrame = requestAnimationFrame(step);
  }

  /** `rank * count` is 1 for an average entry, whatever the size of the view. */
  const sizeFor = (rank: number, count: number) => Math.min(18, Math.max(3.5, 2 + Math.sqrt(rank * count) * 3.6));

  /** Highlight a node and its direct neighbours (hover wins over selection). */
  function applyHighlight() {
    if (!renderer) return;
    const pointedAt = prefs.values.hoverDim ? hovered : null;
    const focus = pointedAt ?? (selected && graph.hasNode(selected) ? selected : null);
    const nodes = new Set<string>();
    const edges = new Set<string>();
    if (focus) {
      nodes.add(focus);
      graph.forEachEdge(focus, (edge, _a, src, dst) => {
        edges.add(edge);
        nodes.add(src);
        nodes.add(dst);
      });
    }
    graph.forEachNode((n) => renderer!.setNodeState(n, { isHighlighted: nodes.has(n) }));
    graph.forEachEdge((e) => renderer!.setEdgeState(e, { isHighlighted: edges.has(e) }));
    renderer.setGraphState({ hasHighlighted: focus !== null });
  }

  /** Incremental sync so positions, camera and running layouts survive updates. */
  function sync(snap: GraphSnapshot) {
    const viewChanged = snap.view !== currentView;
    if (viewChanged) {
      cancelAnimationFrame(moveFrame);
      dropLifts();
      graph.clear();
      circles = [];
      placed = new Map();
      onScreen = [];
      groupOf = new Map();
      structure = '';
      currentView = snap.view;
    }

    const wanted = new Set(snap.nodes.map((n) => n.id));
    graph.forEachNode((n) => {
      if (!wanted.has(n)) graph.dropNode(n);
    });

    const unplaced: string[] = [];
    for (const n of snap.nodes) {
      const attrs = {
        label: n.title,
        size: sizeFor(n.rank, snap.nodes.length),
        color: colorFor(n),
        born: createdAt(n.id),
        key: n.id,
        reviewed: n.reviewed,
        fixed: n.pinned,
      };
      if (graph.hasNode(n.id)) {
        graph.mergeNodeAttributes(n.id, attrs);
      } else {
        const placed = n.x !== null && n.y !== null;
        graph.addNode(n.id, { ...attrs, x: n.x ?? 0, y: n.y ?? 0 });
        if (!placed) unplaced.push(n.id);
      }
    }

    const wantedEdges = new Set(snap.edges.map((e) => e.id));
    graph.forEachEdge((e) => {
      if (!wantedEdges.has(e) && !e.startsWith(GHOST)) graph.dropEdge(e);
    });
    for (const e of snap.edges) {
      if (!graph.hasEdge(e.id) && graph.hasNode(e.src) && graph.hasNode(e.dst)) {
        graph.addEdgeWithKey(e.id, e.src, e.dst, { relation: e.relation, origin: e.origin, born: createdAt(e.id), src: e.src, dst: e.dst });
      }
    }

    placeNear(unplaced);

    const root = snap.view === 'all' ? null : snap.view;
    const tree = projects.map((p) => ({ id: p.id, parent: p.parent }));
    const groups = assignGroups(snap.nodes, snap.edges, tree, root);
    const dirty = new Set(unplaced);
    for (const [id, group] of groups) {
      const was = groupOf.get(id);
      if (was !== undefined && was !== group) dirty.add(id);
    }
    groupOf = groups;

    const grouped = (JSON.parse(localStorage.getItem(GROUPED_KEY) ?? '[]') as string[]).includes(snap.view);
    const shape = `${[...groups].map(([id, group]) => `${id}:${group}`).join()}|${tree.map((p) => `${p.id}<${p.parent}`).join()}`;
    // Places from before a project moved would leave its new parent far too roomy.
    const moved = !viewChanged && tree.some((p) => parents.has(p.id) && parents.get(p.id) !== p.parent);
    parents = new Map(tree.map((p) => [p.id, p.parent]));
    if (!grouped) {
      requestLayout(snap, snap.nodes.map((n) => n.id), true);
    } else if (moved) {
      frameNext = true;
      requestLayout(snap, snap.nodes.map((n) => n.id), false);
    } else if (dirty.size || shape !== structure) {
      requestLayout(snap, [...dirty], unplaced.length === graph.order);
    }
    structure = shape;

    syncGhosts();
    updateShownGroups();
    if (viewChanged) renderer?.getCamera().reset({ duration: 300 });
  }

  /** Ghost edges carry no weight, so they never pull on the layout. */
  function syncGhosts() {
    graph.forEachEdge((e) => {
      if (e.startsWith(GHOST)) graph.dropEdge(e);
    });
    if (selected && graph.hasNode(selected)) {
      for (const id of suggested) {
        if (graph.hasNode(id) && id !== selected) {
          graph.addEdgeWithKey(`${GHOST}${id}`, selected, id, { ghost: true, weight: 0 });
        }
      }
    }
    applyHighlight();
  }

  /** New nodes start at the centroid of their placed neighbours. */
  function placeNear(ids: string[]) {
    const pending = new Set(ids);
    const radius = Math.max(10, Math.sqrt(graph.order) * 8);
    for (const id of ids) {
      let x = 0, y = 0, k = 0;
      graph.forEachNeighbor(id, (nb, a) => {
        if (!pending.has(nb)) {
          x += a.x;
          y += a.y;
          k++;
        }
      });
      const jitter = () => (Math.random() - 0.5) * (k ? 10 : radius);
      graph.mergeNodeAttributes(id, { x: (k ? x / k : 0) + jitter(), y: (k ? y / k : 0) + jitter() });
      pending.delete(id);
    }
  }

  function saveLayout() {
    if (!currentView) return;
    const positions: { node: Id; x: number; y: number; pinned: boolean }[] = [];
    graph.forEachNode((node, a) => positions.push({ node, x: a.x, y: a.y, pinned: !!a.fixed }));
    api.saveLayout(currentView, positions).catch(() => {});
  }

  function focusSelected() {
    if (!renderer || !selected || !graph.hasNode(selected)) return;
    const d = renderer.getNodeDisplayData(selected);
    if (d) renderer.getCamera().animate({ x: d.x, y: d.y, ratio: Math.min(renderer.getCamera().ratio, 1) }, { duration: 400 });
  }

  onMount(() => {
    renderer = new Sigma(graph, container, {
      settings: {
        renderLabels: true,
        enableNodeDrag: true,
        // Sigma 4 defaults to sizes in graph units; we want pixels.
        itemSizesReference: 'screen',
        // Room for labels of nodes at the edge of the view.
        stagePadding: 60,
        ...LABELS[prefs.values.labels],
        // The label grid already avoids collisions; don't also hide small nodes.
        labelRenderedSizeThreshold: 3,
      },
      styles: {
        nodes: [
          DEFAULT_STYLES.nodes,
          {
            visibility: (a) => (filteredOut.has(a.key as Id) || (a.born as number) > visibleUntil ? 'hidden' : 'visible'),
            x: (a) => (a.x as number) + (lifted.get(a.key as string)?.x ?? 0),
            y: (a) => (a.y as number) + (lifted.get(a.key as string)?.y ?? 0),
            size: (a) => (a.size as number) * nodeScale * (lifted.get(a.key as string)?.scale ?? 1),
            // Sigma's default here is a white plate with a shadow behind the entry.
            backdropVisibility: 'hidden',
            // So that a line leaving the entry does not run through its name.
            labelBackgroundColor: (_a, s) => (s.isHovered || s.isHighlighted ? `${colors.plate}b8` : 'rgba(0, 0, 0, 0)'),
            labelBackgroundPadding: 2,
            color: (a, s, gs) => (gs.hasHighlighted && !s.isHighlighted ? colors.dimNode : (a.color as string)),
            labelColor: (a, s, gs) =>
              gs.hasHighlighted && !s.isHighlighted ? colors.dimLabel : a.reviewed === false ? colors.labelAi : colors.label,
            labelFont: '-apple-system, BlinkMacSystemFont, sans-serif',
            labelSize: 12,
            labelVisibility: (_a, s) => (s.isHighlighted || s.isHovered ? 'visible' : 'auto'),
          },
        ],
        edges: [
          DEFAULT_STYLES.edges,
          {
            visibility: (a) =>
              filteredOut.has(a.src as Id) || filteredOut.has(a.dst as Id) || (a.born as number) > visibleUntil ? 'hidden' : 'visible',
            color: (a, s, gs) =>
              a.ghost ? colors.edgeGhost : s.isHighlighted ? colors.edgeActive : gs.hasHighlighted ? colors.edgeDim : isBridge(a) ? colors.edgeBridge : colors.edge,
            size: (a, s) => edgeScale * (a.ghost ? 1 : s.isHighlighted ? 1.5 : isBridge(a) ? 0.9 : 0.6),
          },
        ],
      },
    });

    // Untracked: effects call refresh(), which can render right there, and must not
    // come to depend on the rings this writes.
    renderer.on('afterRender', () =>
      untrack(() => {
        if (rings.length) placeRings();
        if (circles.length) placeCircles();
      }),
    );
    renderer.on('clickNode', ({ node }) => onselect(node));
    renderer.on('doubleClickNode', ({ node, preventSigmaDefault }) => {
      preventSigmaDefault?.();
      onopen(node);
    });
    renderer.on('clickStage', () => onselect(null));
    // Into the innermost project under the pointer. Elsewhere it zooms, as usual.
    renderer.on('doubleClickStage', ({ event, preventSigmaDefault }) => {
      const at = renderer!.viewportToGraph(event);
      const inside = circles.filter((c) => c.id !== NO_PROJECT && shownGroups.has(c.id) && Math.hypot(at.x - c.x, at.y - c.y) <= c.r);
      const innermost = inside.at(-1);
      if (innermost) {
        preventSigmaDefault();
        onopenproject(innermost.id);
      }
    });
    renderer.on('enterNode', ({ node }) => {
      hovered = node;
      applyHighlight();
      if (!linking) hoverOver(node);
    });
    renderer.on('leaveNode', () => {
      hovered = null;
      applyHighlight();
      hoverOver(null);
    });
    renderer.on('nodeDragStart', ({ node, event, preventSigmaDefault }) => {
      // Dragging works on real positions, so nothing may be off them any more.
      cancelAnimationFrame(moveFrame);
      dropLifts();
      renderer?.refresh();
      if ((event.original as MouseEvent).altKey) {
        preventSigmaDefault();
        const { x, y } = graph.getNodeAttributes(node) as { x: number; y: number };
        linking = { from: node, ...renderer!.graphToViewport({ x, y }), toX: event.x, toY: event.y };
      }
    });
    renderer.on('moveBody', ({ event, preventSigmaDefault }) => {
      if (!linking) return;
      preventSigmaDefault();
      linking = { ...linking, toX: event.x, toY: event.y };
    });
    renderer.on('upNode', ({ node }) => {
      if (linking && node !== linking.from) onlink(linking.from, node);
    });
    renderer.on('nodeDragEnd', ({ node }) => {
      graph.setNodeAttribute(node, 'fixed', true);
      // The circle follows what it contains.
      if (snapshot) requestLayout(snapshot, [], false);
    });

    // For the walkthrough: the canvas has nothing else to look at from outside.
    if (import.meta.env.DEV) {
      (window as unknown as { __bmGraph: unknown }).__bmGraph = {
        onScreen: (title: string) => {
          const id = graph.findNode((_n, a) => a.label === title);
          const d = id && renderer?.getNodeDisplayData(id);
          return d && id ? { ...renderer!.framedGraphToViewport(d), size: d.size, savedX: graph.getNodeAttribute(id, 'x') } : null;
        },
        ringOnScreen: (project: string) => {
          const c = circles.find((circle) => circle.id === project);
          if (!c || !renderer) return null;
          const centre = renderer.graphToViewport(c);
          return { ...centre, radius: Math.abs(renderer.graphToViewport({ x: c.x + c.r, y: c.y }).x - centre.x) };
        },
        camera: () => renderer?.getCamera().getState(),
      };
    }

    // Sigma only reacts to window resizes; panels also change the container.
    resizeObserver = new ResizeObserver(() => {
      renderer?.resize();
      renderer?.refresh();
    });
    resizeObserver.observe(container);

    if (snapshot) sync(snapshot);
  });

  onDestroy(() => {
    resizeObserver?.disconnect();
    cancelAnimationFrame(moveFrame);
    cancelAnimationFrame(hoverFrame);
    worker.terminate();
    renderer?.kill();
  });

  $effect(() => {
    if (renderer && snapshot) sync(snapshot);
  });

  $effect(() => {
    void selected;
    void suggested;
    syncGhosts();
  });

  $effect(() => {
    visibleUntil = until ?? Infinity;
    untrack(updateShownGroups);
    renderer?.refresh();
  });

  $effect(() => {
    filteredOut = hidden;
    untrack(() => {
      updateShownGroups();
      const key = shownKey();
      if (key !== framed && circles.length) frameShown();
      framed = key;
    });
    renderer?.refresh();
  });

  // After the sync above, so entries it just added have a place.
  $effect(() => {
    void snapshot;
    void pulsing;
    untrack(placeRings);
  });

  $effect(() => {
    void theme.tick;
    void prefs.values.accent;
    colors = readColors();
    renderer?.refresh();
  });

  $effect(() => {
    nodeScale = NODE_SCALE[prefs.values.nodeSize];
    edgeScale = EDGE_SCALE[prefs.values.edgeWeight];
    const { labelDensity, labelGridCellSize } = LABELS[prefs.values.labels];
    void prefs.values.hoverDim;
    untrack(() => {
      renderer?.setSetting('labelDensity', labelDensity);
      renderer?.setSetting('labelGridCellSize', labelGridCellSize);
      applyHighlight();
      renderer?.refresh();
    });
  });

  $effect(() => {
    void focusTick;
    // Wait a frame so freshly added nodes have display data.
    requestAnimationFrame(focusSelected);
  });
</script>

<!-- Beneath the canvas, whose background is transparent. -->
<div class="pointer-events-none absolute inset-0 overflow-hidden" aria-hidden="true">
  {#if prefs.values.groupBackgrounds}
    {#each onScreen as c (c.id)}
      <div class="group-circle" style="left: {c.x - c.r}px; top: {c.y - c.r}px; width: {c.r * 2}px; height: {c.r * 2}px; --group: {c.color}"></div>
    {/each}
  {/if}
</div>

<div class="absolute inset-0" bind:this={container}></div>

<!-- Above it, so that the names can be clicked. -->
<div class="pointer-events-none absolute inset-0 overflow-hidden">
  {#each onScreen as c (c.id)}
    {#if c.labelled && prefs.values.groupNames}
      {#if c.id === NO_PROJECT}
        <span class="group-label" style="left: {c.x}px; top: {c.y - c.r + 7}px; max-width: {c.r * 1.5}px; --group: {c.color}">{c.name}</span>
      {:else}
        <button class="group-label group-link" style="left: {c.x}px; top: {c.y - c.r + 7}px; max-width: {c.r * 1.5}px; --group: {c.color}"
          title="„{c.name}“ öffnen" onclick={() => onopenproject(c.id)}>{c.name}</button>
      {/if}
    {/if}
  {/each}
</div>

{#if linking}
  <svg class="pointer-events-none absolute inset-0 h-full w-full" aria-hidden="true">
    <line x1={linking.x} y1={linking.y} x2={linking.toX} y2={linking.toY} class="link-line" />
    <circle cx={linking.toX} cy={linking.toY} r="4" class="link-end" />
  </svg>
{/if}

<!-- Wherever the button is let go; `upNode` has already seen it by then. -->
<svelte:window onmouseup={() => setTimeout(() => (linking = null))} />

{#each rings as ring (ring.id)}
  <span class="pulse-ring" style="left: {ring.x}px; top: {ring.y}px"></span>
{/each}

{#if snapshot && snapshot.nodes.length > 0}
  <div class="glass-pill absolute right-4 bottom-4 z-10">
    <button class="icon-btn" aria-label="Vergrößern" title="Vergrößern" onclick={() => renderer?.getCamera().zoomIn({ duration: 180 })}><Plus size={15} /></button>
    <button class="icon-btn" aria-label="Verkleinern" title="Verkleinern" onclick={() => renderer?.getCamera().zoomOut({ duration: 180 })}><Minus size={15} /></button>
    <span class="mx-0.5 h-4 w-px bg-line"></span>
    <button class="icon-btn" aria-label="Alles zeigen" title="Alles zeigen" onclick={() => renderer?.getCamera().reset({ duration: 300 })}><Maximize size={14} /></button>
  </div>
{/if}

<style>
  .group-circle {
    position: absolute;
    border: 1px solid color-mix(in srgb, var(--group) 28%, transparent);
    border-radius: 50%;
    background: color-mix(in srgb, var(--group) 5%, transparent);
  }
  .group-label {
    position: absolute;
    overflow: hidden;
    transform: translateX(-50%);
    color: color-mix(in srgb, var(--group) 85%, var(--color-text));
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.01em;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .group-link {
    pointer-events: auto;
    border-radius: 4px;
    padding: 0 4px;
    transition: background-color 120ms;
  }
  .group-link:hover {
    background: color-mix(in srgb, var(--group) 16%, transparent);
  }
  .link-line {
    stroke: var(--color-accent);
    stroke-width: 1.5;
    stroke-dasharray: 5 4;
  }
  .link-end {
    fill: var(--color-accent);
  }
  .pulse-ring {
    position: absolute;
    width: 30px;
    height: 30px;
    margin: -15px 0 0 -15px;
    border: 2px solid var(--color-ai);
    border-radius: 50%;
    opacity: 0;
    pointer-events: none;
    animation: pulse-ring 1.3s var(--ease-out-soft) 2;
  }
  @keyframes pulse-ring {
    from {
      transform: scale(0.35);
      opacity: 0.95;
    }
    to {
      transform: scale(1.7);
      opacity: 0;
    }
  }
</style>
