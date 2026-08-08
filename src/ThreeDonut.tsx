import { useEffect, useRef, useState } from "react";
import * as THREE from "three";

export type ThreeDonutItem = {
  name: string;
  percent: number;
  duration: string;
  color: string;
};

type ThreeDonutProps = {
  items: ThreeDonutItem[];
  total: string;
  onHover: (item: ThreeDonutItem, x: number, y: number) => void;
  onLeave: () => void;
  onSelect: (item: ThreeDonutItem, x: number, y: number) => void;
};

function makeParticleTexture() {
  const canvas = document.createElement("canvas");
  canvas.width = 96;
  canvas.height = 96;
  const context = canvas.getContext("2d");
  if (!context) return null;
  const gradient = context.createRadialGradient(48, 48, 0, 48, 48, 48);
  gradient.addColorStop(0, "rgba(255,255,255,1)");
  gradient.addColorStop(.16, "rgba(178,224,255,.92)");
  gradient.addColorStop(.48, "rgba(86,156,255,.26)");
  gradient.addColorStop(1, "rgba(35,91,181,0)");
  context.fillStyle = gradient;
  context.fillRect(0, 0, 96, 96);
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  return texture;
}

function makeHaloTexture() {
  const canvas = document.createElement("canvas");
  canvas.width = 256;
  canvas.height = 256;
  const context = canvas.getContext("2d");
  if (!context) return null;
  const gradient = context.createRadialGradient(128, 128, 18, 128, 128, 128);
  gradient.addColorStop(0, "rgba(115,220,255,.46)");
  gradient.addColorStop(.32, "rgba(73,123,255,.2)");
  gradient.addColorStop(.7, "rgba(63,72,210,.07)");
  gradient.addColorStop(1, "rgba(27,42,110,0)");
  context.fillStyle = gradient;
  context.fillRect(0, 0, 256, 256);
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  return texture;
}

