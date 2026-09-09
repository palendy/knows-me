/**
 * Type declarations for react-force-graph-2d.
 * The library ships no official TypeScript types, so we declare the slice we use.
 * (Adapted from the same shim used in the jiravis project.)
 */

/* eslint-disable @typescript-eslint/no-explicit-any */

declare module "react-force-graph-2d" {
  import { MutableRefObject } from "react";

  export type NodeObject<NodeType = object> = NodeType & {
    id?: string | number;
    x?: number;
    y?: number;
    vx?: number;
    vy?: number;
    fx?: number | null;
    fy?: number | null;
  };

  export type LinkObject<LinkType = object> = LinkType & {
    source?: string | number | NodeObject<any>;
    target?: string | number | NodeObject<any>;
  };

  export interface GraphData<NodeType = any, LinkType = any> {
    nodes: NodeObject<NodeType>[];
    links: LinkObject<LinkType>[];
  }

  export interface ForceGraphMethods<NodeType = any, LinkType = any> {
    d3Force: (forceName: string, force?: any) => any;
    d3ReheatSimulation: () => void;
    centerAt: (x?: number, y?: number, duration?: number) => void;
    zoom: (scale?: number, duration?: number) => void;
    zoomToFit: (duration?: number, padding?: number) => void;
    screen2GraphCoords: (x: number, y: number) => { x: number; y: number };
    graph2ScreenCoords: (x: number, y: number) => { x: number; y: number };
  }

  export interface ForceGraphProps<NodeType = any, LinkType = any> {
    graphData?: GraphData<NodeType, LinkType>;
    width?: number;
    height?: number;
    backgroundColor?: string;
    nodeRelSize?: number;
    nodeId?: string;
    nodeLabel?: string | ((node: NodeObject<NodeType>) => string);
    nodeVal?: string | number | ((node: NodeObject<NodeType>) => number);
    nodeColor?: string | ((node: NodeObject<NodeType>) => string);
    nodeCanvasObject?: (node: NodeObject<NodeType>, ctx: CanvasRenderingContext2D, globalScale: number) => void;
    nodeCanvasObjectMode?: string | ((node: NodeObject<NodeType>) => string);
    nodePointerAreaPaint?: (node: NodeObject<NodeType>, color: string, ctx: CanvasRenderingContext2D, globalScale: number) => void;
    onNodeHover?: (node: NodeObject<NodeType> | null, previousNode: NodeObject<NodeType> | null) => void;
    onNodeClick?: (node: NodeObject<NodeType>) => void;
    onNodeRightClick?: (node: NodeObject<NodeType>) => void;
    onNodeDrag?: (node: NodeObject<NodeType>) => void;
    onNodeDragEnd?: (node: NodeObject<NodeType>) => void;
    onBackgroundClick?: (event: MouseEvent) => void;
    linkSource?: string;
    linkTarget?: string;
    linkColor?: string | ((link: LinkObject<LinkType>) => string);
    linkWidth?: number | string | ((link: LinkObject<LinkType>) => number);
    linkCurvature?: number | string | ((link: LinkObject<LinkType>) => number);
    linkDirectionalArrowLength?: number | string | ((link: LinkObject<LinkType>) => number);
    linkDirectionalArrowColor?: string | ((link: LinkObject<LinkType>) => string);
    linkDirectionalArrowRelPos?: number | string | ((link: LinkObject<LinkType>) => number);
    d3AlphaMin?: number;
    d3AlphaDecay?: number;
    d3VelocityDecay?: number;
    warmupTicks?: number;
    cooldownTicks?: number;
    cooldownTime?: number;
    onEngineStop?: () => void;
    onEngineTick?: () => void;
    enableNodeDrag?: boolean;
    enableZoomInteraction?: boolean;
    enablePanInteraction?: boolean;
    minZoom?: number;
    maxZoom?: number;
    ref?: MutableRefObject<ForceGraphMethods<NodeType, LinkType> | undefined>;
    [key: string]: any;
  }

  export default function ForceGraph<NodeType = any, LinkType = any>(props: ForceGraphProps<NodeType, LinkType>): JSX.Element;
}
