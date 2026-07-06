import type { Edge } from '@xyflow/react';
import type { TraceFlowNode } from '../../components/TraceAnalysis/TraceNode';
import type { ToolFlowNode } from '../../components/TraceAnalysis/ToolNode';
import type { TaskFlowNode } from '../../components/TraceAnalysis/TaskNode';
import type { TraceEvent, ContentBlock, ToolNodeData, TaskNodeData, ToolCall, NodeEventType, SessionData } from '../../types/trace';

export type GraphNode = TraceFlowNode | ToolFlowNode | TaskFlowNode;

const NODE_WIDTH = 260;
const NODE_HEIGHT = 100;

function extractPreview(content: string | ContentBlock[] | undefined): string {
  if (!content) return '';
  if (typeof content === 'string') return content.slice(0, 200);

  const texts = content
    .filter(b => b.type === 'text')
    .map(b => b.text || '')
    .join(' ')
    .trim();

  if (texts) return texts.slice(0, 200);

  const results = content.filter(b => b.type === 'tool_result');
  if (results.length > 0) {
    const first = results[0];
    const c = first.content;
    if (typeof c === 'string') return c.slice(0, 200);
  }

  const thinking = content.filter(b => b.type === 'thinking');
  if (thinking.length > 0) {
    return thinking.map(b => b.thinking || '').join(' ').trim().slice(0, 200);
  }

  const toolUses = content.filter(b => b.type === 'tool_use');
  if (toolUses.length > 0) {
    return toolUses.map(b => b.name || 'tool').join(', ');
  }

  return '';
}

function extractTools(content: string | ContentBlock[] | undefined): string[] {
  if (!content || typeof content === 'string') return [];
  return content.filter(b => b.type === 'tool_use').map(b => b.name || 'unknown');
}

function classifyEvent(event: TraceEvent): NodeEventType {
  if (event.type === 'user') return 'user';
  if (event.type === 'assistant') return 'assistant';
  if (event.type === 'summary') return 'summary';

  if (event.type === 'progress') {
    const dataType = event.data?.type;
    if (dataType === 'hook_progress') return 'hook-progress';
    const inner = event.data?.message;
    if (inner?.type === 'user') return 'subagent-user';
    if (inner?.type === 'assistant') return 'subagent-assistant';
  }
  return 'user';
}

const EDGE_STYLE = { stroke: 'hsl(var(--border-300))', strokeWidth: 1.5 };
const EDGE_MARKER = { type: 'arrowclosed' as const, color: 'hsl(var(--border-300))' };

export function buildGraph(sessionData: SessionData): { nodes: TraceFlowNode[]; edges: Edge[] } {
  const { events } = sessionData;
  const nodes: TraceFlowNode[] = [];
  const edges: Edge[] = [];
  const seenIds = new Set<string>();

  function addEvent(event: TraceEvent) {
    if (!event.uuid) return;
    if (seenIds.has(event.uuid)) return;
    seenIds.add(event.uuid);

    const eventType = classifyEvent(event);

    if (event.type === 'file-history-snapshot' || event.type === 'system' || event.type === 'queue-operation') return;
    if (event.type === 'progress' && eventType === 'user') return;

    const subagentId = event.type === 'progress' && event.data?.agentId
      ? event.data.agentId
      : undefined;

    let preview = '';
    let tools: string[] = [];

    if (event.type === 'user' || event.type === 'assistant') {
      preview = extractPreview(event.message?.content);
      tools = extractTools(event.message?.content);
    } else if (event.type === 'progress' && event.data?.message) {
      const inner = event.data.message;
      preview = extractPreview(inner.message?.content);
      tools = extractTools(inner.message?.content);
    } else if (event.type === 'progress' && event.data?.type === 'hook_progress') {
      const hookName = event.data.hookName ?? '';
      const hookEvent = event.data.hookEvent ?? '';
      preview = [hookName, hookEvent].filter(Boolean).join(': ');
    } else if (event.type === 'summary') {
      const msg = event.message;
      if (msg) {
        preview = extractPreview(msg.content);
      }
    }

    nodes.push({
      id: event.uuid,
      type: 'traceNode' as const,
      data: {
        eventType,
        preview,
        tools,
        agentId: event.agentId,
        timestamp: event.timestamp,
        event,
        subagentId,
      },
      position: { x: 0, y: 0 },
    });

    if (event.parentUuid) {
      edges.push({
        id: `e-${event.parentUuid}-${event.uuid}`,
        source: event.parentUuid,
        target: event.uuid,
        type: 'smoothstep',
        style: EDGE_STYLE,
        markerEnd: EDGE_MARKER,
      });
    }
  }

  for (const event of events) {
    addEvent(event);
  }

  return { nodes, edges };
}

