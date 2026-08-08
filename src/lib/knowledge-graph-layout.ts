import { forceCollide, forceLink, forceManyBody, forceRadial, forceSimulation, type SimulationNodeDatum } from "d3-force-3d";
import type { KnowledgeGraphLink, KnowledgeGraphNode, KnowledgeGraphPayload } from "./desktop";

export interface PositionedKnowledgeNode extends KnowledgeGraphNode {
  x: number;
  y: number;
  z: number;
  size: number;
  color: string;
  opacity: number;
  semantic: boolean;
}

export interface PreparedKnowledgeGraph {
  nodes: PositionedKnowledgeNode[];
  links: KnowledgeGraphLink[];
  nodeById: Map<string, PositionedKnowledgeNode>;
}

type HubNode = KnowledgeGraphNode & SimulationNodeDatum & { x: number; y: number; z: number };
type HubLink = { source: string | HubNode; target: string | HubNode; weight: number };

const categoryColors: Record<string, string> = {
  idle: "#7891aa",
  research: "#ffb84d",
  video_input: "#ff6bb5",
  text_input: "#26d0ce",
  game: "#ff5964",
  social: "#9b75ff",
  creation_development: "#5d7cff",
  file_management: "#1fd8a4",
  pending: "#a4b2c4",
};

const kindColors: Record<KnowledgeGraphNode["kind"], string> = {
  category: "#f8f4ff",
  app: "#2fffc2",
  domain: "#47c6ff",
  day: "#ff8b6b",
  activity: "#e5f7ff",
  "browser-visit": "#ff8ccc",
};

export function prepareKnowledgeSpaceLayout(payload: KnowledgeGraphPayload): PreparedKnowledgeGraph {
  const semanticIds = new Set(payload.nodes.filter((node) => isSemantic(node)).map((node) => node.id));
  const hubs: HubNode[] = payload.nodes
    .filter((node) => semanticIds.has(node.id))
    .map((node, index, nodes) => {
      const seed = hash01(node.id);
      const angle = seed * Math.PI * 2;
      const latitude = Math.acos(1 - 2 * ((index + .5) / Math.max(nodes.length, 1)));
      const radius = 62 + kindRing(node.kind);
      return {
        ...node,
        x: Math.sin(latitude) * Math.cos(angle) * radius,
        y: Math.cos(latitude) * radius,
        z: Math.sin(latitude) * Math.sin(angle) * radius,
      };
    });

  if (hubs.length > 1) {
    const hubLinks = buildHubLinks(payload.links, semanticIds);
    forceSimulation(hubs, 3)
      .randomSource(seededRandom(0x5f3759df))
      .force("charge", forceManyBody<HubNode>().strength(-70).distanceMax(180))
      .force("radial", forceRadial<HubNode>((node) => 56 + kindRing(node.kind)).strength(.32))
      .force("collision", forceCollide<HubNode>((node) => Math.max(4, semanticSize(node))).strength(.9))
      .force("links", forceLink<HubNode, HubLink>(hubLinks).id((node: HubNode) => node.id).distance(34).strength(.12))
      .stop()
      .tick(96);
  }

  const hubById = new Map(hubs.map((node) => [node.id, node]));
  const linkedHubs = new Map<string, HubNode[]>();
  for (const link of payload.links) {
    const sourceHub = hubById.get(link.source);
    const targetHub = hubById.get(link.target);
    if (!sourceHub && targetHub) pushLinkedHub(linkedHubs, link.source, targetHub);
    if (!targetHub && sourceHub) pushLinkedHub(linkedHubs, link.target, sourceHub);
  }

  const positioned = payload.nodes.map<PositionedKnowledgeNode>((node) => {
    const hub = hubById.get(node.id);
    const color = node.category && categoryColors[node.category] ? categoryColors[node.category] : kindColors[node.kind];
    if (hub) {
      return {
        ...node,
        x: hub.x ?? 0,
        y: hub.y ?? 0,
        z: hub.z ?? 0,
        size: semanticSize(node),
        color,
        opacity: .95,
        semantic: true,
      };
    }

    const anchors = linkedHubs.get(node.id) ?? [];
    const anchor = averagePosition(anchors);
    const seed = hash01(node.id);
    const phi = Math.acos(1 - 2 * seed);
    const theta = Math.PI * 2 * hash01(`${node.id}:theta`);
    const radius = 7 + 16 * hash01(`${node.id}:radius`);
    return {
      ...node,
      x: anchor.x + Math.sin(phi) * Math.cos(theta) * radius,
      y: anchor.y + Math.cos(phi) * radius,
      z: anchor.z + Math.sin(phi) * Math.sin(theta) * radius,
      size: .48 + Math.min(1.15, Math.log2(Math.max(1, node.durationSeconds) + 1) * .11),
      color,
      opacity: Math.max(.28, Math.min(.92, .28 + node.confidence * .64)),
      semantic: false,
    };
  });

  return {
    nodes: positioned,
    links: payload.links,
    nodeById: new Map(positioned.map((node) => [node.id, node])),
  };
}

