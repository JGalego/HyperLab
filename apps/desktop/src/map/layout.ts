/**
 * Where to draw each card.
 *
 * A small force-directed layout, of the kind Graphviz's `sfdp` does properly:
 * every card pushes every other away, every route pulls its two ends
 * together, and cards sharing a background pull together more weakly still —
 * which is what separates the Ages in a graph of Myst.
 *
 * Pure, and deterministic. The same stack lays out the same way every time,
 * because a map that rearranges itself each time you open it is a map you
 * have to re-read each time you open it.
 */

import type { Graph, GraphNode } from '../types';

/** A card, and where it ended up. */
export interface Placed {
  node: GraphNode;
  x: number;
  y: number;
}

/** A route between two cards, after the parallel ones are collapsed. */
export interface Route {
  from: number;
  to: number;
  /** How many separate `go` statements take this same path. */
  count: number;
  /** Where they are written, for the tooltip. */
  where: string[];
}

/**
 * A laid-out graph, and the space it was laid out in.
 *
 * The space grows with the number of cards rather than matching the window,
 * and the caller hands `frame` to the SVG as its `viewBox`. That is what
 * keeps a big stack legible: boxes and the gaps between them shrink
 * together, so a hundred cards get small but never pile on top of each
 * other. Scaling the positions alone — the obvious thing — draws a hundred
 * full-sized boxes in a heap.
 */
export interface Layout {
  placed: Placed[];
  routes: Route[];
  frame: { width: number; height: number };
}

/** Half the width and height of a card box, in layout units. */
export const NODE_HALF_WIDTH = 46;
export const NODE_HALF_HEIGHT = 15;

/**
 * How hard cards sharing a background hold together, relative to a route.
 *
 * Low on purpose. Grouping should show when the routes allow it and give way
 * when they do not — a background that happens to span two ends of the stack
 * should not drag them into one blob.
 */
const BACKGROUND_PULL = 0.12;

/**
 * How hard the drawing holds itself together, relative to a route.
 *
 * Sets how far the whole thing spreads: repulsion pushes outwards until this
 * balances it. Raising it makes a tighter, denser picture.
 */
const CENTRE_PULL = 0.6;

/** A card mid-simulation: where it is, and where this pass wants it to go. */
interface Point {
  node: GraphNode;
  x: number;
  y: number;
  dx: number;
  dy: number;
}

/**
 * How many passes to make.
 *
 * Every pass is O(n²), so this shrinks as the stack grows: a 1,300-card
 * stack settles roughly rather than exactly, which is the right trade for a
 * layout computed while somebody waits.
 */
function passes(count: number): number {
  return Math.max(60, Math.min(300, Math.round(9000 / Math.max(count, 1))));
}

/**
 * How far apart two cards want to be.
 *
 * Measured from the boxes themselves, so the spacing means the same thing
 * whatever the window is doing. Wide enough that a label and the arrow
 * reaching it both have room.
 */
const SPACING = NODE_HALF_WIDTH * 2 + 70;

/**
 * Lays a graph out.
 *
 * `aspect` is the shape of the space to fill — the window's width over its
 * height — so the drawing comes out roughly the shape of the hole it goes
 * in and does not waste half of it.
 */
export function layout(graph: Graph, aspect: number): Layout {
  const wanted = Number.isFinite(aspect) && aspect > 0 ? aspect : 1.6;
  const routes = collapse(graph);
  const points = seed(graph.nodes);
  if (points.length === 0) {
    return { placed: [], routes, frame: { width: SPACING, height: SPACING } };
  }
  if (points.length === 1) {
    const point = points[0];
    return {
      placed: point === undefined ? [] : [{ node: point.node, x: 0, y: 0 }],
      routes,
      frame: bounds([{ x: 0, y: 0 }]),
    };
  }

  const byId = new Map(points.map((point) => [point.node.id, point]));
  const total = passes(points.length);

  for (let pass = 0; pass < total; pass += 1) {
    for (const point of points) {
      point.dx = 0;
      point.dy = 0;
    }
    repel(points, SPACING);
    attract(routes, byId, SPACING);
    group(points, SPACING);
    hold(points, SPACING);

    // Cooling: big rearrangements early, small corrections late, so the last
    // passes tidy rather than churn.
    const limit = (SPACING / 3) * (1 - pass / total) + 1;
    for (const point of points) {
      const distance = Math.hypot(point.dx, point.dy);
      if (distance < 1e-9) continue;
      const step = Math.min(distance, limit) / distance;
      point.x += point.dx * step;
      point.y += point.dy * step;
    }
  }

  // The forces have no opinion about the shape of the paper, so they settle
  // into a blob. Stretching it towards the window's proportions fills the
  // space instead of letterboxing it — and any overlap that causes is undone
  // by the separation pass that follows.
  stretch(points, wanted);

  // The forces get the shape right and the spacing approximately right.
  // Prising the last overlaps apart afterwards is what makes it readable,
  // and no force setting reliably does it: two cards pulled together by
  // three routes each will sit on top of each other however hard they push.
  separate(points);

  const frame = bounds(points);
  return { placed: shift(points, frame), routes, frame };
}

