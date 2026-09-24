// Smoke-test for ts_load.mjs: import each ported class and run one behaviour.
import { loadTs } from "./ts_load.mjs";

const { MinHeap, BucketQueue } = await loadTs("src/core/pathfinding/algorithms/PriorityQueue.ts");
const { FlatBinaryHeap } = await loadTs("src/core/execution/utils/FlatBinaryHeap.ts");
const { BFSGrid } = await loadTs("src/core/pathfinding/algorithms/BFS.Grid.ts");
const { AStar } = await loadTs("src/core/pathfinding/algorithms/AStar.ts");

const h = new MinHeap(4);
h.push(5, 3); h.push(1, 1); h.push(9, 2); h.push(7, 1.5);
console.log("MinHeap pop:", [h.pop(), h.pop(), h.pop(), h.pop()]);

const q = new BucketQueue(3);
q.push(10, 2); q.push(20, 2); q.push(30, 0);
console.log("BucketQueue pop:", [q.pop(), q.pop(), q.pop(), q.pop()]);

const fh = new FlatBinaryHeap(2);
for (const [t, p] of [[5, 3], [1, 1], [9, 2], [7, 1.5], [3, 0.5]]) fh.enqueue(t, p);
console.log("FlatBinaryHeap pop:", [fh.dequeue(), fh.dequeue(), fh.dequeue()]);

const g = new BFSGrid(9);
const order = [];
g.search(3, 3, 4, Infinity, () => true, (n) => { order.push(n); });
console.log("BFSGrid order:", order.join(","));

// AStar through the dependency-rewritten loader (5x5 grid, 4-neighbours,
// unit cost, manhattan heuristic; tiles 6/11/12/13 blocked).
const GW = 5;
const blocked = new Set([6, 11, 12, 13]);
const gridAdapter = {
  neighbors(node, buffer) {
    let n = 0;
    const x = node % GW;
    if (node >= GW && !blocked.has(node - GW)) buffer[n++] = node - GW;
    if (node < GW * GW - GW && !blocked.has(node + GW)) buffer[n++] = node + GW;
    if (x > 0 && !blocked.has(node - 1)) buffer[n++] = node - 1;
    if (x < GW - 1 && !blocked.has(node + 1)) buffer[n++] = node + 1;
    return n;
  },
  cost: () => 1,
  heuristic(node, goal) {
    return (
      Math.abs((node % GW) - (goal % GW)) +
      Math.abs(Math.floor(node / GW) - Math.floor(goal / GW))
    );
  },
  numNodes: () => GW * GW,
  maxPriority: () => GW * GW,
  maxNeighbors: () => 4,
};
const astar = new AStar({ adapter: gridAdapter });
console.log("AStar 0->24:", JSON.stringify(astar.findPath(0, 24)));
console.log("AStar 0->12 (blocked):", JSON.stringify(astar.findPath(0, 12)));

// Iteration cap must return null (gives up), not spin.
const capped = new AStar({ adapter: gridAdapter, maxIterations: 3 });
console.log("AStar capped 0->24:", JSON.stringify(capped.findPath(0, 24)));

// Multi-start.
console.log("AStar [1,23]->7:", JSON.stringify(astar.findPath([1, 23], 7)));

// AStarRail through the real GameMapImpl (terrain byte: bit7 land, bit6
// shoreline, bit5 ocean, bits0-4 magnitude).
const { GameMapImpl } = await loadTs("src/core/game/GameMap.ts");
const { AStarRail } = await loadTs("src/core/pathfinding/algorithms/AStar.Rail.ts");
const rows = ["LLsLL", "LLsLL", "LLsLL", "LLsLL", "LLsLL"];
const bytes = { L: 0x85, s: 0x40 };
const terrain = Uint8Array.from(rows.join("").split("").map((c) => bytes[c]));
const gm = new GameMapImpl(5, 5, terrain, 20);
const rail = new AStarRail(gm);
console.log("AStarRail 0->4:", JSON.stringify(rail.findPath(0, 4)));

console.log("ALL OK");
