// Left-Right (Brandes 2009) planarity test, ported faithfully from the
// canonical NetworkX implementation (networkx/algorithms/planarity.py:
// LRPlanarity, ConflictPair, Interval, PlanarEmbedding).
//
// Produces a combinatorial planar embedding (rotation system) when planar.
// Pure TypeScript, no dependencies. Recursive DFS replaced with the same
// explicit-stack iterative DFS NetworkX uses.

export type EdgePair = [string, string];
export type Embedding = Map<string, string[]>;

// A directed edge is a tuple [u, w].
type Edge = [string, string];

// Edge -> string key for maps that are keyed by an edge.
function ek(e: Edge): string {
  return e[0] + "\0" + e[1];
}

function edgeEq(a: Edge | null, b: Edge | null): boolean {
  if (a === null || b === null) return a === b;
  return a[0] === b[0] && a[1] === b[1];
}

// ---------------------------------------------------------------------------
// Interval / ConflictPair
// ---------------------------------------------------------------------------

class Interval {
  low: Edge | null;
  high: Edge | null;
  constructor(low: Edge | null = null, high: Edge | null = null) {
    this.low = low;
    this.high = high;
  }
  empty(): boolean {
    return this.low === null && this.high === null;
  }
  copy(): Interval {
    return new Interval(this.low, this.high);
  }
  // True if interval conflicts with edge b.
  conflicting(b: Edge, st: LRPlanarity): boolean {
    return !this.empty() && st.lowptOf(this.high as Edge) > st.lowptOf(b);
  }
}

class ConflictPair {
  left: Interval;
  right: Interval;
  constructor(left: Interval = new Interval(), right: Interval = new Interval()) {
    this.left = left;
    this.right = right;
  }
  swap(): void {
    const t = this.left;
    this.left = this.right;
    this.right = t;
  }
  lowest(st: LRPlanarity): number {
    if (this.left.empty()) return st.lowptOf(this.right.low as Edge);
    if (this.right.empty()) return st.lowptOf(this.left.low as Edge);
    return Math.min(
      st.lowptOf(this.left.low as Edge),
      st.lowptOf(this.right.low as Edge),
    );
  }
}

function topOfStack<T>(l: T[]): T | null {
  if (l.length === 0) return null;
  return l[l.length - 1];
}

// ---------------------------------------------------------------------------
// PlanarEmbedding: ordered-dict of cw/ccw pointers, ported from NetworkX.
// ---------------------------------------------------------------------------

type HalfEdge = { cw: string; ccw: string };

class PlanarEmbedding {
  // node -> ordered Map(neighbor -> {cw, ccw}); insertion order is significant.
  succ: Map<string, Map<string, HalfEdge>> = new Map();

  addNode(v: string): void {
    if (!this.succ.has(v)) this.succ.set(v, new Map());
  }

  private static lastKey(m: Map<string, HalfEdge>): string {
    let last: string = "";
    for (const k of m.keys()) last = k;
    return last;
  }

  addHalfEdge(
    startNode: string,
    endNode: string,
    opts: { cw?: string | null; ccw?: string | null } = {},
  ): void {
    const cw = opts.cw ?? null;
    const ccw = opts.ccw ?? null;
    let succs = this.succ.get(startNode);
    if (succs === undefined) {
      succs = new Map();
      this.succ.set(startNode, succs);
    }
    this.addNode(endNode);

    if (succs.size > 0) {
      const leftmost = PlanarEmbedding.lastKey(succs);
      let moveLeftmost: boolean;
      if (cw !== null) {
        if (!succs.has(cw)) throw new Error("Invalid clockwise reference node.");
        if (ccw !== null) throw new Error("Only one of cw/ccw can be specified.");
        const refCcw = succs.get(cw)!.ccw;
        succs.set(endNode, { cw, ccw: refCcw });
        succs.get(refCcw)!.cw = endNode;
        succs.get(cw)!.ccw = endNode;
        moveLeftmost = cw !== leftmost;
      } else if (ccw !== null) {
        if (!succs.has(ccw)) throw new Error("Invalid ccw reference node.");
        const refCw = succs.get(ccw)!.cw;
        succs.set(endNode, { cw: refCw, ccw });
        succs.get(refCw)!.ccw = endNode;
        succs.get(ccw)!.cw = endNode;
        moveLeftmost = true;
      } else {
        throw new Error("Node already has out-half-edge(s); cw or ccw required.");
      }
      if (moveLeftmost) {
        const val = succs.get(leftmost)!;
        succs.delete(leftmost);
        succs.set(leftmost, val);
      }
    } else {
      if (cw !== null || ccw !== null) throw new Error("Invalid reference node.");
      succs.set(endNode, { cw: endNode, ccw: endNode });
    }
  }

