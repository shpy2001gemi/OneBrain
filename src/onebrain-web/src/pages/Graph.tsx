import { useState, useRef, useEffect, useCallback } from 'react';
import { Network, Search, Info, X } from 'lucide-react';
import { api } from '../api/client';
import type { NeighborInfo } from '../api/types';
import { GENE_TYPE_COLORS } from '../api/types';

interface GraphNode {
  id: string;
  label: string;
  geneType: string;
  pomv: number;
  preview: string;
  x: number;
  y: number;
  vx: number;
  vy: number;
  isCenter: boolean;
}

interface GraphLink {
  source: string;
  target: string;
  relation: string;
  weight: number;
}

const GENE_COLORS: Record<string, string> = GENE_TYPE_COLORS;

export function GraphPage() {
  const [cidInput, setCidInput] = useState('');
  const [depth, setDepth] = useState(2);
  const [nodes, setNodes] = useState<GraphNode[]>([]);
  const [links, setLinks] = useState<GraphLink[]>([]);
  const [selected, setSelected] = useState<GraphNode | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const svgRef = useRef<SVGSVGElement>(null);
  const animRef = useRef<number>(0);
  const nodesRef = useRef<GraphNode[]>([]);
  const dragging = useRef<string | null>(null);
  const dragOffset = useRef({ x: 0, y: 0 });
  const linksRef = useRef<GraphLink[]>([]);
  const [simVersion, setSimVersion] = useState(0);

  const loadGraph = async (cid?: string) => {
    const targetCid = cid || cidInput.trim();
    if (!targetCid) return;
    setLoading(true);
    setError('');
    try {
      const data = await api.getGraph(targetCid, depth);
      const neighborList: NeighborInfo[] = data.neighbors || [];
      
      const nodeMap = new Map<string, GraphNode>();
      const linkList: GraphLink[] = [];
      const cx = 400, cy = 300;
      
      // Center node
      nodeMap.set(targetCid, {
        id: targetCid, label: targetCid.slice(0, 8), geneType: 'Fact',
        pomv: 1.0, preview: 'Center node', x: cx, y: cy, vx: 0, vy: 0, isCenter: true,
      });
      
      // Add neighbors
      neighborList.forEach((n, i) => {
        const angle = (2 * Math.PI * i) / neighborList.length;
        const r = 120 + Math.random() * 60;
        if (!nodeMap.has(n.cid_hex)) {
          nodeMap.set(n.cid_hex, {
            id: n.cid_hex, label: n.cid_hex.slice(0, 8), geneType: n.gene_type,
            pomv: n.pomv, preview: n.preview, x: cx + r * Math.cos(angle), y: cy + r * Math.sin(angle),
            vx: 0, vy: 0, isCenter: false,
          });
        }
        linkList.push({ source: targetCid, target: n.cid_hex, relation: n.relation, weight: n.weight });
        
        // Add children (depth 2+)
        n.children?.forEach((c, j) => {
          const childAngle = angle + (j - (n.children?.length ?? 0) / 2) * 0.3;
          const cr = r + 100 + Math.random() * 40;
          if (!nodeMap.has(c.cid_hex)) {
            nodeMap.set(c.cid_hex, {
              id: c.cid_hex, label: c.cid_hex.slice(0, 8), geneType: c.gene_type,
              pomv: c.pomv, preview: c.preview, x: cx + cr * Math.cos(childAngle), y: cy + cr * Math.sin(childAngle),
              vx: 0, vy: 0, isCenter: false,
            });
          }
          linkList.push({ source: n.cid_hex, target: c.cid_hex, relation: c.relation, weight: c.weight });
        });
      });
      
      const nodeArr = Array.from(nodeMap.values());
      setNodes(nodeArr);
      setLinks(linkList);
      nodesRef.current = nodeArr;
      linksRef.current = linkList;
      setSimVersion(v => v + 1);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to load graph');
    } finally {
      setLoading(false);
    }
  };

  // Force simulation — triggered by simVersion (set after graph data loads)
  useEffect(() => {
    if (nodesRef.current.length === 0) return;
    let running = true;
    let frameCount = 0;
    
    const tick = () => {
      if (!running) return;
      const ns = nodesRef.current;
      const k = 0.01; // spring
      const rep = 5000; // repulsion
      const damp = 0.9;
      const cx = 400, cy = 300;

      // Build lookup map for O(1) access
      const nodeMap = new Map<string, GraphNode>();
      ns.forEach(n => nodeMap.set(n.id, n));
      
      // Reset forces
      ns.forEach(n => { n.vx *= damp; n.vy *= damp; });
      
      // Repulsion
      for (let i = 0; i < ns.length; i++) {
        for (let j = i + 1; j < ns.length; j++) {
          const dx = ns[j].x - ns[i].x;
          const dy = ns[j].y - ns[i].y;
          const d = Math.max(1, Math.sqrt(dx * dx + dy * dy));
          const f = rep / (d * d);
          ns[i].vx -= f * dx / d;
          ns[i].vy -= f * dy / d;
          ns[j].vx += f * dx / d;
          ns[j].vy += f * dy / d;
        }
      }
      
      // Attraction (links) — O(1) lookup via Map
      linksRef.current.forEach(l => {
        const src = nodeMap.get(l.source);
        const tgt = nodeMap.get(l.target);
        if (!src || !tgt) return;
        const dx = tgt.x - src.x;
        const dy = tgt.y - src.y;
        const d = Math.max(1, Math.sqrt(dx * dx + dy * dy));
        const f = k * (d - 150);
        src.vx += f * dx / d;
        src.vy += f * dy / d;
        tgt.vx -= f * dx / d;
        tgt.vy -= f * dy / d;
      });
      
      // Center gravity
      ns.forEach(n => {
        if (dragging.current === n.id) return;
        n.vx += (cx - n.x) * 0.001;
        n.vy += (cy - n.y) * 0.001;
        n.x += n.vx;
        n.y += n.vy;
      });
      
      // Throttle React re-renders to every 3rd frame
      frameCount++;
      if (frameCount % 3 === 0) {
        setNodes([...ns]);
      }
      animRef.current = requestAnimationFrame(tick);
    };
    
    animRef.current = requestAnimationFrame(tick);
    return () => { running = false; cancelAnimationFrame(animRef.current); };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [simVersion]);

  const handleMouseDown = (id: string, e: React.MouseEvent) => {
    e.preventDefault();
    dragging.current = id;
    const node = nodesRef.current.find(n => n.id === id);
    if (node) {
      dragOffset.current = { x: e.clientX - node.x, y: e.clientY - node.y };
    }
  };

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!dragging.current) return;
    const idx = nodesRef.current.findIndex(n => n.id === dragging.current);
    if (idx === -1) return;
    nodesRef.current[idx].x = e.clientX - dragOffset.current.x;
    nodesRef.current[idx].y = e.clientY - dragOffset.current.y;
  }, []);

  const handleMouseUp = useCallback(() => {
    dragging.current = null;
  }, []);

  return (
    <div className="page">
      <div className="page-header">
        <h1>Knowledge Graph</h1>
        <p>Visualize relationships between knowledge units</p>
      </div>

      {/* Controls */}
      <div style={{ display: 'flex', gap: 'var(--ob-gap-md)', marginBottom: 'var(--ob-gap-md)', alignItems: 'center' }}>
        <div style={{ flex: 1, position: 'relative' }}>
          <Network size={16} style={{ position: 'absolute', left: 12, top: '50%', transform: 'translateY(-50%)', color: 'var(--ob-text-muted)' }} />
          <input className="input" placeholder="Enter CID to explore..." value={cidInput}
            onChange={e => setCidInput(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && loadGraph()}
            style={{ paddingLeft: 36 }} />
        </div>
        <select className="input" style={{ width: 120 }} value={depth} onChange={e => setDepth(Number(e.target.value))}>
          <option value={1}>Depth 1</option>
          <option value={2}>Depth 2</option>
          <option value={3}>Depth 3</option>
        </select>
        <button className="btn btn-primary" onClick={() => loadGraph()} disabled={loading}>
          {loading ? <span className="spinner" /> : <><Search size={16} /> Explore</>}
        </button>
      </div>

      {error && <div className="glass-card" style={{ borderColor: 'rgba(239,68,68,0.3)', color: 'var(--ob-error)', marginBottom: 'var(--ob-gap-md)' }}>{error}</div>}

      {/* Graph + Detail */}
      <div style={{ display: 'flex', gap: 'var(--ob-gap-md)' }}>
        <div className="glass-card animate-in" style={{ flex: 1, padding: 0, overflow: 'hidden', minHeight: 500 }}>
          {nodes.length === 0 ? (
            <div className="empty-state" style={{ height: 500 }}>
              <Network size={48} />
              <p>Enter a KU CID to visualize its knowledge graph</p>
            </div>
          ) : (
            <svg ref={svgRef} width="100%" height="600" viewBox="0 0 800 600"
              onMouseMove={handleMouseMove} onMouseUp={handleMouseUp} onMouseLeave={handleMouseUp}
              style={{ cursor: dragging.current ? 'grabbing' : 'default' }}>
              <defs>
                <radialGradient id="glow">
                  <stop offset="0%" stopColor="var(--ob-accent)" stopOpacity="0.3" />
                  <stop offset="100%" stopColor="transparent" stopOpacity="0" />
                </radialGradient>
              </defs>
              {/* Links */}
              {links.map((l, i) => {
                const src = nodes.find(n => n.id === l.source);
                const tgt = nodes.find(n => n.id === l.target);
                if (!src || !tgt) return null;
                return <g key={i}>
                  <line x1={src.x} y1={src.y} x2={tgt.x} y2={tgt.y}
                    stroke="rgba(255,255,255,0.1)" strokeWidth={Math.max(1, l.weight * 3)} />
                  <text x={(src.x + tgt.x) / 2} y={(src.y + tgt.y) / 2 - 4}
                    fill="var(--ob-text-muted)" fontSize="9" textAnchor="middle">{l.relation}</text>
                </g>;
              })}
              {/* Nodes */}
              {nodes.map(n => {
                const color = GENE_COLORS[n.geneType] || '#64748b';
                const r = 12 + n.pomv * 16;
                return <g key={n.id} style={{ cursor: 'grab' }}
                  onMouseDown={e => handleMouseDown(n.id, e)}
                  onClick={() => { setSelected(n); setCidInput(n.id); }}>
                  {n.isCenter && <circle cx={n.x} cy={n.y} r={r + 15} fill="url(#glow)" />}
                  <circle cx={n.x} cy={n.y} r={r} fill={color} fillOpacity={0.8}
                    stroke={selected?.id === n.id ? '#fff' : color} strokeWidth={selected?.id === n.id ? 2 : 1} />
                  <text x={n.x} y={n.y + r + 14} fill="var(--ob-text-secondary)"
                    fontSize="10" textAnchor="middle" fontFamily="var(--ob-font-mono)">{n.label}</text>
                  <text x={n.x} y={n.y + 4} fill="#fff" fontSize="8" textAnchor="middle" fontWeight="600">
                    {n.geneType.slice(0, 4)}
                  </text>
                </g>;
              })}
            </svg>
          )}
        </div>

        {/* Detail Panel */}
        {selected && (
          <div className="glass-card animate-in" style={{ width: 280, flexShrink: 0 }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 'var(--ob-gap-md)' }}>
              <h3 style={{ fontSize: '0.95rem', fontWeight: 600, display: 'flex', alignItems: 'center', gap: 6 }}>
                <Info size={16} /> Node Detail
              </h3>
              <button className="btn btn-icon" onClick={() => setSelected(null)}><X size={16} /></button>
            </div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--ob-gap-sm)' }}>
              <div><span className="stat-label">CID</span><p className="mono" style={{ fontSize: '0.72rem', wordBreak: 'break-all' }}>{selected.id}</p></div>
              <div><span className="stat-label">Gene Type</span><span className="badge badge-cyan">{selected.geneType}</span></div>
              <div><span className="stat-label">PoMV</span><p>{(selected.pomv * 100).toFixed(1)}%</p></div>
              <div><span className="stat-label">Preview</span><p style={{ fontSize: '0.82rem', color: 'var(--ob-text-secondary)' }}>{selected.preview}</p></div>
              <button className="btn btn-primary btn-sm" onClick={() => loadGraph(selected.id)}>Explore from here</button>
            </div>
          </div>
        )}
      </div>

      {/* Legend */}
      {nodes.length > 0 && (
        <div style={{ display: 'flex', gap: 'var(--ob-gap-lg)', marginTop: 'var(--ob-gap-md)', justifyContent: 'center' }}>
          {Object.entries(GENE_COLORS).map(([type, color]) => (
            <div key={type} style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: '0.78rem', color: 'var(--ob-text-secondary)' }}>
              <div style={{ width: 10, height: 10, borderRadius: '50%', background: color }} />
              {type}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
