# 图计算 (Graph Computing)

Valkyrie 提供了完整的图数据结构和算法库，支持各种图计算任务，从简单的图遍历到复杂的网络分析。

## 基本图类型

### 无向图

```valkyrie
use graph::*

# 创建无向图
let mut graph = UndirectedGraph::new()

# 添加节点
let node_a = graph.add_node("A")
let node_b = graph.add_node("B")
let node_c = graph.add_node("C")

# 添加边
graph.add_edge(node_a, node_b, 1.0)  # 权重为1.0的边
graph.add_edge(node_b, node_c, 2.0)
graph.add_edge(node_a, node_c, 3.0)

# 图的基本信息
let node_count = graph.node_count()
let edge_count = graph.edge_count()
let neighbors = graph.neighbors(node_a)
```

### 有向图

```valkyrie
# 创建有向图
let mut digraph = DirectedGraph::new()

# 添加节点和边
let start = digraph.add_node("start")
let middle = digraph.add_node("middle")
let end = digraph.add_node("end")

digraph.add_edge(start, middle, 1.0)
digraph.add_edge(middle, end, 2.0)
digraph.add_edge(start, end, 5.0)  # 直接路径

# 有向图特有操作
let in_degree = digraph.in_degree(end)
let out_degree = digraph.out_degree(start)
let predecessors = digraph.predecessors(end)
let successors = digraph.successors(start)
```

### 加权图

```valkyrie
# 自定义边权重类型
class EdgeWeight {
    distance: f64,
    cost: f64,
    capacity: i32,
}

let mut weighted_graph = WeightedGraph::<String, EdgeWeight>::new()

let city_a = weighted_graph.add_node("CityA")
let city_b = weighted_graph.add_node("CityB")

weighted_graph.add_edge(city_a, city_b, EdgeWeight {
    distance: 100.0,
    cost: 50.0,
    capacity: 1000
})
```

## 图算法

### 遍历算法

```valkyrie
# 深度优先搜索 (DFS)
let dfs_result = graph.dfs(start_node)
let dfs_tree = graph.dfs_tree(start_node)

# 广度优先搜索 (BFS)
let bfs_result = graph.bfs(start_node)
let bfs_levels = graph.bfs_levels(start_node)

# 自定义遍历
let custom_traversal = graph.traverse(start_node, |node, depth| {
    print("Visiting node {} at depth {}", node, depth)
    depth < 5  # 限制遍历深度
})
```

### 最短路径算法

```valkyrie
# Dijkstra算法
let shortest_paths = graph.dijkstra(start_node)
let path_to_target = graph.shortest_path(start_node, target_node)

# A*算法（需要启发式函数）
let heuristic = { $node -> estimate_distance_to_goal($node) }
let astar_path = graph.astar(start_node, target_node, heuristic)

# Floyd-Warshall算法（所有节点对之间的最短路径）
let all_pairs_shortest = graph.floyd_warshall()

# Bellman-Ford算法（处理负权重）
let bellman_ford_result = graph.bellman_ford(start_node)
match result {
    Fine { value: distances } => print("Shortest distances: {:?}", distances),
    Fail { error: NegativeCycle } => print("Graph contains negative cycle")
}
```

### 连通性算法

```valkyrie
# 连通分量
let connected_components = graph.connected_components()
let is_connected = graph.is_connected()

# 强连通分量（有向图）
let strongly_connected = digraph.strongly_connected_components()
let condensation = digraph.condensation()  # 强连通分量的DAG

# 桥和割点
let bridges = graph.find_bridges()
let articulation_points = graph.find_articulation_points()
```

### 最小生成树

```valkyrie
# Kruskal算法
let mst_kruskal = graph.minimum_spanning_tree_kruskal()

# Prim算法
let mst_prim = graph.minimum_spanning_tree_prim(start_node)

# 最小生成森林
let msf = graph.minimum_spanning_forest()
```