  addHalfEdgeFirst(startNode: string, endNode: string): void {
    const succs = this.succ.get(startNode);
    const leftmost =
      succs && succs.size > 0 ? PlanarEmbedding.lastKey(succs) : null;
    this.addHalfEdge(startNode, endNode, { cw: leftmost });
  }

  // Neighbors of v in clockwise (rotation) order.
  neighborsCwOrder(v: string): string[] {
    const succs = this.succ.get(v);
    if (succs === undefined || succs.size === 0) return [];
    const start = PlanarEmbedding.lastKey(succs);
    const out: string[] = [start];
    let cur = succs.get(start)!.cw;
    while (start !== cur) {
      out.push(cur);
      cur = succs.get(cur)!.cw;
    }
    return out;
  }
}

// ---------------------------------------------------------------------------
// LRPlanarity
// ---------------------------------------------------------------------------

class LRPlanarity {
  nodes: string[]; // node iteration order
  adjs: Map<string, string[]> = new Map(); // undirected adjacency (dfs order)
  order: number;
  size: number;

  roots: string[] = [];
  height: Map<string, number | null> = new Map();
  lowpt: Map<string, number> = new Map();
  lowpt2: Map<string, number> = new Map();
  nestingDepth: Map<string, number> = new Map();
  parentEdge: Map<string, Edge | null> = new Map();

  // Oriented DFS graph.
  dgSucc: Map<string, string[]> = new Map();
  dgEdgeSet: Set<string> = new Set();
  dgEdges: Edge[] = [];

  orderedAdjs: Map<string, Edge[]> = new Map();

  ref: Map<string, Edge | null> = new Map();
  side: Map<string, number> = new Map();

  S: ConflictPair[] = [];
  stackBottom: Map<string, ConflictPair | null> = new Map();
  lowptEdge: Map<string, Edge> = new Map();

  leftRef: Map<string, string> = new Map();
  rightRef: Map<string, string> = new Map();

  embedding: PlanarEmbedding = new PlanarEmbedding();

  constructor(nodes: string[], edges: EdgePair[]) {
    this.nodes = nodes.slice();
    for (const v of nodes) {
      this.adjs.set(v, []);
      this.height.set(v, null);
      this.parentEdge.set(v, null);
      this.dgSucc.set(v, []);
    }
    // Build undirected adjacency in edge order (no self-loops; deduped already).
    let m = 0;
    for (const [a, b] of edges) {
      this.adjs.get(a)!.push(b);
      this.adjs.get(b)!.push(a);
      m++;
    }
    this.order = nodes.length;
    this.size = m;
  }

  // ----- small typed accessors -----
  lowptOf(e: Edge): number {
    return this.lowpt.get(ek(e))!;
  }
  private heightOf(v: string): number {
    return this.height.get(v)! as number;
  }
  private refGet(e: Edge): Edge | null {
    return this.ref.get(ek(e)) ?? null;
  }
  private refSet(e: Edge, val: Edge | null): void {
    this.ref.set(ek(e), val);
  }
  private sideOf(e: Edge | null): number {
    if (e === null) return 1;
    return this.side.get(ek(e)) ?? 1;
  }
  private dgHas(v: string, w: string): boolean {
    return this.dgEdgeSet.has(v + "\0" + w);
  }
  private dgAddEdge(v: string, w: string): void {
    this.dgSucc.get(v)!.push(w);
    this.dgEdgeSet.add(v + "\0" + w);
    this.dgEdges.push([v, w]);
  }

