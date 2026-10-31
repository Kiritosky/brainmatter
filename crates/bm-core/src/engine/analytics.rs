//! Graph analytics over node indices: importance and topic clusters. Edges are
//! treated as undirected for clustering and for following links.

/// PageRank with damping 0.85 over the undirected graph. Sums to about 1.
pub(crate) fn pagerank(n: usize, edges: &[(usize, usize)]) -> Vec<f32> {
    const DAMPING: f32 = 0.85;
    const ITERATIONS: usize = 20;
    if n == 0 {
        return Vec::new();
    }
    let adjacency = adjacency(n, edges);
    let mut rank = vec![1.0 / n as f32; n];
    let mut next = vec![0.0f32; n];
    for _ in 0..ITERATIONS {
        let dangling: f32 =
            rank.iter().zip(&adjacency).filter(|(_, a)| a.is_empty()).map(|(r, _)| r).sum();
        let base = (1.0 - DAMPING + DAMPING * dangling) / n as f32;
        next.fill(base);
        for (i, neighbours) in adjacency.iter().enumerate() {
            if !neighbours.is_empty() {
                let share = DAMPING * rank[i] / neighbours.len() as f32;
                for &j in neighbours {
                    next[j] += share;
                }
            }
        }
        std::mem::swap(&mut rank, &mut next);
    }
    rank
}

fn adjacency(n: usize, edges: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let mut adjacency = vec![Vec::new(); n];
    for &(a, b) in edges {
        if a != b {
            adjacency[a].push(b);
            adjacency[b].push(a);
        }
    }
    adjacency
}

/// Louvain community detection. Deterministic for a given input. Returns one
/// cluster per node; cluster 0 is the largest.
pub(crate) fn clusters(n: usize, edges: &[(usize, usize)]) -> Vec<u32> {
    const MAX_LEVELS: usize = 8;
    const MAX_PASSES: usize = 12;

    // The graph being coarsened: weighted adjacency per super-node.
    let mut graph: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    for &(a, b) in edges {
        if a != b {
            graph[a].push((b, 1.0));
            graph[b].push((a, 1.0));
        }
    }
    let mut membership: Vec<usize> = (0..n).collect();

    for _ in 0..MAX_LEVELS {
        let size = graph.len();
        let total: f64 = graph.iter().flatten().map(|(_, w)| w).sum();
        if total == 0.0 {
            break;
        }
        let degree: Vec<f64> = graph.iter().map(|a| a.iter().map(|(_, w)| w).sum()).collect();
        let mut community: Vec<usize> = (0..size).collect();
        let mut community_degree = degree.clone();
        let mut weight_to = vec![0.0f64; size];
        let mut touched: Vec<usize> = Vec::new();
        let mut improved_level = false;

        for _ in 0..MAX_PASSES {
            let mut moved = false;
            for node in 0..size {
                let current = community[node];
                for &(other, w) in &graph[node] {
                    if other != node {
                        let c = community[other];
                        if weight_to[c] == 0.0 {
                            touched.push(c);
                        }
                        weight_to[c] += w;
                    }
                }
                community_degree[current] -= degree[node];
                let gain = |c: usize| weight_to[c] - community_degree[c] * degree[node] / total;
                let (mut best, mut best_gain) = (current, gain(current));
                for &c in &touched {
                    let g = gain(c);
                    if g > best_gain + 1e-12
                        || (g > best_gain - 1e-12 && c < best && g >= best_gain)
                    {
                        (best, best_gain) = (c, g);
                    }
                }
                community_degree[best] += degree[node];
                if best != current {
                    community[node] = best;
                    moved = true;
                    improved_level = true;
                }
                for c in touched.drain(..) {
                    weight_to[c] = 0.0;
                }
            }
            if !moved {
                break;
            }
        }
        if !improved_level {
            break;
        }

        // Renumber communities and fold each into one super-node.
        let mut renumber = vec![usize::MAX; size];
        let mut count = 0;
        for &c in &community {
            if renumber[c] == usize::MAX {
                renumber[c] = count;
                count += 1;
            }
        }
        for m in membership.iter_mut() {
            *m = renumber[community[*m]];
        }
        let mut folded: Vec<std::collections::BTreeMap<usize, f64>> =
            vec![Default::default(); count];
        for (node, neighbours) in graph.iter().enumerate() {
            let from = renumber[community[node]];
            for &(other, w) in neighbours {
                *folded[from].entry(renumber[community[other]]).or_default() += w;
            }
        }
        graph = folded.into_iter().map(|m| m.into_iter().collect()).collect();
    }

    // Number clusters by size, largest first; ties by first appearance.
    let groups = membership.iter().copied().max().map_or(0, |m| m + 1);
    let mut sizes = vec![(0usize, usize::MAX); groups];
    for (node, &m) in membership.iter().enumerate() {
        sizes[m].0 += 1;
        sizes[m].1 = sizes[m].1.min(node);
    }
    let mut order: Vec<usize> = (0..groups).collect();
    order.sort_by_key(|&g| (std::cmp::Reverse(sizes[g].0), sizes[g].1));
    let mut label = vec![0u32; groups];
    for (rank, &g) in order.iter().enumerate() {
        label[g] = rank as u32;
    }
    membership.into_iter().map(|m| label[m]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pagerank_favours_the_hub_and_sums_to_one() {
        let star = [(0, 1), (0, 2), (0, 3), (0, 4)];
        let r = pagerank(6, &star); // node 5 is isolated
        assert!(r[0] > r[1] * 2.0, "{r:?}");
        assert!(r[5] < r[1]);
        assert!((r.iter().sum::<f32>() - 1.0).abs() < 1e-3);
        assert!(pagerank(0, &[]).is_empty());
    }

    #[test]
    fn clusters_find_two_cliques_joined_by_a_bridge() {
        let mut edges = Vec::new();
        for group in [[0, 1, 2, 3], [4, 5, 6, 7]] {
            for (i, a) in group.iter().enumerate() {
                for b in &group[i + 1..] {
                    edges.push((*a, *b));
                }
            }
        }
        edges.push((3, 4));
        let c = clusters(9, &edges); // node 8 stands alone
        assert!(c[0] == c[1] && c[1] == c[2] && c[2] == c[3], "{c:?}");
        assert!(c[4] == c[5] && c[5] == c[6] && c[6] == c[7], "{c:?}");
        assert_ne!(c[0], c[4]);
        assert_ne!(c[8], c[0]);
        assert_ne!(c[8], c[4]);
        assert!(c[0] < c[8] && c[4] < c[8], "largest first: {c:?}");
    }

    #[test]
    fn clusters_handle_empty_and_edgeless_graphs() {
        assert!(clusters(0, &[]).is_empty());
        assert_eq!(clusters(3, &[]), [0, 1, 2]);
    }
}