function isSemantic(node: KnowledgeGraphNode) {
  return node.kind !== "activity" && node.kind !== "browser-visit";
}

function semanticSize(node: KnowledgeGraphNode) {
  const base = node.kind === "category" ? 4.8 : node.kind === "app" ? 3.7 : 3.1;
  return base + Math.min(4.2, Math.log2(Math.max(1, node.durationSeconds) + 1) * .28);
}

function kindRing(kind: KnowledgeGraphNode["kind"]) {
  if (kind === "category") return -22;
  if (kind === "app") return -7;
  if (kind === "domain") return 11;
  return 25;
}

function buildHubLinks(links: KnowledgeGraphLink[], semanticIds: Set<string>): HubLink[] {
  const rawConnections = new Map<string, string[]>();
  for (const link of links) {
    if (semanticIds.has(link.source) && !semanticIds.has(link.target)) {
      pushUnique(rawConnections, link.target, link.source);
    } else if (semanticIds.has(link.target) && !semanticIds.has(link.source)) {
      pushUnique(rawConnections, link.source, link.target);
    }
  }
  const weighted = new Map<string, HubLink>();
  for (const hubs of rawConnections.values()) {
    for (let left = 0; left < hubs.length; left += 1) {
      for (let right = left + 1; right < hubs.length; right += 1) {
        const [source, target] = [hubs[left], hubs[right]].sort();
        const key = `${source}\n${target}`;
        const existing = weighted.get(key);
        if (existing) existing.weight += 1;
        else weighted.set(key, { source, target, weight: 1 });
      }
    }
  }
  return [...weighted.values()];
}

function pushLinkedHub(map: Map<string, HubNode[]>, rawId: string, hub: HubNode) {
  const list = map.get(rawId) ?? [];
  if (!list.some((item) => item.id === hub.id)) list.push(hub);
  map.set(rawId, list);
}

function pushUnique(map: Map<string, string[]>, key: string, value: string) {
  const values = map.get(key) ?? [];
  if (!values.includes(value)) values.push(value);
  map.set(key, values);
}

function averagePosition(nodes: HubNode[]) {
  if (!nodes.length) return { x: 0, y: 0, z: 0 };
  return nodes.reduce((total, node) => ({
    x: total.x + (node.x ?? 0) / nodes.length,
    y: total.y + (node.y ?? 0) / nodes.length,
    z: total.z + (node.z ?? 0) / nodes.length,
  }), { x: 0, y: 0, z: 0 });
}

function hash01(value: string) {
  let hash = 2166136261;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0) / 4_294_967_295;
}

function seededRandom(initial: number) {
  let state = initial >>> 0;
  return () => {
    state = (Math.imul(state, 1_664_525) + 1_013_904_223) >>> 0;
    return state / 4_294_967_296;
  };
}