  // ----- main -----
  lrPlanarity(): PlanarEmbedding | null {
    if (this.order > 2 && this.size > 3 * this.order - 6) {
      return null; // too many edges => non-planar
    }

    // orientation
    for (const v of this.nodes) {
      if (this.height.get(v) === null) {
        this.height.set(v, 0);
        this.roots.push(v);
        this.dfsOrientation(v);
      }
    }

    // sort adjacency lists by nesting depth
    for (const v of this.nodes) {
      const succs = this.dgSucc.get(v)!.map((w) => [v, w] as Edge);
      succs.sort((a, b) => this.nestingDepth.get(ek(a))! - this.nestingDepth.get(ek(b))!);
      this.orderedAdjs.set(v, succs);
    }

    // testing
    for (const v of this.roots) {
      if (!this.dfsTesting(v)) return null;
    }

    // resolve signs
    for (const e of this.dgEdges) {
      this.nestingDepth.set(ek(e), this.sign(e) * this.nestingDepth.get(ek(e))!);
    }

    for (const v of this.nodes) this.embedding.addNode(v);
    for (const v of this.nodes) {
      const succs = this.dgSucc.get(v)!.map((w) => [v, w] as Edge);
      succs.sort((a, b) => this.nestingDepth.get(ek(a))! - this.nestingDepth.get(ek(b))!);
      this.orderedAdjs.set(v, succs);
      let previousNode: string | null = null;
      for (const e of succs) {
        const w = e[1];
        this.embedding.addHalfEdge(v, w, { ccw: previousNode });
        previousNode = w;
      }
    }

    for (const v of this.roots) this.dfsEmbedding(v);

    return this.embedding;
  }

  private dfsOrientation(vStart: string): void {
    const dfsStack: string[] = [vStart];
    const ind: Map<string, number> = new Map();
    const skipInit: Set<string> = new Set();
    const getInd = (v: string) => ind.get(v) ?? 0;

    while (dfsStack.length > 0) {
      const v = dfsStack.pop()!;
      const e = this.parentEdge.get(v) ?? null;

      while (getInd(v) < this.adjs.get(v)!.length) {
        const w = this.adjs.get(v)![getInd(v)];
        const vw: Edge = [v, w];
        const vwk = ek(vw);

        if (!skipInit.has(vwk)) {
          if (this.dgHas(v, w) || this.dgHas(w, v)) {
            ind.set(v, getInd(v) + 1);
            continue;
          }
          this.dgAddEdge(v, w);
          this.lowpt.set(vwk, this.heightOf(v));
          this.lowpt2.set(vwk, this.heightOf(v));
          if (this.height.get(w) === null) {
            this.parentEdge.set(w, vw);
            this.height.set(w, this.heightOf(v) + 1);
            dfsStack.push(v);
            dfsStack.push(w);
            skipInit.add(vwk);
            break;
          } else {
            this.lowpt.set(vwk, this.heightOf(w));
          }
        }

        // determine nesting depth
        this.nestingDepth.set(vwk, 2 * this.lowpt.get(vwk)!);
        if (this.lowpt2.get(vwk)! < this.heightOf(v)) {
          this.nestingDepth.set(vwk, this.nestingDepth.get(vwk)! + 1);
        }

        // update lowpoints of parent edge e
        if (e !== null) {
          const ekk = ek(e);
          if (this.lowpt.get(vwk)! < this.lowpt.get(ekk)!) {
            this.lowpt2.set(ekk, Math.min(this.lowpt.get(ekk)!, this.lowpt2.get(vwk)!));
            this.lowpt.set(ekk, this.lowpt.get(vwk)!);
          } else if (this.lowpt.get(vwk)! > this.lowpt.get(ekk)!) {
            this.lowpt2.set(ekk, Math.min(this.lowpt2.get(ekk)!, this.lowpt.get(vwk)!));
          } else {
            this.lowpt2.set(ekk, Math.min(this.lowpt2.get(ekk)!, this.lowpt2.get(vwk)!));
          }
        }

        ind.set(v, getInd(v) + 1);
      }
    }
  }

