import { useEffect, useRef } from "react";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { EffectComposer } from "three/examples/jsm/postprocessing/EffectComposer.js";
import { RenderPass } from "three/examples/jsm/postprocessing/RenderPass.js";
import { UnrealBloomPass } from "three/examples/jsm/postprocessing/UnrealBloomPass.js";
import { prepareKnowledgeSpaceLayout, type PositionedKnowledgeNode } from "./lib/knowledge-graph-layout";
import type { KnowledgeGraphPayload } from "./lib/desktop";

interface SceneProps {
  payload: KnowledgeGraphPayload;
  selectedId: string | null;
  bloomMode: "strong" | "readable";
  onHover: (node: PositionedKnowledgeNode | null, x: number, y: number) => void;
  onSelect: (node: PositionedKnowledgeNode | null) => void;
  onQualityChange?: (reduced: boolean) => void;
}

interface SceneApi {
  focus: (id: string | null) => void;
}

const vertexShader = `
  attribute float aSize;
  attribute float aAlpha;
  varying vec3 vColor;
  varying float vAlpha;
  void main() {
    vColor = color;
    vAlpha = aAlpha;
    vec4 mvPosition = modelViewMatrix * vec4(position, 1.0);
    gl_PointSize = clamp(aSize * (330.0 / max(24.0, -mvPosition.z)), 1.4, 44.0);
    gl_Position = projectionMatrix * mvPosition;
  }
`;

const fragmentShader = `
  varying vec3 vColor;
  varying float vAlpha;
  void main() {
    float distanceToCenter = distance(gl_PointCoord, vec2(0.5));
    if (distanceToCenter > 0.5) discard;
    float core = 1.0 - smoothstep(0.0, 0.16, distanceToCenter);
    float halo = 1.0 - smoothstep(0.08, 0.5, distanceToCenter);
    float alpha = (core * 0.86 + halo * 0.3) * vAlpha;
    gl_FragColor = vec4(vColor * (1.02 + core * 1.28), alpha);
  }
`;

