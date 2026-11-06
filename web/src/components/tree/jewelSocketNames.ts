import type { PassiveNode } from '../../api/types';
import { messages, type Lang } from '../../lib/i18n';
import { hasPosition } from '../../lib/treeGeometry';
import { cleanLineText } from '../../lib/modifierText';
import { passiveNodeName, type PassiveNodeNames } from './passiveNodeNames';

/** A shared, stable display name for tree planning and upgrade positions. IDs stay in the data only. */
export function jewelSocketNames(nodes: PassiveNode[], names: PassiveNodeNames, lang: Lang): Map<number, string> {
  const text = messages(lang, 'jewelPlanner');
  const landmarks = nodes.filter(node => ['notable', 'keystone'].includes(node.kind) && !node.ascendancy_id && hasPosition(node));
  const sockets = nodes.filter(node => node.kind === 'jewel_socket').sort((a, b) => a.skill - b.skill);
  const labels = sockets.map((socket, index) => {
    const near = hasPosition(socket) ? landmarks.map(node => ({ node, distance: Math.hypot(node.x! - socket.x, node.y! - socket.y) }))
      .filter(entry => entry.distance <= 1800).sort((a, b) => a.distance - b.distance || a.node.skill - b.node.skill)[0]?.node : undefined;
    const label = near ? cleanLineText(passiveNodeName(near, names)) : '';
    return label ? text.socketNear.replace('{node}', label) : text.socketIndex.replace('{index}', String(index + 1));
  });
  return new Map(sockets.map((socket, index) => [socket.skill, labels.filter(label => label === labels[index]).length > 1
    ? `${labels[index]} · ${index + 1}` : labels[index]]));
}