export default function ThreeDonut({ items, total, onHover, onLeave, onSelect }: ThreeDonutProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const callbacksRef = useRef({ onHover, onLeave, onSelect });
  const [fallback, setFallback] = useState(false);
  callbacksRef.current = { onHover, onLeave, onSelect };

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    let renderer: THREE.WebGLRenderer;
    try {
      renderer = new THREE.WebGLRenderer({ alpha: true, antialias: true, powerPreference: "high-performance" });
    } catch {
      setFallback(true);
      return;
    }

    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 1.8));
    renderer.setClearColor(0x000000, 0);
    renderer.outputColorSpace = THREE.SRGBColorSpace;
    renderer.toneMapping = THREE.ACESFilmicToneMapping;
    renderer.toneMappingExposure = 1.12;
    host.appendChild(renderer.domElement);

    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(37, 1, .1, 100);
    camera.position.set(0, .05, 6.25);
    const ringGroup = new THREE.Group();
    ringGroup.rotation.x = .69;
    ringGroup.rotation.z = -.04;
    scene.add(ringGroup);

    scene.add(new THREE.HemisphereLight(0xdff6ff, 0x17234e, 2.5));
    const keyLight = new THREE.DirectionalLight(0xffffff, 4.2);
    keyLight.position.set(-3, 4, 6);
    scene.add(keyLight);
    const rimLight = new THREE.PointLight(0x4ca6ff, 16, 12);
    rimLight.position.set(3.2, -.8, 3.6);
    scene.add(rimLight);
    const cyanLight = new THREE.PointLight(0x45ffe4, 10, 10);
    cyanLight.position.set(-3, 1.2, 2.4);
    scene.add(cyanLight);

    const meshes: THREE.Mesh[] = [];
    let cumulative = 0;
    items.forEach((item) => {
      const arc = Math.max(.055, item.percent / 100 * Math.PI * 2 - .045);
      const geometry = new THREE.TorusGeometry(1.54, .37, 28, Math.max(10, Math.round(arc * 22)), arc);
      const color = new THREE.Color(item.color);
      const material = new THREE.MeshPhysicalMaterial({
        color,
        emissive: color.clone().multiplyScalar(.28),
        emissiveIntensity: .32,
        metalness: .24,
        roughness: .2,
        clearcoat: 1,
        clearcoatRoughness: .12,
        transmission: .08,
        transparent: true,
        opacity: .97,
      });
      const mesh = new THREE.Mesh(geometry, material);
      mesh.rotation.z = -Math.PI / 2 + cumulative / 100 * Math.PI * 2;
      mesh.userData.item = item;
      mesh.userData.baseScale = 1;
      ringGroup.add(mesh);
      meshes.push(mesh);

      const glowMaterial = new THREE.MeshBasicMaterial({
        color,
        transparent: true,
        opacity: .16,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
        side: THREE.BackSide,
      });
      const glow = new THREE.Mesh(geometry.clone(), glowMaterial);
      glow.rotation.copy(mesh.rotation);
      glow.scale.setScalar(1.075);
      glow.renderOrder = -1;
      ringGroup.add(glow);
      cumulative += item.percent;
    });

    const haloTexture = makeHaloTexture();
    const haloMaterial = new THREE.SpriteMaterial({ map: haloTexture, color: 0x78bcff, transparent: true, opacity: .74, blending: THREE.AdditiveBlending, depthWrite: false });
    const halo = new THREE.Sprite(haloMaterial);
    halo.scale.set(5.4, 5.4, 1);
    halo.position.z = -.55;
    scene.add(halo);

    const particleTexture = makeParticleTexture();
    const particleGeometry = new THREE.BufferGeometry();
    const positions = new Float32Array(210 * 3);
    for (let index = 0; index < 210; index += 1) {
      const radius = 1.4 + Math.random() * 1.85;
      const angle = Math.random() * Math.PI * 2;
      positions[index * 3] = Math.cos(angle) * radius;
      positions[index * 3 + 1] = Math.sin(angle) * radius * .78;
      positions[index * 3 + 2] = (Math.random() - .5) * 1.65;
    }
    particleGeometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
    const particleMaterial = new THREE.PointsMaterial({ map: particleTexture, size: .13, color: 0xa8ddff, transparent: true, opacity: .52, blending: THREE.AdditiveBlending, depthWrite: false, sizeAttenuation: true });
    const particles = new THREE.Points(particleGeometry, particleMaterial);
    scene.add(particles);

    const gasGeometry = new THREE.BufferGeometry();
    const gasPositions = new Float32Array(48 * 3);
    for (let index = 0; index < 48; index += 1) {
      const radius = .7 + Math.random() * 2.3;
      const angle = Math.random() * Math.PI * 2;
      gasPositions[index * 3] = Math.cos(angle) * radius;
      gasPositions[index * 3 + 1] = Math.sin(angle) * radius * .64;
      gasPositions[index * 3 + 2] = -1 + Math.random() * 1.4;
    }
    gasGeometry.setAttribute("position", new THREE.BufferAttribute(gasPositions, 3));
    const gasMaterial = new THREE.PointsMaterial({ map: haloTexture, size: .72, color: 0x657cf7, transparent: true, opacity: .075, blending: THREE.AdditiveBlending, depthWrite: false });
    const gas = new THREE.Points(gasGeometry, gasMaterial);
    scene.add(gas);

    const raycaster = new THREE.Raycaster();
    const pointer = new THREE.Vector2(10, 10);
    const pointerTarget = new THREE.Vector2(0, 0);
    let hovered: THREE.Mesh | null = null;

    const updatePointer = (event: PointerEvent) => {
      const rect = renderer.domElement.getBoundingClientRect();
      pointer.x = (event.clientX - rect.left) / rect.width * 2 - 1;
      pointer.y = -(event.clientY - rect.top) / rect.height * 2 + 1;
      pointerTarget.set(pointer.x, pointer.y);
      raycaster.setFromCamera(pointer, camera);
      const hit = raycaster.intersectObjects(meshes, false)[0]?.object as THREE.Mesh | undefined;
      if (hovered !== hit) {
        if (hovered) {
          (hovered.material as THREE.MeshPhysicalMaterial).emissiveIntensity = .32;
          hovered.userData.targetScale = 1;
        }
        hovered = hit ?? null;
        if (hovered) {
          (hovered.material as THREE.MeshPhysicalMaterial).emissiveIntensity = 1.05;
          hovered.userData.targetScale = 1.09;
        } else {
          callbacksRef.current.onLeave();
        }
      }
      if (hovered) callbacksRef.current.onHover(hovered.userData.item as ThreeDonutItem, event.clientX, event.clientY);
    };
    const leave = () => {
      pointerTarget.set(0, 0);
      if (hovered) {
        (hovered.material as THREE.MeshPhysicalMaterial).emissiveIntensity = .32;
        hovered.userData.targetScale = 1;
      }
      hovered = null;
      callbacksRef.current.onLeave();
    };
    const select = (event: PointerEvent) => {
      if (hovered) callbacksRef.current.onSelect(hovered.userData.item as ThreeDonutItem, event.clientX, event.clientY);
    };
    renderer.domElement.addEventListener("pointermove", updatePointer);
    renderer.domElement.addEventListener("pointerleave", leave);
    renderer.domElement.addEventListener("pointerdown", select);

    const resize = () => {
      const width = Math.max(1, host.clientWidth);
      const height = Math.max(1, host.clientHeight);
      renderer.setSize(width, height, false);
      camera.aspect = width / height;
      camera.updateProjectionMatrix();
    };
    const observer = new ResizeObserver(resize);
    observer.observe(host);
    resize();

    const clock = new THREE.Clock();
    let frame = 0;
    const render = () => {
      const elapsed = clock.getElapsedTime();
      ringGroup.rotation.z += (pointerTarget.x * .12 - ringGroup.rotation.z) * .035;
      ringGroup.rotation.x += (.69 - pointerTarget.y * .08 - ringGroup.rotation.x) * .035;
      particles.rotation.z = elapsed * .018;
      particles.rotation.x = Math.sin(elapsed * .19) * .08;
      gas.rotation.z = -elapsed * .012;
      haloMaterial.opacity = .61 + Math.sin(elapsed * .9) * .11;
      meshes.forEach((mesh) => {
        const target = mesh.userData.targetScale ?? 1;
        const scale = THREE.MathUtils.lerp(mesh.scale.x, target, .12);
        mesh.scale.setScalar(scale);
      });
      renderer.render(scene, camera);
      frame = requestAnimationFrame(render);
    };
    render();

    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      renderer.domElement.removeEventListener("pointermove", updatePointer);
      renderer.domElement.removeEventListener("pointerleave", leave);
      renderer.domElement.removeEventListener("pointerdown", select);
      scene.traverse((object) => {
        if (object instanceof THREE.Mesh || object instanceof THREE.Points) object.geometry.dispose();
        if (object instanceof THREE.Mesh || object instanceof THREE.Points || object instanceof THREE.Sprite) {
          const material = object.material;
          (Array.isArray(material) ? material : [material]).forEach((entry) => entry.dispose());
        }
      });
      particleTexture?.dispose();
      haloTexture?.dispose();
      renderer.dispose();
      renderer.domElement.remove();
    };
  }, [items]);

  if (fallback) return <div className="three-donut-fallback" aria-label="WebGL 不可用，显示静态圆环"><span>总时长</span><strong>{total}</strong></div>;
  return <div className="three-donut-shell">
    <div ref={hostRef} className="three-donut-canvas" role="img" aria-label={`三维数据圆环，总时长 ${total}`} />
    <div className="three-donut-label"><span>总时长</span><strong>{total}</strong><small>拖动光标探索</small></div>
  </div>;
}