function getNodeContentBlocks(node: TraceFlowNode): ContentBlock[] {
  const event = node.data.event;
  let content: string | ContentBlock[] | undefined;
  if (event.type === 'user' || event.type === 'assistant') {
    content = event.message?.content;
  } else if (event.type === 'progress' && event.data?.message) {
    content = event.data.message.message?.content;
  }
  if (!content || typeof content === 'string') return [];
  return content;
}

function nodeHasToolUse(node: TraceFlowNode): boolean {
  return getNodeContentBlocks(node).some(b => b.type === 'tool_use');
}

function nodeHasToolResult(node: TraceFlowNode): boolean {
  return getNodeContentBlocks(node).some(b => b.type === 'tool_result');
}

function collectToolCalls(assistantNode: TraceFlowNode, userNode: TraceFlowNode): ToolCall[] {
  const assistantBlocks = getNodeContentBlocks(assistantNode);
  const userBlocks = getNodeContentBlocks(userNode);
  const toolUses = assistantBlocks.filter(b => b.type === 'tool_use');
  const toolResults = userBlocks.filter(b => b.type === 'tool_result');

  return toolUses.map(use => {
    const result = toolResults.find(r => r.tool_use_id === use.id);
    let resultText: string | undefined;
    if (result) {
      const c = result.content;
      if (typeof c === 'string') resultText = c;
      else if (Array.isArray(c)) {
        resultText = c.filter(b => b.type === 'text').map(b => b.text || '').join('\n');
      }
    }
    return {
      id: use.id ?? '',
      name: use.name ?? 'unknown',
      input: use.input ?? {},
      result: resultText,
    };
  });
}

