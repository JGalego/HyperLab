/**
 * The map: a stack drawn as the routes between its cards.
 *
 * Nothing is run to build it. Every arrow was read out of a script, which is
 * why some cards carry a `?` — `go to card whicheverOneTheyPicked` has no
 * answer until it runs, and drawing a guess would be worse than drawing the
 * doubt.
 *
 * What it is for is the three lists underneath the picture: cards nothing
 * leads to, cards with no way out, and links naming a card that is not there.
 * A stack grows by copying cards and rewiring buttons, and all three are
 * invisible until somebody goes looking.
 */

import { useEffect, useMemo, useRef, useState } from 'react';

import { NODE_HALF_HEIGHT, NODE_HALF_WIDTH, layout, type Placed } from '../map/layout';
import type { Graph, GraphNode } from '../types';

interface Props {
  graph: Graph;
  /** The card showing behind the map, drawn as where you are. */
  current: number;
  onGoTo: (position: number) => void;
  onClose: () => void;
}

/** What a card's unreadable routes add up to, for its badge. */
interface Doubts {
  missing: number;
  unresolved: number;
}

export function StackMap({ graph, current, onGoTo, onClose }: Props) {
  const canvas = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 800, height: 460 });
  const [hovered, setHovered] = useState<number | null>(null);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  // The layout fills whatever room it is given, so it is re-run when the
  // window changes shape.
  useEffect(() => {
    const element = canvas.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => {
      if (entry === undefined) return;
      const { width, height } = entry.contentRect;
      if (width > 0 && height > 0) setSize({ width, height });
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  // Only the *shape* of the window feeds the layout, not its size: the SVG
  // scales the drawing to fit, so resizing must not rearrange it.
  const aspect = Math.round((size.width / size.height) * 4) / 4;
  const { placed, routes, frame } = useMemo(() => layout(graph, aspect), [graph, aspect]);
  const where = useMemo(() => new Map(placed.map((one) => [one.node.id, one])), [placed]);
  const doubts = useMemo(() => doubtsByCard(graph), [graph]);

  const stranded = graph.nodes.filter((node) => !node.reachable);
  const closed =
    graph.nodes.length > 1 ? graph.nodes.filter((n) => !n.leadsAnywhere) : [];
  const broken = graph.edges.filter((edge) => edge.to.kind === 'missing');

  const go = (node: GraphNode) => {
    onGoTo(node.position);
    onClose();
  };

  return (
    <div className="map__scrim" onMouseDown={onClose}>
      <div className="map" onMouseDown={(event) => event.stopPropagation()}>
        <div className="map__bar">
          <strong>{graph.stack}</strong>
          <span className="map__count">
            {count(graph.nodes.length, 'card')}, {count(routes.length, 'route')}
          </span>
          <span className="map__spacer" />
          <button type="button" onClick={onClose}>
            Done
          </button>
        </div>

        <p className="map__legend">
          <span>
            <b className="map__key map__key--here" /> where you are
          </span>
          <span>
            <b className="map__key map__key--stranded" /> nothing leads here
          </span>
          <span>
            <b className="map__key map__key--closed" /> no way out
          </span>
          <span>
            <code>✕</code> names a card that is not there
          </span>
          <span>
            <code>?</code> only running it would say
          </span>
        </p>

        <div className="map__canvas" ref={canvas}>
          <svg
            width={size.width}
            height={size.height}
            viewBox={`0 0 ${frame.width} ${frame.height}`}
            preserveAspectRatio="xMidYMid meet"
            role="img"
            aria-label="Card map"
          >
            <defs>
              <marker
                id="map-arrow"
                viewBox="0 0 8 8"
                refX="7"
                refY="4"
                markerWidth="6"
                markerHeight="6"
                orient="auto-start-reverse"
              >
                <path d="M 0 0 L 8 4 L 0 8 z" fill="currentColor" />
              </marker>
            </defs>

            {routes.map((route) => {
              const from = where.get(route.from);
              const to = where.get(route.to);
              if (!from || !to) return null;
              const key = `${route.from}-${route.to}`;
              const lit = hovered === route.from || hovered === route.to;
              if (route.from === route.to) {
                return (
                  <path
                    key={key}
                    className={cx(
                      'map__route',
                      'map__route--self',
                      lit && 'map__route--lit',
                    )}
                    d={selfLoop(from)}
                    markerEnd="url(#map-arrow)"
                  >
                    <title>{route.where.join('\n')}</title>
                  </path>
                );
              }
              const [x1, y1, x2, y2] = trim(from, to);
              return (
                <line
                  key={key}
                  className={cx('map__route', lit && 'map__route--lit')}
                  x1={x1}
                  y1={y1}
                  x2={x2}
                  y2={y2}
                  strokeWidth={route.count > 1 ? 2 : 1}
                  markerEnd="url(#map-arrow)"
                >
                  <title>{route.where.join('\n')}</title>
                </line>
              );
            })}

            {placed.map((one) => (
              <Box
                key={one.node.id}
                placed={one}
                here={one.node.id === current}
                doubts={doubts.get(one.node.id)}
                onEnter={() => setHovered(one.node.id)}
                onLeave={() => setHovered(null)}
                onOpen={() => go(one.node)}
              />
            ))}
          </svg>
        </div>

        <div className="map__foot">
          <Findings label="Nothing leads here" nodes={stranded} onGoTo={go} />
          <Findings label="No way out" nodes={closed} onGoTo={go} />
          <div className="map__finding">
            <h3>Links to nowhere</h3>
            {broken.length === 0 ? (
              <p className="map__none">None.</p>
            ) : (
              <ul>
                {broken.map((edge, at) => (
                  <li key={`${edge.from}-${at}`}>
                    <button
                      type="button"
                      className="map__jump"
                      onClick={() => jump(edge.from)}
                    >
                      {nameOf(graph, edge.from)}
                    </button>{' '}
                    → {edge.to.kind === 'missing' ? edge.to.wanted : ''}
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>
      </div>
    </div>
  );

  function jump(card: number) {
    const node = graph.nodes.find((one) => one.id === card);
    if (node) go(node);
  }
}

/** One card. */
function Box({
  placed,
  here,
  doubts,
  onEnter,
  onLeave,
  onOpen,
}: {
  placed: Placed;
  here: boolean;
  doubts: Doubts | undefined;
  onEnter: () => void;
  onLeave: () => void;
  onOpen: () => void;
}) {
  const { node, x, y } = placed;
  const name = node.name === '' ? `Card ${node.position}` : node.name;
  return (
    <g
      className={cx(
        'map__card',
        here && 'map__card--here',
        !node.reachable && 'map__card--stranded',
        !node.leadsAnywhere && 'map__card--closed',
      )}
      transform={`translate(${x}, ${y})`}
      onMouseEnter={onEnter}
      onMouseLeave={onLeave}
      onClick={onOpen}
      role="button"
      tabIndex={0}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') onOpen();
      }}
    >
      <title>
        {`${node.position}. ${name}`}
        {node.reachable ? '' : '\nNothing leads here from the first card.'}
        {node.leadsAnywhere ? '' : '\nNo way out of this card.'}
      </title>
      {!node.reachable && (
        <rect
          className="map__card-outer"
          x={-NODE_HALF_WIDTH - 3}
          y={-NODE_HALF_HEIGHT - 3}
          width={NODE_HALF_WIDTH * 2 + 6}
          height={NODE_HALF_HEIGHT * 2 + 6}
        />
      )}
      <rect
        x={-NODE_HALF_WIDTH}
        y={-NODE_HALF_HEIGHT}
        width={NODE_HALF_WIDTH * 2}
        height={NODE_HALF_HEIGHT * 2}
      />
      <text x={0} y={4} textAnchor="middle">
        {clip(`${node.position}. ${name}`)}
      </text>
      {doubts !== undefined && (
        <text
          className="map__doubt"
          x={NODE_HALF_WIDTH - 4}
          y={-NODE_HALF_HEIGHT + 9}
          textAnchor="end"
        >
          {'✕'.repeat(Math.min(doubts.missing, 3))}
          {'?'.repeat(Math.min(doubts.unresolved, 3))}
        </text>
      )}
    </g>
  );
}

/** One list of cards worth looking at. */
function Findings({
  label,
  nodes,
  onGoTo,
}: {
  label: string;
  nodes: GraphNode[];
  onGoTo: (node: GraphNode) => void;
}) {
  return (
    <div className="map__finding">
      <h3>{label}</h3>
      {nodes.length === 0 ? (
        <p className="map__none">None.</p>
      ) : (
        <ul>
          {nodes.map((node) => (
            <li key={node.id}>
              <button type="button" className="map__jump" onClick={() => onGoTo(node)}>
                {node.position}. {node.name === '' ? `Card ${node.position}` : node.name}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** How many routes out of each card could not be read. */
function doubtsByCard(graph: Graph): Map<number, Doubts> {
  const counted = new Map<number, Doubts>();
  for (const edge of graph.edges) {
    if (edge.to.kind === 'card') continue;
    const doubt = counted.get(edge.from) ?? { missing: 0, unresolved: 0 };
    // `go back` counts as unresolved, and for the same reason: only the visit
    // knows where it lands.
    if (edge.to.kind === 'missing') doubt.missing += 1;
    else doubt.unresolved += 1;
    counted.set(edge.from, doubt);
  }
  return counted;
}

/**
 * Where a line between two boxes should start and stop.
 *
 * Ending it at the border rather than the centre is what makes the arrowhead
 * visible: aimed at the middle, it hides under the box it points at.
 */
function trim(from: Placed, to: Placed): [number, number, number, number] {
  const apart = to.x - from.x;
  const down = to.y - from.y;
  // Whichever border the line crosses first, and never past the midpoint —
  // two boxes almost on top of each other must not produce a backwards line.
  const reach = Math.min(
    Math.abs(apart) < 1e-6 ? Infinity : (NODE_HALF_WIDTH + 2) / Math.abs(apart),
    Math.abs(down) < 1e-6 ? Infinity : (NODE_HALF_HEIGHT + 2) / Math.abs(down),
    0.5,
  );
  return [
    from.x + apart * reach,
    from.y + down * reach,
    to.x - apart * reach,
    to.y - down * reach,
  ];
}

/** A card that goes to itself: a small loop over the top of it. */
function selfLoop(at: Placed): string {
  const top = at.y - NODE_HALF_HEIGHT;
  return [
    `M ${at.x - 10} ${top}`,
    `C ${at.x - 22} ${top - 26} ${at.x + 22} ${top - 26} ${at.x + 9} ${top - 1}`,
  ].join(' ');
}

function nameOf(graph: Graph, card: number): string {
  const node = graph.nodes.find((one) => one.id === card);
  if (node === undefined) return `card ${card}`;
  return node.name === '' ? `Card ${node.position}` : node.name;
}

/** A long label loses its middle: the end is often what distinguishes it. */
function clip(label: string, most = 16): string {
  if (label.length <= most) return label;
  const head = Math.ceil((most - 1) / 2);
  return `${label.slice(0, head)}…${label.slice(label.length - (most - 1 - head))}`;
}

function count(many: number, thing: string): string {
  return `${many} ${thing}${many === 1 ? '' : 's'}`;
}

function cx(...names: (string | false | undefined)[]): string {
  return names.filter(Boolean).join(' ');
}