  private dfsTesting(vStart: string): boolean {
    const dfsStack: string[] = [vStart];
    const ind: Map<string, number> = new Map();
    const skipInit: Set<string> = new Set();
    const getInd = (v: string) => ind.get(v) ?? 0;

    while (dfsStack.length > 0) {
      const v = dfsStack.pop()!;
      const e = this.parentEdge.get(v) ?? null;
      let skipFinal = false;
      const oadj = this.orderedAdjs.get(v)!;

      while (getInd(v) < oadj.length) {
        const ei = oadj[getInd(v)];
        const eik = ek(ei);
        const w = ei[1];

        if (!skipInit.has(eik)) {
          this.stackBottom.set(eik, topOfStack(this.S));
          if (edgeEq(ei, this.parentEdge.get(w) ?? null)) {
            dfsStack.push(v);
            dfsStack.push(w);
            skipInit.add(eik);
            skipFinal = true;
            break;
          } else {
            this.lowptEdge.set(eik, ei);
            this.S.push(new ConflictPair(new Interval(), new Interval(ei, ei)));
          }
        }

        if (this.lowpt.get(eik)! < this.heightOf(v)) {
          if (w === oadj[0][1]) {
            // e is the parent edge of v; non-null here
            this.lowptEdge.set(ek(e as Edge), this.lowptEdge.get(eik)!);
          } else {
            if (!this.addConstraints(ei, e as Edge)) return false;
          }
        }

        ind.set(v, getInd(v) + 1);
      }

      if (!skipFinal) {
        if (e !== null) this.removeBackEdges(e);
      }
    }

    return true;
  }

  private addConstraints(ei: Edge, e: Edge): boolean {
    const P = new ConflictPair(new Interval(), new Interval());
    const eik = ek(ei);
    const ekk = ek(e);

    // merge return edges of ei into P.right
    while (true) {
      const Q = this.S.pop()!;
      if (!Q.left.empty()) Q.swap();
      if (!Q.left.empty()) return false;
      if (this.lowptOf(Q.right.low as Edge) > this.lowpt.get(ekk)!) {
        if (P.right.empty()) {
          P.right = Q.right.copy();
        } else {
          this.refSet(P.right.low as Edge, Q.right.high);
        }
        P.right.low = Q.right.low;
      } else {
        this.refSet(Q.right.low as Edge, this.lowptEdge.get(ekk)!);
      }
      if (topOfStack(this.S) === (this.stackBottom.get(eik) ?? null)) break;
    }

    // merge conflicting return edges of e1,...,ei-1 into P.left
    while (
      (topOfStack(this.S) as ConflictPair).left.conflicting(ei, this) ||
      (topOfStack(this.S) as ConflictPair).right.conflicting(ei, this)
    ) {
      const Q = this.S.pop()!;
      if (Q.right.conflicting(ei, this)) Q.swap();
      if (Q.right.conflicting(ei, this)) return false;
      this.refSet(P.right.low as Edge, Q.right.high);
      if (Q.right.low !== null) P.right.low = Q.right.low;
      if (P.left.empty()) {
        P.left = Q.left.copy();
      } else {
        this.refSet(P.left.low as Edge, Q.left.high);
      }
      P.left.low = Q.left.low;
    }

    if (!(P.left.empty() && P.right.empty())) this.S.push(P);
    return true;
  }

  private removeBackEdges(e: Edge): void {
    const u = e[0];
    const hu = this.heightOf(u);

    // drop entire conflict pairs
    while (this.S.length > 0 && (topOfStack(this.S) as ConflictPair).lowest(this) === hu) {
      const P = this.S.pop()!;
      if (P.left.low !== null) this.side.set(ek(P.left.low), -1);
    }

    if (this.S.length > 0) {
      const P = this.S.pop()!;
      // trim left interval
      while (P.left.high !== null && P.left.high[1] === u) {
        P.left.high = this.refGet(P.left.high);
      }
      if (P.left.high === null && P.left.low !== null) {
        this.refSet(P.left.low, P.right.low);
        this.side.set(ek(P.left.low), -1);
        P.left.low = null;
      }
      // trim right interval
      while (P.right.high !== null && P.right.high[1] === u) {
        P.right.high = this.refGet(P.right.high);
      }
      if (P.right.high === null && P.right.low !== null) {
        this.refSet(P.right.low, P.left.low);
        this.side.set(ek(P.right.low), -1);
        P.right.low = null;
      }
      this.S.push(P);
    }

    // side of e is side of a highest return edge
    if (this.lowpt.get(ek(e))! < hu) {
      const top = topOfStack(this.S) as ConflictPair;
      const hl = top.left.high;
      const hr = top.right.high;
      if (hl !== null && (hr === null || this.lowptOf(hl) > this.lowptOf(hr))) {
        this.refSet(e, hl);
      } else {
        this.refSet(e, hr);
      }
    }
  }