export function mergeToolCallNodes(
  rawNodes: TraceFlowNode[],
  rawEdges: Edge[]
): { nodes: GraphNode[]; edges: Edge[] } {
  const nodeMap = new Map(rawNodes.map(n => [n.id, n]));
  const nodeIds = new Set(rawNodes.map(n => n.id));

  const outEdges = new Map<string, string[]>();
  for (const n of rawNodes) outEdges.set(n.id, []);
  for (const e of rawEdges) {
    if (nodeIds.has(e.source) && nodeIds.has(e.target)) {
      outEdges.get(e.source)!.push(e.target);
    }
  }

  const toRemove = new Set<string>();
  const nodeRemap = new Map<string, string>();
  const mergedNodes: (ToolFlowNode | TaskFlowNode)[] = [];

  for (const node of rawNodes) {
    if (toRemove.has(node.id)) continue;
    const et = node.data.eventType;
    if (et !== 'assistant' && et !== 'subagent-assistant') continue;
    if (!nodeHasToolUse(node)) continue;

    const allChildren = outEdges.get(node.id) ?? [];
    const childId = allChildren.find(cid => {
      if (toRemove.has(cid)) return false;
      const child = nodeMap.get(cid);
      if (!child) return false;
      const cet = child.data.eventType;
      return (cet === 'user' || cet === 'subagent-user') && nodeHasToolResult(child);
    });
    if (!childId) continue;
    const childNode = nodeMap.get(childId)!;

    let toolCalls = collectToolCalls(node, childNode);
    // ⚡ Bolt Performance Optimization:
    // Replace array with Set to prevent O(N) .has() lookups during graph traversal
    const nodesToAbsorb = new Set<string>();

    const parallelStack = allChildren.filter(cid => {
      if (cid === childId || toRemove.has(cid)) return false;
      const child = nodeMap.get(cid);
      if (!child) return false;
      const cet = child.data.eventType;
      return (cet === 'assistant' || cet === 'subagent-assistant') && nodeHasToolUse(child);
    });

    while (parallelStack.length > 0) {
      const parId = parallelStack.pop()!;
      if (toRemove.has(parId) || nodesToAbsorb.has(parId)) continue;
      const parNode = nodeMap.get(parId);
      if (!parNode) continue;

      const parChildren = outEdges.get(parId) ?? [];
      const parResultId = parChildren.find(cid => {
        if (toRemove.has(cid)) return false;
        const child = nodeMap.get(cid);
        if (!child) return false;
        const cet = child.data.eventType;
        return (cet === 'user' || cet === 'subagent-user') && nodeHasToolResult(child);
      });

      if (parResultId) {
        const parResultNode = nodeMap.get(parResultId)!;
        toolCalls = [...toolCalls, ...collectToolCalls(parNode, parResultNode)];
        nodesToAbsorb.add(parId);
        nodesToAbsorb.add(parResultId);

        for (const cid of parChildren) {
          if (cid === parResultId || toRemove.has(cid)) continue;
          const child = nodeMap.get(cid);
          if (!child) continue;
          const cet = child.data.eventType;
          if ((cet === 'assistant' || cet === 'subagent-assistant') && nodeHasToolUse(child)) {
            parallelStack.push(cid);
          }
        }
      }
    }

    const isTaskCall = toolCalls.some(t => t.name === 'Task');

    if (isTaskCall) {
      const spawnedSubagentId = allChildren
        .map(cid => nodeMap.get(cid))
        .filter(n => n?.data.event.type === 'progress' && n.data.event.data?.agentId)
        .map(n => n!.data.event.data!.agentId)[0];

      const taskTool = toolCalls.find(t => t.name === 'Task')!;
      const taskDescription = String(
        taskTool.input.description ?? taskTool.input.prompt ?? taskTool.input.task ?? ''
      );
      const subagentType = taskTool.input.subagent_type
        ? String(taskTool.input.subagent_type)
        : undefined;

      const taskNodeId = `task-${node.id}`;
      const taskNodeData: TaskNodeData = {
        eventType: 'task-call',
        tools: toolCalls,
        preview: taskDescription.slice(0, 120),
        taskDescription,
        subagentType,
        spawnedSubagentId,
        agentId: node.data.agentId,
        timestamp: node.data.timestamp,
        assistantEvent: node.data.event,
        userEvent: childNode.data.event,
        subagentId: node.data.subagentId,
      };

      mergedNodes.push({
        id: taskNodeId,
        type: 'taskNode' as const,
        data: taskNodeData,
        position: { x: 0, y: 0 },
      } as TaskFlowNode);

      toRemove.add(node.id);
      toRemove.add(childId);
      nodeRemap.set(node.id, taskNodeId);
      nodeRemap.set(childId, taskNodeId);
      for (const absId of nodesToAbsorb) {
        toRemove.add(absId);
        nodeRemap.set(absId, taskNodeId);
      }
    } else {
      const toolNodeId = `tool-${node.id}`;
      const toolNodeData: ToolNodeData = {
        eventType: 'tool-call',
        tools: toolCalls,
        preview: toolCalls.map(t => t.name).join(', '),
        agentId: node.data.agentId,
        timestamp: node.data.timestamp,
        assistantEvent: node.data.event,
        userEvent: childNode.data.event,
        subagentId: node.data.subagentId,
      };

      mergedNodes.push({
        id: toolNodeId,
        type: 'toolNode' as const,
        data: toolNodeData,
        position: { x: 0, y: 0 },
      } as ToolFlowNode);

      toRemove.add(node.id);
      toRemove.add(childId);
      nodeRemap.set(node.id, toolNodeId);
      nodeRemap.set(childId, toolNodeId);
      for (const absId of nodesToAbsorb) {
        toRemove.add(absId);
        nodeRemap.set(absId, toolNodeId);
      }
    }
  }

  const outputNodes: GraphNode[] = [
    ...rawNodes.filter(n => !toRemove.has(n.id)),
    ...mergedNodes,
  ];

  const seenEdges = new Set<string>();
  const outputEdges: Edge[] = [];

  for (const e of rawEdges) {
    if (!nodeIds.has(e.source) || !nodeIds.has(e.target)) continue;
    const src = nodeRemap.get(e.source) ?? e.source;
    const tgt = nodeRemap.get(e.target) ?? e.target;
    if (src === tgt) continue;
    const key = `${src}->${tgt}`;
    if (seenEdges.has(key)) continue;
    seenEdges.add(key);
    outputEdges.push({ ...e, id: `te-${src}-${tgt}`, source: src, target: tgt });
  }

  return { nodes: outputNodes, edges: outputEdges };
}

const LANE_STRIDE = NODE_WIDTH + 30;
const NODE_STEP = 140;
const MARGIN_X = 40;
const MARGIN_Y = 40;
const NODE_GAP = 12;

function getNodeTimestamp(data: Record<string, unknown>): number {
  const ts = data.timestamp;
  if (typeof ts === 'string') {
    const t = new Date(ts).getTime();
    if (!isNaN(t)) return t;
  }
  const events = data.events as Array<{ timestamp?: string }> | undefined;
  if (events) {
    for (const ev of events) {
      if (ev.timestamp) {
        const t = new Date(ev.timestamp).getTime();
        if (!isNaN(t)) return t;
      }
    }
  }
  return 0;
}