export default function KnowledgeGraphScene({ payload, selectedId, bloomMode, onHover, onSelect, onQualityChange }: SceneProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const apiRef = useRef<SceneApi | null>(null);
  const callbacksRef = useRef({ onHover, onSelect, onQualityChange });
  callbacksRef.current = { onHover, onSelect, onQualityChange };

  useEffect(() => {
    apiRef.current?.focus(selectedId);
  }, [selectedId]);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const layout = prepareKnowledgeSpaceLayout(payload);
    const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const scene = new THREE.Scene();
    scene.background = new THREE.Color("#040711");
    scene.fog = new THREE.FogExp2("#040711", .0028);

    const camera = new THREE.PerspectiveCamera(44, 1, .1, 1_400);
    camera.position.set(0, 8, 250);
    const renderer = new THREE.WebGLRenderer({ antialias: true, powerPreference: "high-performance" });
    renderer.outputColorSpace = THREE.SRGBColorSpace;
    renderer.toneMapping = THREE.ACESFilmicToneMapping;
    renderer.toneMappingExposure = bloomMode === "strong" ? .96 : .84;
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 1.6));
    host.replaceChildren(renderer.domElement);

    const composer = new EffectComposer(renderer);
    composer.addPass(new RenderPass(scene, camera));
    const bloom = new UnrealBloomPass(new THREE.Vector2(1, 1), bloomMode === "strong" ? 1.38 : .58, bloomMode === "strong" ? .58 : .36, .16);
    composer.addPass(bloom);

    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = .065;
    controls.minDistance = 45;
    controls.maxDistance = 420;
    controls.autoRotate = !reducedMotion;
    controls.autoRotateSpeed = .18;

    const graphGroup = new THREE.Group();
    graphGroup.rotation.x = -.08;
    scene.add(graphGroup);

    const positions = new Float32Array(layout.nodes.length * 3);
    const colors = new Float32Array(layout.nodes.length * 3);
    const sizes = new Float32Array(layout.nodes.length);
    const alphas = new Float32Array(layout.nodes.length);
    layout.nodes.forEach((node, index) => {
      positions.set([node.x, node.y, node.z], index * 3);
      const color = new THREE.Color(node.color);
      colors.set([color.r, color.g, color.b], index * 3);
      sizes[index] = node.size * (node.semantic ? 5.4 : 2.55);
      alphas[index] = node.semantic ? Math.min(.92, node.opacity) : Math.min(.68, node.opacity);
    });
    const pointsGeometry = new THREE.BufferGeometry();
    pointsGeometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
    pointsGeometry.setAttribute("color", new THREE.BufferAttribute(colors, 3));
    pointsGeometry.setAttribute("aSize", new THREE.BufferAttribute(sizes, 1));
    pointsGeometry.setAttribute("aAlpha", new THREE.BufferAttribute(alphas, 1));
    const pointsMaterial = new THREE.ShaderMaterial({
      vertexShader,
      fragmentShader,
      vertexColors: true,
      transparent: true,
      depthWrite: false,
      blending: THREE.AdditiveBlending,
    });
    const points = new THREE.Points(pointsGeometry, pointsMaterial);
    graphGroup.add(points);

    const linePositions: number[] = [];
    const lineColors: number[] = [];
    for (const link of layout.links) {
      const source = layout.nodeById.get(link.source);
      const target = layout.nodeById.get(link.target);
      if (!source || !target) continue;
      linePositions.push(source.x, source.y, source.z, target.x, target.y, target.z);
      const sourceColor = new THREE.Color(source.color).multiplyScalar(.74);
      const targetColor = new THREE.Color(target.color).multiplyScalar(.74);
      lineColors.push(sourceColor.r, sourceColor.g, sourceColor.b, targetColor.r, targetColor.g, targetColor.b);
    }
    const lineGeometry = new THREE.BufferGeometry();
    lineGeometry.setAttribute("position", new THREE.Float32BufferAttribute(linePositions, 3));
    lineGeometry.setAttribute("color", new THREE.Float32BufferAttribute(lineColors, 3));
    const lineMaterial = new THREE.LineBasicMaterial({ vertexColors: true, transparent: true, opacity: bloomMode === "strong" ? .115 : .065, depthWrite: false, blending: THREE.AdditiveBlending });
    const lines = new THREE.LineSegments(lineGeometry, lineMaterial);
    graphGroup.add(lines);

    const dustGeometry = new THREE.BufferGeometry();
    const dustPositions = new Float32Array(1_400 * 3);
    const random = seededRandom(0x19b4c2a7);
    for (let index = 0; index < 1_400; index += 1) {
      const radius = 35 + random() * 125;
      const phi = Math.acos(1 - 2 * random());
      const theta = Math.PI * 2 * random();
      dustPositions.set([Math.sin(phi) * Math.cos(theta) * radius, Math.cos(phi) * radius, Math.sin(phi) * Math.sin(theta) * radius], index * 3);
    }
    dustGeometry.setAttribute("position", new THREE.BufferAttribute(dustPositions, 3));
    const dust = new THREE.Points(dustGeometry, new THREE.PointsMaterial({ color: "#67d8ff", size: .27, transparent: true, opacity: .2, depthWrite: false, blending: THREE.AdditiveBlending }));
    graphGroup.add(dust);

    const highlightGeometry = new THREE.BufferGeometry();
    const highlightMaterial = new THREE.LineBasicMaterial({ color: "#8ffff0", transparent: true, opacity: .92, blending: THREE.AdditiveBlending, depthWrite: false });
    const highlights = new THREE.LineSegments(highlightGeometry, highlightMaterial);
    graphGroup.add(highlights);

    const relatedById = new Map<string, Set<string>>();
    for (const link of layout.links) {
      addRelated(relatedById, link.source, link.target);
      addRelated(relatedById, link.target, link.source);
    }
    const focus = (id: string | null) => {
      const alphaAttribute = pointsGeometry.getAttribute("aAlpha") as THREE.BufferAttribute;
      const related = id ? new Set([id, ...(relatedById.get(id) ?? [])]) : null;
      layout.nodes.forEach((node, index) => {
        alphaAttribute.setX(index, related ? (related.has(node.id) ? Math.max(.9, node.opacity) : .08) : node.opacity);
      });
      alphaAttribute.needsUpdate = true;
      lineMaterial.opacity = id ? .025 : bloomMode === "strong" ? .115 : .065;
      const selectedLines: number[] = [];
      if (id) {
        for (const link of layout.links) {
          if (link.source !== id && link.target !== id) continue;
          const source = layout.nodeById.get(link.source);
          const target = layout.nodeById.get(link.target);
          if (source && target) selectedLines.push(source.x, source.y, source.z, target.x, target.y, target.z);
        }
      }
      highlightGeometry.setAttribute("position", new THREE.Float32BufferAttribute(selectedLines, 3));
      highlightGeometry.computeBoundingSphere();
      const selected = id ? layout.nodeById.get(id) : null;
      if (selected) controls.target.set(selected.x, selected.y, selected.z);
      else controls.target.set(0, 0, 0);
    };
    apiRef.current = { focus };
    focus(selectedId);

    const raycaster = new THREE.Raycaster();
    raycaster.params.Points = { threshold: 3.2 };
    const pointer = new THREE.Vector2();
    let hoveredIndex = -1;
    const updatePointer = (event: PointerEvent) => {
      const rect = renderer.domElement.getBoundingClientRect();
      pointer.set(((event.clientX - rect.left) / rect.width) * 2 - 1, -((event.clientY - rect.top) / rect.height) * 2 + 1);
      raycaster.setFromCamera(pointer, camera);
      const hit = raycaster.intersectObject(points, false)[0];
      const nextIndex = typeof hit?.index === "number" ? hit.index : -1;
      if (nextIndex === hoveredIndex) return;
      hoveredIndex = nextIndex;
      renderer.domElement.style.cursor = nextIndex >= 0 ? "pointer" : "grab";
      callbacksRef.current.onHover(nextIndex >= 0 ? layout.nodes[nextIndex] : null, event.clientX, event.clientY);
    };
    const selectPointer = () => callbacksRef.current.onSelect(hoveredIndex >= 0 ? layout.nodes[hoveredIndex] : null);
    renderer.domElement.addEventListener("pointermove", updatePointer);
    renderer.domElement.addEventListener("click", selectPointer);

    const resize = () => {
      const width = Math.max(1, host.clientWidth);
      const height = Math.max(1, host.clientHeight);
      camera.aspect = width / height;
      camera.updateProjectionMatrix();
      renderer.setSize(width, height, false);
      composer.setSize(width, height);
    };
    const observer = new ResizeObserver(resize);
    observer.observe(host);
    resize();

    let frame = 0;
    let frameWindowStart = performance.now();
    let reducedQuality = false;
    renderer.setAnimationLoop(() => {
      controls.update();
      if (!reducedMotion) {
        dust.rotation.y += .00022;
        graphGroup.rotation.y += .000045;
      }
      composer.render();
      frame += 1;
      if (!reducedQuality && frame >= 150) {
        const fps = frame * 1_000 / Math.max(1, performance.now() - frameWindowStart);
        if (fps < 28) {
          reducedQuality = true;
          renderer.setPixelRatio(1);
          bloom.strength = Math.min(bloom.strength, .82);
          controls.autoRotate = false;
          callbacksRef.current.onQualityChange?.(true);
          resize();
        }
        frame = 0;
        frameWindowStart = performance.now();
      }
    });

    return () => {
      apiRef.current = null;
      observer.disconnect();
      renderer.domElement.removeEventListener("pointermove", updatePointer);
      renderer.domElement.removeEventListener("click", selectPointer);
      renderer.setAnimationLoop(null);
      controls.dispose();
      pointsGeometry.dispose();
      pointsMaterial.dispose();
      lineGeometry.dispose();
      lineMaterial.dispose();
      highlightGeometry.dispose();
      highlightMaterial.dispose();
      dustGeometry.dispose();
      (dust.material as THREE.Material).dispose();
      composer.dispose();
      renderer.dispose();
      host.replaceChildren();
    };
  }, [payload, bloomMode]);

  return <div ref={hostRef} className="knowledge-graph-canvas" aria-label="三维知识图谱，可拖动旋转并滚轮缩放" />;
}

function addRelated(map: Map<string, Set<string>>, source: string, target: string) {
  const values = map.get(source) ?? new Set<string>();
  values.add(target);
  map.set(source, values);
}

function seededRandom(initial: number) {
  let state = initial >>> 0;
  return () => {
    state = (Math.imul(state, 1_664_525) + 1_013_904_223) >>> 0;
    return state / 4_294_967_296;
  };
}