### 网络流算法

```valkyrie
# 最大流算法
let max_flow = graph.max_flow(source, sink)
let flow_value = max_flow.value
let flow_edges = max_flow.edges

# Ford-Fulkerson算法
let ford_fulkerson = graph.ford_fulkerson(source, sink)

# 最小割
let min_cut = graph.min_cut(source, sink)

# 最小费用最大流
let min_cost_max_flow = graph.min_cost_max_flow(source, sink)
```

## 图分析

### 中心性度量

```valkyrie
# 度中心性
let degree_centrality = graph.degree_centrality()

# 接近中心性
let closeness_centrality = graph.closeness_centrality()

# 介数中心性
let betweenness_centrality = graph.betweenness_centrality()

# PageRank
let pagerank = graph.pagerank(0.85, 100)  # 阻尼因子0.85，最大迭代100次

# 特征向量中心性
let eigenvector_centrality = graph.eigenvector_centrality()
```

### 社区检测

```valkyrie
# Louvain算法
let communities_louvain = graph.louvain_communities()

# 标签传播算法
let communities_label_prop = graph.label_propagation_communities()

# 模块度优化
let modularity = graph.modularity(communities_louvain)

# 层次聚类
let dendrogram = graph.hierarchical_clustering()
let communities_at_level = dendrogram.communities_at_level(0.5)
```

### 图统计

```valkyrie
# 基本统计
let density = graph.density()
let diameter = graph.diameter()
let radius = graph.radius()
let average_path_length = graph.average_path_length()

# 度分布
let degree_distribution = graph.degree_distribution()
let degree_histogram = graph.degree_histogram()

# 聚类系数
let clustering_coefficient = graph.clustering_coefficient()
let local_clustering = graph.local_clustering_coefficient(node)

# 同配性
let assortativity = graph.assortativity()
```

## 特殊图类型

### 二分图

```valkyrie
# 创建二分图
let mut bipartite = BipartiteGraph::new()

# 添加两个集合的节点
let left_nodes = bipartite.add_left_nodes(["L1", "L2", "L3"])
let right_nodes = bipartite.add_right_nodes(["R1", "R2"])

# 添加边（只能在两个集合之间）
bipartite.add_edge(left_nodes[0], right_nodes[0], 1.0)
bipartite.add_edge(left_nodes[1], right_nodes[1], 1.0)

# 二分图算法
let is_bipartite = graph.is_bipartite()
let bipartite_matching = bipartite.maximum_matching()
let vertex_cover = bipartite.minimum_vertex_cover()
```

### 平面图

```valkyrie
# 平面性检测
let is_planar = graph.is_planar()

# 平面嵌入
let planar_embedding = graph.planar_embedding()

# 面的计算
let faces = planar_embedding.faces()
let outer_face = planar_embedding.outer_face()
```

### 树和森林

```valkyrie
# 树的特殊操作
let tree = Tree::from_edges(edges)

# 树的遍历
let preorder = tree.preorder_traversal(root)
let postorder = tree.postorder_traversal(root)
let inorder = tree.inorder_traversal(root)  # 对于二叉树

# 树的性质
let height = tree.height()
let leaves = tree.leaves()
let internal_nodes = tree.internal_nodes()

# 最近公共祖先
let lca = tree.lowest_common_ancestor(node1, node2)
```

## 动态图

```valkyrie
# 支持动态更新的图
let mut dynamic_graph = DynamicGraph::new()

# 时间戳边
dynamic_graph.add_temporal_edge(node_a, node_b, 1.0, timestamp: 100)
dynamic_graph.add_temporal_edge(node_b, node_c, 2.0, timestamp: 200)

# 查询特定时间的图状态
let snapshot_at_150 = dynamic_graph.snapshot_at(150)

# 时间窗口查询
let active_edges = dynamic_graph.edges_in_window(100, 300)

# 动态算法
let temporal_paths = dynamic_graph.temporal_paths(start, end, start_time, end_time)
```