  private dfsEmbedding(vStart: string): void {
    const dfsStack: string[] = [vStart];
    const ind: Map<string, number> = new Map();
    const getInd = (v: string) => ind.get(v) ?? 0;

    while (dfsStack.length > 0) {
      const v = dfsStack.pop()!;
      const oadj = this.orderedAdjs.get(v)!;

      while (getInd(v) < oadj.length) {
        const ei = oadj[getInd(v)];
        const w = ei[1];
        ind.set(v, getInd(v) + 1);

        if (edgeEq(ei, this.parentEdge.get(w) ?? null)) {
          this.embedding.addHalfEdgeFirst(w, v);
          this.leftRef.set(v, w);
          this.rightRef.set(v, w);
          dfsStack.push(v);
          dfsStack.push(w);
          break;
        } else {
          if (this.sideOf(ei) === 1) {
            this.embedding.addHalfEdge(w, v, { ccw: this.rightRef.get(w)! });
          } else {
            this.embedding.addHalfEdge(w, v, { cw: this.leftRef.get(w)! });
            this.leftRef.set(w, v);
          }
        }
      }
    }
  }

  // resolve relative side to absolute side
  private sign(eStart: Edge): number {
    const dfsStack: Edge[] = [eStart];
    const oldRef: Map<string, Edge | null> = new Map();
    let e: Edge = eStart;

    while (dfsStack.length > 0) {
      e = dfsStack.pop()!;
      const r = this.refGet(e);
      if (r !== null) {
        dfsStack.push(e);
        dfsStack.push(r);
        oldRef.set(ek(e), r);
        this.refSet(e, null);
      } else {
        const or = oldRef.get(ek(e)) ?? null;
        this.side.set(ek(e), this.sideOf(e) * this.sideOf(or));
      }
    }
    return this.sideOf(e);
  }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export function lrPlanarity(
  nodeIds: string[],
  edges: EdgePair[],
): { isPlanar: boolean; embedding: Embedding | null } {
  // Dedupe node ids, preserve order.
  const seenNode = new Set<string>();
  const nodes: string[] = [];
  for (const v of nodeIds) {
    if (!seenNode.has(v)) {
      seenNode.add(v);
      nodes.push(v);
    }
  }

  // Dedupe edges (undirected), drop self-loops and parallels. Edges may
  // reference nodes not in nodeIds — include them as nodes too.
  const seenEdge = new Set<string>();
  const cleanEdges: EdgePair[] = [];
  for (const [a0, b0] of edges) {
    if (a0 === b0) continue; // self-loop
    if (!seenNode.has(a0)) {
      seenNode.add(a0);
      nodes.push(a0);
    }
    if (!seenNode.has(b0)) {
      seenNode.add(b0);
      nodes.push(b0);
    }
    const key = a0 < b0 ? a0 + "\0" + b0 : b0 + "\0" + a0;
    if (seenEdge.has(key)) continue;
    seenEdge.add(key);
    cleanEdges.push([a0, b0]);
  }

  const st = new LRPlanarity(nodes, cleanEdges);
  const emb = st.lrPlanarity();

  if (emb === null) {
    return { isPlanar: false, embedding: null };
  }

  const out: Embedding = new Map();
  for (const v of nodes) {
    out.set(v, emb.neighborsCwOrder(v));
  }
  return { isPlanar: true, embedding: out };
}