/** Pulls the drawing towards the proportions of the window it goes in. */
function stretch(points: Point[], wanted: number): void {
  const blob = bounds(points);
  const have = blob.width / blob.height;
  if (!Number.isFinite(have) || have <= 0) return;
  // Halfway only. Going all the way turns a long thin stack into a line, and
  // the shape of the routes is worth more than the last of the whitespace.
  const by = Math.sqrt(wanted / have) ** 0.5;
  for (const point of points) {
    point.x *= by;
    point.y /= by;
  }
}

/**
 * Starting positions, on a spiral in stack order.
 *
 * The golden angle keeps them evenly spread and never collinear, which
 * matters: two cards at the same point feel no force apart and stay there.
 * Stack order means a stack that is simply a sequence starts near its answer.
 */
function seed(nodes: GraphNode[]): Point[] {
  const golden = Math.PI * (3 - Math.sqrt(5));
  return nodes.map((node, at) => {
    const radius = Math.sqrt(at + 0.5);
    return {
      node,
      x: Math.cos(at * golden) * radius,
      y: Math.sin(at * golden) * radius,
      dx: 0,
      dy: 0,
    };
  });
}

/** Every card pushes every other away, harder the closer they are. */
function repel(points: Point[], ideal: number): void {
  for (let a = 0; a < points.length; a += 1) {
    const one = points[a];
    if (one === undefined) continue;
    for (let b = a + 1; b < points.length; b += 1) {
      const other = points[b];
      if (other === undefined) continue;
      let apart = one.x - other.x;
      let down = one.y - other.y;
      let distance = Math.hypot(apart, down);
      if (distance < 0.01) {
        // Exactly coincident: nudge them apart along a fixed direction, so
        // the result stays the same on every run.
        apart = (a % 2 === 0 ? 1 : -1) * 0.01;
        down = 0.01;
        distance = Math.hypot(apart, down);
      }
      const force = (ideal * ideal) / distance;
      const pushX = (apart / distance) * force;
      const pushY = (down / distance) * force;
      one.dx += pushX;
      one.dy += pushY;
      other.dx -= pushX;
      other.dy -= pushY;
    }
  }
}

/** Every route pulls its two ends together. */
function attract(routes: Route[], byId: Map<number, Point>, ideal: number): void {
  for (const route of routes) {
    const one = byId.get(route.from);
    const other = byId.get(route.to);
    if (one === undefined || other === undefined || one === other) continue;
    const apart = one.x - other.x;
    const down = one.y - other.y;
    const distance = Math.max(Math.hypot(apart, down), 0.01);
    const force = (distance * distance) / ideal;
    const pullX = (apart / distance) * force;
    const pullY = (down / distance) * force;
    one.dx -= pullX;
    one.dy -= pullY;
    other.dx += pullX;
    other.dy += pullY;
  }
}

/**
 * Everything drifts towards the middle of the drawing.
 *
 * Without this, two parts of a stack with no route between them push each
 * other apart for ever — repulsion has no opposing force across the gap —
 * and one orphaned card ends up a mile from everything else, shrinking the
 * rest to nothing. Weak enough that it only bites at long range.
 */
function hold(points: Point[], ideal: number): void {
  let sumX = 0;
  let sumY = 0;
  for (const point of points) {
    sumX += point.x;
    sumY += point.y;
  }
  const middleX = sumX / points.length;
  const middleY = sumY / points.length;

  for (const point of points) {
    const apart = middleX - point.x;
    const down = middleY - point.y;
    const distance = Math.max(Math.hypot(apart, down), 0.01);
    const force = ((distance * distance) / ideal) * CENTRE_PULL;
    point.dx += (apart / distance) * force;
    point.dy += (down / distance) * force;
  }
}