## 图的可视化

```valkyrie
use graph::visualization::*

# 布局算法
let spring_layout = graph.spring_layout(iterations: 1000)
let circular_layout = graph.circular_layout()
let hierarchical_layout = digraph.hierarchical_layout()

# 导出为可视化格式
let dot_format = graph.to_dot()
let graphml_format = graph.to_graphml()
let json_format = graph.to_json()

# 交互式可视化
let visualization = GraphVisualization::new(graph)
visualization.set_node_color({ $node -> 
    if communities[0].contains($node) { "red" } else { "blue" }
})
visualization.set_edge_width { $edge -> $edge.weight * 2.0 }
visualization.render("output.svg")
```

## 并行图算法

```valkyrie
use graph::parallel::*

# 并行BFS
let parallel_bfs = graph.parallel_bfs(start_node, num_threads: 8)

# 并行PageRank
let parallel_pagerank = graph.parallel_pagerank(0.85, 100, num_threads: 8)

# 并行连通分量
let parallel_components = graph.parallel_connected_components()

# 分布式图计算
let distributed_graph = DistributedGraph::from_partitions(partitions)
let distributed_pagerank = distributed_graph.distributed_pagerank()
```

## 图数据库接口

```valkyrie
# 图查询语言
let query_result = graph.query("
    MATCH (a)-[r]->(b)
    WHERE a.type = 'Person' AND r.weight > 0.5
    RETURN a.name, b.name, r.weight
")

# 路径查询
let path_query = graph.find_paths(
    start: { type: "City", name: "Beijing" },
    end: { type: "City", name: "Shanghai" },
    max_hops: 3
)

# 子图匹配
let pattern = Graph::from_edges([
    ("A", "B", "friend"),
    ("B", "C", "colleague")
])
let matches = graph.subgraph_isomorphism(pattern)
```

## 性能优化

### 内存优化

```valkyrie
# 压缩图表示
let compressed_graph = graph.compress()  # 使用压缩存储格式

# 稀疏图优化
let sparse_graph = SparseGraph::from_coo(rows, cols, values)  # COO格式
let csr_graph = sparse_graph.to_csr()  # 转换为CSR格式

# 内存映射大图
let memory_mapped = Graph::from_file_mmap("large_graph.bin")
```

### 缓存优化

```valkyrie
# 预计算常用查询
let mut cached_graph = CachedGraph::new(graph)
cached_graph.precompute_shortest_paths()  # 预计算最短路径
cached_graph.precompute_centrality()      # 预计算中心性

# 查询缓存
let cached_result = cached_graph.cached_query("shortest_path", (start, end))
```

## 最佳实践

### 1. 选择合适的图表示

```valkyrie
# 稠密图使用邻接矩阵
let dense_graph = AdjacencyMatrixGraph::new(node_count)

# 稀疏图使用邻接列表
let sparse_graph = AdjacencyListGraph::new()

# 大规模图使用压缩格式
let large_graph = CompressedGraph::new()
```

### 2. 算法选择

```valkyrie
# 根据图的特性选择算法
micro choose_shortest_path_algorithm(graph: &Graph) -> ShortestPathResult {
    if graph.has_negative_weights() {
        graph.bellman_ford(start)
    } else if graph.is_sparse() {
        graph.dijkstra(start)
    } else {
        graph.floyd_warshall()
    }
}
```

### 3. 内存管理

```valkyrie
# 流式处理大图
micro process_large_graph(graph_stream: GraphStream) {
    for chunk in graph_stream.chunks(1000) {
        let subgraph = Graph::from_edges(chunk)
        let result = subgraph.compute_metrics()
        // 处理结果，释放内存
    }
}
```

图计算为 Valkyrie 提供了处理复杂网络数据的强大能力，支持从社交网络分析到生物信息学等各种应用场景。