function isSubagentType(data: Record<string, unknown>): boolean {
  const events = data.events as Array<{ eventType?: string }> | undefined;
  if (events && events.length > 0) {
    const et = events[0].eventType ?? '';
    return et === 'subagent-user' || et === 'subagent-assistant';
  }
  const et = data.eventType as string | undefined;
  return et === 'subagent-user' || et === 'subagent-assistant';
}

export function layoutGraph<T extends {
  id: string;
  type?: string;
  data: Record<string, unknown>;
  position: { x: number; y: number };
}>(
  nodes: T[],
  edges: Edge[]
): { nodes: T[]; edges: Edge[] } {
  if (nodes.length === 0) return { nodes, edges };

  const nodeMap = new Map(nodes.map(n => [n.id, n]));
  const nodeIds = new Set(nodes.map(n => n.id));

  const inEdges = new Map<string, string[]>();
  const outEdges = new Map<string, string[]>();
  for (const n of nodes) {
    inEdges.set(n.id, []);
    outEdges.set(n.id, []);
  }
  for (const e of edges) {
    if (nodeIds.has(e.source) && nodeIds.has(e.target)) {
      outEdges.get(e.source)!.push(e.target);
      inEdges.get(e.target)!.push(e.source);
    }
  }

  const componentOf = new Map<string, number>();
  let numComponents = 0;
  for (const startNode of nodes) {
    if (componentOf.has(startNode.id)) continue;
    const comp = numComponents++;
    const stack = [startNode.id];
    while (stack.length > 0) {
      const id = stack.pop()!;
      if (componentOf.has(id)) continue;
      componentOf.set(id, comp);
      for (const nb of [...(outEdges.get(id) ?? []), ...(inEdges.get(id) ?? [])]) {
        if (!componentOf.has(nb)) stack.push(nb);
      }
    }
  }

  const compMinTime = new Map<number, number>();
  for (const node of nodes) {
    const comp = componentOf.get(node.id)!;
    const t = getNodeTimestamp(node.data);
    if (!compMinTime.has(comp) || t < compMinTime.get(comp)!) compMinTime.set(comp, t);
  }
  const primaryComp = [...compMinTime.entries()].sort((a, b) => a[1] - b[1])[0]?.[0] ?? 0;

  const primaryNodes = nodes.filter(n => componentOf.get(n.id) === primaryComp);

  const secondaryComps = new Map<number, T[]>();
  for (const node of nodes) {
    const comp = componentOf.get(node.id)!;
    if (comp === primaryComp) continue;
    if (!secondaryComps.has(comp)) secondaryComps.set(comp, []);
    secondaryComps.get(comp)!.push(node);
  }

  const nodeLane = new Map<string, number>();
  const queued = new Set<string>();
  const subagentEdgeKeys = new Set<string>();

  const primaryRoots = primaryNodes.filter(n => (inEdges.get(n.id) ?? []).length === 0);
  const queue: Array<{ id: string; lane: number }> = [];
  for (const r of primaryRoots) {
    queue.push({ id: r.id, lane: 0 });
    queued.add(r.id);
  }
  // OPTIMIZATION: Use an index pointer instead of queue.shift() to avoid O(N^2) complexity
  // caused by shifting contiguous array elements.
  let qIdx = 0;
  while (qIdx < queue.length) {
    const { id, lane } = queue[qIdx++];
    if (nodeLane.has(id)) continue;
    nodeLane.set(id, lane);
    const node = nodeMap.get(id);
    const isTask = node?.type === 'taskNode';
    for (const childId of outEdges.get(id) ?? []) {
      if (queued.has(childId)) continue;
      queued.add(childId);
      const childNode = nodeMap.get(childId);
      if (isTask && childNode && isSubagentType(childNode.data)) {
        queue.push({ id: childId, lane: lane + 1 });
        subagentEdgeKeys.add(`${id}->${childId}`);
      } else {
        queue.push({ id: childId, lane });
      }
    }
  }
  for (const n of primaryNodes) {
    if (!nodeLane.has(n.id)) nodeLane.set(n.id, 0);
  }

  const primaryIdSet = new Set(primaryNodes.map(n => n.id));

  const topoOrder: string[] = [];
  const topoVisited = new Set<string>();
  const dfsStack = primaryNodes
    .filter(n => (inEdges.get(n.id) ?? []).length === 0)
    .map(n => n.id);
  while (dfsStack.length > 0) {
    const id = dfsStack.pop()!;
    if (topoVisited.has(id)) continue;
    topoVisited.add(id);
    topoOrder.push(id);
    const children = (outEdges.get(id) ?? []).filter(c => primaryIdSet.has(c) && !topoVisited.has(c));
    const continuationChildren = children.filter(c => !subagentEdgeKeys.has(`${id}->${c}`));
    const subagentChildren = children.filter(c => subagentEdgeKeys.has(`${id}->${c}`));
    for (const c of [...continuationChildren].reverse()) dfsStack.push(c);
    for (const c of [...subagentChildren].reverse()) dfsStack.push(c);
  }
  for (const n of primaryNodes) {
    if (!topoVisited.has(n.id)) topoOrder.push(n.id);
  }

  const laneCurrentY = new Map<number, number>();
  for (const n of primaryNodes) {
    const lane = nodeLane.get(n.id) ?? 0;
    if (!laneCurrentY.has(lane)) laneCurrentY.set(lane, MARGIN_Y);
  }

  const nodeY = new Map<string, number>();
  const nodeHeightMap = new Map<string, number>();

  for (const id of topoOrder) {
    const lane = nodeLane.get(id) ?? 0;

    for (const parentId of inEdges.get(id) ?? []) {
      if (!nodeY.has(parentId)) continue;
      if ((nodeLane.get(parentId) ?? 0) === lane) continue;
      const parentBottom = nodeY.get(parentId)! + (nodeHeightMap.get(parentId) ?? NODE_HEIGHT) + NODE_GAP;
      if (parentBottom > (laneCurrentY.get(lane) ?? MARGIN_Y)) laneCurrentY.set(lane, parentBottom);
    }

    if (lane === 0) {
      for (const [l, y] of laneCurrentY) {
        if (l !== 0 && y > (laneCurrentY.get(0) ?? MARGIN_Y)) laneCurrentY.set(0, y);
      }
    }

    const yTop = laneCurrentY.get(lane) ?? MARGIN_Y;
    nodeY.set(id, yTop);
    nodeHeightMap.set(id, NODE_HEIGHT);
    laneCurrentY.set(lane, yTop + NODE_HEIGHT + NODE_GAP);
  }

  // OPTIMIZATION: Avoid using Math.max(...nodeLane.values()) to prevent
  // "Maximum call stack size exceeded" errors on very large graphs
  // and eliminate O(N) array allocation overhead from the spread operator.
  let maxPrimaryLane = 0;
  if (nodeLane.size > 0) {
    for (const lane of nodeLane.values()) {
      if (lane > maxPrimaryLane) maxPrimaryLane = lane;
    }
  }
  let nextSecondaryLane = maxPrimaryLane + 1;

  const sortedSecondary = [...secondaryComps.entries()].sort((a, b) => {
    const minA = compMinTime.get(a[0]) ?? 0;
    const minB = compMinTime.get(b[0]) ?? 0;
    return minA - minB;
  });

  for (const [, compNodes] of sortedSecondary) {
    const compLane = nextSecondaryLane++;
    const compRoots = compNodes.filter(n => (inEdges.get(n.id) ?? []).length === 0);
    const visited = new Set<string>();
    const order: string[] = [];
    const bfsQ = compRoots.map(n => n.id);
    // OPTIMIZATION: Use an index pointer instead of bfsQ.shift() to avoid O(N^2) complexity
    // caused by shifting contiguous array elements.
    let bfsQIdx = 0;
    while (bfsQIdx < bfsQ.length) {
      const id = bfsQ[bfsQIdx++];
      if (visited.has(id)) continue;
      visited.add(id);
      order.push(id);
      for (const childId of outEdges.get(id) ?? []) {
        if (!visited.has(childId)) bfsQ.push(childId);
      }
    }
    for (const n of compNodes) {
      if (!visited.has(n.id)) order.push(n.id);
    }
    order.forEach((id, rank) => {
      nodeLane.set(id, compLane);
      nodeY.set(id, MARGIN_Y + rank * NODE_STEP);
      nodeHeightMap.set(id, NODE_HEIGHT);
    });
  }

  const positionedNodes = nodes.map(n => ({
    ...n,
    data: { ...n.data, nodeHeight: nodeHeightMap.get(n.id) ?? NODE_HEIGHT },
    position: {
      x: MARGIN_X + (nodeLane.get(n.id) ?? 0) * LANE_STRIDE,
      y: nodeY.get(n.id) ?? MARGIN_Y,
    },
  }));

  const updatedEdges = edges.map(e => {
    if (subagentEdgeKeys.has(`${e.source}->${e.target}`)) {
      return { ...e, sourceHandle: 'source-right' };
    }
    if (nodeMap.get(e.source)?.type === 'taskNode') {
      return { ...e, sourceHandle: 'source-bottom' };
    }
    return e;
  });

  return { nodes: positionedNodes, edges: updatedEdges };
}