/** Cards sharing a background drift towards its centre. */
function group(points: Point[], ideal: number): void {
  const centres = new Map<number, { x: number; y: number; count: number }>();
  for (const point of points) {
    const centre = centres.get(point.node.background) ?? { x: 0, y: 0, count: 0 };
    centre.x += point.x;
    centre.y += point.y;
    centre.count += 1;
    centres.set(point.node.background, centre);
  }
  // One background is no grouping at all — every card would be pulled to the
  // same place, which is only a weaker version of the routes.
  if (centres.size < 2) return;

  for (const point of points) {
    const centre = centres.get(point.node.background);
    if (centre === undefined || centre.count < 2) continue;
    const apart = centre.x / centre.count - point.x;
    const down = centre.y / centre.count - point.y;
    const distance = Math.max(Math.hypot(apart, down), 0.01);
    const force = ((distance * distance) / ideal) * BACKGROUND_PULL;
    point.dx += (apart / distance) * force;
    point.dy += (down / distance) * force;
  }
}

/** The gap to leave between two boxes that would otherwise touch. */
const CLEARANCE = 12;

/** Air around the whole drawing. */
const MARGIN = 20;

/**
 * Pushes overlapping boxes apart until none of them overlap.
 *
 * Boxes, not points: two cards can be far enough apart at their centres and
 * still have their labels on top of each other, because a card is much wider
 * than it is tall. Each pass moves a colliding pair the shortest way out of
 * each other, which disturbs the shape the forces found as little as
 * possible.
 */
function separate(points: Point[]): void {
  const wide = NODE_HALF_WIDTH * 2 + CLEARANCE;
  const tall = NODE_HALF_HEIGHT * 2 + CLEARANCE;

  for (let pass = 0; pass < 60; pass += 1) {
    let moved = false;
    for (let a = 0; a < points.length; a += 1) {
      const one = points[a];
      if (one === undefined) continue;
      for (let b = a + 1; b < points.length; b += 1) {
        const other = points[b];
        if (other === undefined) continue;
        const apart = other.x - one.x;
        const down = other.y - one.y;
        const overlapX = wide - Math.abs(apart);
        const overlapY = tall - Math.abs(down);
        if (overlapX <= 0 || overlapY <= 0) continue;

        moved = true;
        // Out the near side: shoving two boxes apart vertically when they are
        // side by side would undo the layout for no reason.
        if (overlapX / wide < overlapY / tall) {
          const push = (overlapX / 2) * (apart < 0 ? -1 : 1);
          one.x -= push;
          other.x += push;
        } else {
          const push = (overlapY / 2) * (down < 0 ? -1 : 1);
          one.y -= push;
          other.y += push;
        }
      }
    }
    if (!moved) break;
  }
}

/**
 * The space the drawing needs, boxes and all.
 *
 * Room for a whole box either side of the outermost centre, so nothing at
 * the edge is clipped, plus a little air.
 */
function bounds(points: { x: number; y: number }[]): { width: number; height: number } {
  const { left, right, top, bottom } = extent(points);
  return {
    width: right - left + 2 * (NODE_HALF_WIDTH + MARGIN),
    height: bottom - top + 2 * (NODE_HALF_HEIGHT + MARGIN),
  };
}

/** Moves the drawing so its own bounds start at the origin. */
function shift(points: Point[], frame: { width: number; height: number }): Placed[] {
  const { left, right, top, bottom } = extent(points);
  const across = frame.width / 2 - (left + right) / 2;
  const down = frame.height / 2 - (top + bottom) / 2;
  return points.map((point) => ({
    node: point.node,
    x: point.x + across,
    y: point.y + down,
  }));
}

function extent(points: { x: number; y: number }[]): {
  left: number;
  right: number;
  top: number;
  bottom: number;
} {
  let left = Infinity;
  let right = -Infinity;
  let top = Infinity;
  let bottom = -Infinity;
  for (const point of points) {
    left = Math.min(left, point.x);
    right = Math.max(right, point.x);
    top = Math.min(top, point.y);
    bottom = Math.max(bottom, point.y);
  }
  return { left, right, top, bottom };
}

/**
 * Collapses parallel routes into one.
 *
 * Two buttons on a background that both go home are two edges in the graph,
 * because clicking either does the thing — but they are one line on a map,
 * and drawing them twice only makes it harder to read.
 */
function collapse(graph: Graph): Route[] {
  const routes = new Map<string, Route>();
  for (const edge of graph.edges) {
    if (edge.to.kind !== 'card') continue;
    const key = `${edge.from}→${edge.to.id}`;
    const route = routes.get(key) ?? {
      from: edge.from,
      to: edge.to.id,
      count: 0,
      where: [],
    };
    route.count += 1;
    route.where.push(`${edge.via.kind} ${edge.via.id}, line ${edge.line}`);
    routes.set(key, route);
  }
  return [...routes.values()];
}
