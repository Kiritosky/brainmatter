<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import Graph from 'graphology';
  import Sigma, { DEFAULT_STYLES } from 'sigma';
  import forceAtlas2 from 'graphology-layout-forceatlas2';
  import FA2Layout from 'graphology-layout-forceatlas2/worker';
  import { Maximize, Minus, Plus } from '@lucide/svelte';
  import { api, type GraphSnapshot, type Id } from '#lib/api.ts';

  let {
    snapshot,
    selected,
    focusTick,
    colorFor,
    onselect,
    onopen,
  }: {
    snapshot: GraphSnapshot | null;
    selected: Id | null;
    focusTick: number;
    colorFor: (projects: Id[], kind: string) => string;
    onselect: (id: Id | null) => void;
    onopen: (id: Id) => void;
  } = $props();

  let container: HTMLDivElement;
  let renderer: Sigma | undefined;
  let layout: FA2Layout | undefined;
  let layoutTimer: ReturnType<typeof setTimeout> | undefined;
  let currentView: string | null = null;
  let hovered: string | null = null;
  let resizeObserver: ResizeObserver | undefined;
  const graph = new Graph({ type: 'directed', multi: true });

  // WebGL needs concrete colours: read them from the theme once.
  const css = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  const DIM_NODE = css('--color-line');
  const DIM_LABEL = css('--color-faint');
  const LABEL = css('--color-text');
  const EDGE = css('--color-line');
  const EDGE_DIM = css('--color-line-soft');
  const EDGE_ACTIVE = css('--color-muted');

  const sizeFor = (degree: number) => Math.min(18, 3.5 + Math.sqrt(degree) * 2.2);

  /** Highlight a node and its direct neighbours (hover wins over selection). */
  function applyHighlight() {
    if (!renderer) return;
    const focus = hovered ?? (selected && graph.hasNode(selected) ? selected : null);
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
      stopLayout(false);
      graph.clear();
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
        size: sizeFor(n.degree),
        color: colorFor(n.projects, n.kind),
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
      if (!wantedEdges.has(e)) graph.dropEdge(e);
    });
    for (const e of snap.edges) {
      if (!graph.hasEdge(e.id) && graph.hasNode(e.src) && graph.hasNode(e.dst)) {
        graph.addEdgeWithKey(e.id, e.src, e.dst, { relation: e.relation, origin: e.origin });
      }
    }

    placeNear(unplaced);
    if (unplaced.length > 0) runLayout(unplaced.length === graph.order ? 'full' : 'settle');
    applyHighlight();
    if (viewChanged) renderer?.getCamera().reset({ duration: 300 });
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

  function runLayout(kind: 'full' | 'settle') {
    stopLayout(false);
    if (graph.order < 2) {
      saveLayout();
      return;
    }
    const settings = forceAtlas2.inferSettings(graph);
    layout = new FA2Layout(graph, {
      settings: {
        ...settings,
        barnesHutOptimize: graph.order > 400,
        // Overlap prevention is incompatible with Barnes-Hut; fine for small graphs.
        adjustSizes: graph.order <= 400,
        gravity: 1,
        slowDown: kind === 'full' ? 2 : 8,
      },
    });
    layout.start();
    const ms = kind === 'full' ? Math.min(6000, 1500 + graph.order) : 1200;
    layoutTimer = setTimeout(() => stopLayout(true), ms);
  }

  function stopLayout(save: boolean) {
    clearTimeout(layoutTimer);
    if (layout) {
      layout.kill();
      layout = undefined;
      if (save) saveLayout();
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
        labelDensity: 0.8,
        labelGridCellSize: 90,
        // The label grid already avoids collisions; don't also hide small nodes.
        labelRenderedSizeThreshold: 3,
      },
      styles: {
        nodes: [
          DEFAULT_STYLES.nodes,
          {
            color: (a, s, gs) => (gs.hasHighlighted && !s.isHighlighted ? DIM_NODE : (a.color as string)),
            labelColor: (a, s, gs) =>
              gs.hasHighlighted && !s.isHighlighted ? DIM_LABEL : a.reviewed === false ? '#d3b0ff' : LABEL,
            labelFont: '-apple-system, BlinkMacSystemFont, sans-serif',
            labelSize: 12,
            labelVisibility: (_a, s) => (s.isHighlighted || s.isHovered ? 'visible' : 'auto'),
          },
        ],
        edges: [
          DEFAULT_STYLES.edges,
          {
            color: (_a, s, gs) => (s.isHighlighted ? EDGE_ACTIVE : gs.hasHighlighted ? EDGE_DIM : EDGE),
            size: (_a, s) => (s.isHighlighted ? 1.5 : 0.6),
          },
        ],
      },
    });

    renderer.on('clickNode', ({ node }) => onselect(node));
    renderer.on('doubleClickNode', ({ node, preventSigmaDefault }) => {
      preventSigmaDefault?.();
      onopen(node);
    });
    renderer.on('clickStage', () => onselect(null));
    renderer.on('enterNode', ({ node }) => {
      hovered = node;
      applyHighlight();
    });
    renderer.on('leaveNode', () => {
      hovered = null;
      applyHighlight();
    });
    renderer.on('nodeDragStart', () => stopLayout(false));
    renderer.on('nodeDragEnd', ({ node }) => {
      graph.setNodeAttribute(node, 'fixed', true);
      saveLayout();
    });

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
    stopLayout(false);
    renderer?.kill();
  });

  $effect(() => {
    if (renderer && snapshot) sync(snapshot);
  });

  $effect(() => {
    void selected;
    applyHighlight();
  });

  $effect(() => {
    void focusTick;
    // Wait a frame so freshly added nodes have display data.
    requestAnimationFrame(focusSelected);
  });
</script>

<div class="absolute inset-0" bind:this={container}></div>

{#if snapshot && snapshot.nodes.length > 0}
  <div class="glass-pill absolute right-4 bottom-4 z-10">
    <button class="icon-btn" aria-label="Vergrößern" title="Vergrößern" onclick={() => renderer?.getCamera().zoomIn({ duration: 180 })}><Plus size={15} /></button>
    <button class="icon-btn" aria-label="Verkleinern" title="Verkleinern" onclick={() => renderer?.getCamera().zoomOut({ duration: 180 })}><Minus size={15} /></button>
    <span class="mx-0.5 h-4 w-px bg-line"></span>
    <button class="icon-btn" aria-label="Alles zeigen" title="Alles zeigen" onclick={() => renderer?.getCamera().reset({ duration: 300 })}><Maximize size={14} /></button>
  </div>
{/if}
