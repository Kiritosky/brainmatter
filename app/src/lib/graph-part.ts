// Puts what changed of a view into the graph already loaded, instead of loading all of it.
import type { GraphEdge, GraphNode, GraphSnapshot, Id } from './api.ts';

/**
 * `part` holds those of `touched` still in the view and all their links within it, as
 * `graph_part` returns them. Rank and cluster come from the whole view, which the part does
 * not know: changed entries keep theirs, and new ones get an average size and the cluster
 * most of their neighbours are in.
 */
export function mergePart(graph: GraphSnapshot, part: GraphSnapshot, touched: Iterable<Id>): GraphSnapshot {
  const ids = new Set(touched);
  const fresh = new Map(part.nodes.map((n) => [n.id, n]));
  const known = new Map<Id, GraphNode>();
  const nodes: GraphNode[] = [];
  for (const n of graph.nodes) {
    known.set(n.id, n);
    if (!ids.has(n.id)) nodes.push(n);
    else {
      const f = fresh.get(n.id);
      if (f) nodes.push({ ...f, rank: n.rank, cluster: n.cluster });
    }
  }
  const added = part.nodes.filter((n) => !known.has(n.id));
  const present = new Set(nodes.map((n) => n.id));
  for (const n of added) present.add(n.id);

  const edges: GraphEdge[] = graph.edges.filter((e) => !ids.has(e.src) && !ids.has(e.dst));
  for (const e of part.edges) if (present.has(e.src) && present.has(e.dst)) edges.push(e);

  const rank = 1 / Math.max(1, present.size);
  for (const n of added) nodes.push({ ...n, rank, cluster: neighbourCluster(n.id, part.edges, known) });

  const degree = new Map<Id, number>();
  for (const e of edges) {
    degree.set(e.src, (degree.get(e.src) ?? 0) + 1);
    degree.set(e.dst, (degree.get(e.dst) ?? 0) + 1);
  }
  return {
    view: graph.view,
    nodes: nodes.map((n) => {
      const d = degree.get(n.id) ?? 0;
      return d === n.degree ? n : { ...n, degree: d };
    }),
    edges,
  };
}

function neighbourCluster(id: Id, edges: GraphEdge[], known: Map<Id, GraphNode>): number {
  const counts = new Map<number, number>();
  for (const e of edges) {
    const other = e.src === id ? e.dst : e.dst === id ? e.src : null;
    const cluster = other ? known.get(other)?.cluster : undefined;
    if (cluster !== undefined) counts.set(cluster, (counts.get(cluster) ?? 0) + 1);
  }
  let best = 0;
  let most = 0;
  for (const [cluster, count] of counts) {
    if (count > most) [best, most] = [cluster, count];
  }
  // Alone, it gets a cluster of its own and is coloured like one.
  return most ? best : 0xffffffff;
}
