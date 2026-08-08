declare module "d3-force-3d" {
  export interface SimulationNodeDatum {
    index?: number;
    x?: number;
    y?: number;
    z?: number;
    vx?: number;
    vy?: number;
    vz?: number;
  }

  export interface Simulation<N extends SimulationNodeDatum> {
    force(name: string, force: unknown): Simulation<N>;
    stop(): Simulation<N>;
    tick(iterations?: number): Simulation<N>;
    randomSource(source: () => number): Simulation<N>;
  }

  export function forceSimulation<N extends SimulationNodeDatum>(nodes?: N[], numDimensions?: number): Simulation<N>;
  export function forceManyBody<N extends SimulationNodeDatum>(): any;
  export function forceLink<N extends SimulationNodeDatum, L extends { source: string | N; target: string | N }>(links?: L[]): any;
  export function forceRadial<N extends SimulationNodeDatum>(radius: number | ((node: N) => number), x?: number, y?: number, z?: number): any;
  export function forceCollide<N extends SimulationNodeDatum>(radius: number | ((node: N) => number)): any;
}
