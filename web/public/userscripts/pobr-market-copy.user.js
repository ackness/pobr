// ==UserScript==
// @name         PoBR - Market item copy
// @name:zh-CN   PoBR - 市集装备一键复制
// @namespace    https://github.com/ackness/pobr
// @version      0.1.1
// @description Copy one market item as complete PoBR text, including requirements and socket effects.
// @description:zh-CN 在商品旁添加「复制到 PoBR」，保留需求、隐式、符文和其他词缀，直接粘贴比较。
// @match        https://poe.game.qq.com/trade2/search/*
// @match        https://www.pathofexile.com/trade2/search/*
// @grant        GM.setClipboard
// @run-at       document-idle
// @noframes
// @homepageURL  https://pobr-web.pages.dev/
// @downloadURL https://pobr-web.pages.dev/userscripts/pobr-market-copy.user.js
// @updateURL   https://pobr-web.pages.dev/userscripts/pobr-market-copy.user.js
// ==/UserScript==

(() => {
  'use strict';
  if (!['poe.game.qq.com', 'www.pathofexile.com'].includes(location.hostname)
      || !location.pathname.startsWith('/trade2/search/') || document.querySelector('#pobr-market-copy-style')) return;
  const zh = location.hostname === 'poe.game.qq.com';
  const message = (cn, en) => zh ? cn : en;
  const label = message('复制到 PoBR', 'Copy for PoBR');
  const separator = '--------';
  const propertyNames = new Map([
    [6, 'Quality'], [9, 'Physical Damage'], [11, 'Chaos Damage'], [12, 'Critical Hit Chance'],
    [13, 'Attacks per Second'], [15, 'Block'], [16, 'Armour'], [17, 'Evasion Rating'], [18, 'Energy Shield'],
  ]);
  const requirements = new Map([[62, 'Level'], [63, 'Str'], [64, 'Dex'], [65, 'Int']]);
  const array = value => Array.isArray(value) ? value : [];
  // GGG links use [Keyword|display text]. Strip links, not rolls or conditional wording.
  const clean = value => String(value ?? '').replace(/\[([^\[\]]+)\]/g, (_, text) => text.split('|').at(-1))
    .replace(/\r/g, '').trim();
  const linesOf = value => clean(typeof value === 'string' ? value : value?.description).split('\n').filter(line => line.trim());

  function itemText(item) {
    const rarity = ['NORMAL', 'MAGIC', 'RARE', 'UNIQUE'][item.frameType];
    const base = clean(item.baseType || item.typeLine);
    if (!rarity || !base || /[\n\r]/.test(base)) throw new Error(message('缺少装备基底或稀有度，无法安全复制。', 'Missing equipment base or supported rarity.'));
    // Magic items have one canonical base line. Keep the decorated name as metadata.
    const lines = [`Rarity: ${rarity}`];
    if (rarity === 'RARE' || rarity === 'UNIQUE') lines.push(clean(item.name || base).replace(/\n/g, ' '));
    lines.push(base, separator);
    if (rarity === 'MAGIC' && item.typeLine && clean(item.typeLine) !== base) lines.push(`Note: ${clean(item.typeLine).replace(/\n/g, ' ')}`);
    if (Number.isInteger(item.ilvl)) lines.push(`Item Level: ${item.ilvl}`);
    for (const property of array(item.properties)) {
      const values = array(property.values).map(value => clean(value[0]));
      if (!values.length) continue;
      if (property.displayMode === 3) {
        lines.push(`Note: ${clean(property.name).replace(/\{(\d+)\}/g, (placeholder, index) => values[Number(index)] ?? placeholder)}`);
      } else if (property.type === 10) {
        // Elemental roll colors identify the damage type even on localized sites.
        array(property.values).forEach(value => {
          const element = ({ 4: 'Fire', 5: 'Cold', 6: 'Lightning' })[value[1]];
          lines.push(`Note: ${element ? `${element} Damage` : clean(property.name)}: ${clean(value[0])}`);
        });
      } else {
        const rawName = clean(property.name);
        const name = propertyNames.get(property.type) || ({ 精魂: 'Spirit', 符文结界: 'Ward', 符文結界: 'Ward' })[rawName] || rawName;
        // Defence rolls are inputs; weapon rolls and utility properties are reference metadata.
        const prefix = ['Quality', 'Block', 'Armour', 'Evasion Rating', 'Energy Shield', 'Spirit', 'Ward'].includes(name) ? '' : 'Note: ';
        if (name) lines.push(`${prefix}${name}: ${values.join(', ')}`);
      }
    }
    const req = array(item.requirements).map(entry => {
      const rawName = clean(entry.name);
      const name = requirements.get(entry.type) || ({ Level: 'Level', 等级: 'Level', 等級: 'Level', Str: 'Str', Strength: 'Str', 力量: 'Str', Dex: 'Dex', Dexterity: 'Dex', 敏捷: 'Dex', Int: 'Int', Intelligence: 'Int', 智慧: 'Int' })[rawName];
      return `${name || `Note: Requirement - ${rawName}`}: ${array(entry.values).map(value => clean(value[0])).join(', ')}`;
    });
    if (req.length) lines.push(separator, 'Requirements:', ...req);
    const sockets = array(item.sockets);
    if (sockets.length) {
      lines.push(separator, `Sockets: ${sockets.map(socket => socket.type === 'jewel' ? 'J' : 'S').join(' ')}`);
      for (const [index, socket] of sockets.entries()) {
        const rune = array(item.socketedItems).find(entry => entry.socket === index);
        if (!rune) continue;
        if (socket.type === 'jewel') {
          lines.push('Unmodeled market effect: equipment-socketed jewel');
          continue;
        }
        const name = clean(rune.baseType || rune.typeLine);
        if (name) lines.push(`Rune: ${name}`);
      }
    }
    const implicit = array(item.implicitMods).flatMap(linesOf);
    lines.push(separator, `Implicits: ${implicit.length}`, ...implicit, separator);
    for (const [field, prefix] of [['enchantMods', '{enchant}'], ['runeMods', '{rune}'], ['explicitMods', ''], ['craftedMods', '{crafted}'], ['fracturedMods', '{fractured}']]) {
      for (const line of array(item[field]).flatMap(linesOf)) lines.push(prefix + line);
    }
    // Keep unknown effects visible to PoBR instead of inventing a numerical benefit.
    for (const [field, value] of Object.entries(item)) {
      // Search aggregates (Sum, combined resistance, etc.) duplicate item stats
      // or depend on the query; they are not item effects or unsupported mods.
      if (field === 'pseudoMods') continue;
      if (field.endsWith('Mods') && !['implicitMods', 'enchantMods', 'runeMods', 'explicitMods', 'craftedMods', 'fracturedMods'].includes(field)) {
        lines.push(...array(value).flatMap(linesOf).map(line => `Unmodeled market effect (${field}): ${line}`));
      }
    }
    if (item.corrupted) lines.push('Corrupted');
    if (item.mirrored) lines.push('Mirrored');
    if (item.identified === false) lines.push('Unidentified');
    const text = lines.join('\n');
    if (text.length > 50000) throw new Error(message('物品文本过长，请使用市集原生复制。', 'Item text is too large. Use the native market copy.'));
    return text;
  }

  async function fetchItem(id) {
    if (!/^[a-f0-9]{64}$/.test(id)) throw new Error(message('无法识别这件商品。', 'Cannot identify this listing.'));
    const parts = location.pathname.split('/').filter(Boolean);
    const query = parts[2] === 'poe2' ? parts[4] : parts[3];
    if (!query || !/^[\w-]+$/.test(query)) throw new Error(message('请先完成一次市集搜索。', 'Run a market search first.'));
    const url = new URL(`/api/trade2/fetch/${id}`, location.origin);
    url.searchParams.set('query', query); url.searchParams.set('realm', 'poe2');
    const response = await fetch(url, { credentials: 'same-origin', redirect: 'error', signal: AbortSignal.timeout(15000) });
    if (!response.ok) throw new Error(response.status === 429
      ? message('市集限流，请稍后手动重试。', 'Market rate limit. Try again later.')
      : message(`市集返回 ${response.status}，请检查登录状态。`, `Market returned ${response.status}. Check your login.`));
    const body = await response.text();
    if (body.length > 1000000) throw new Error(message('商品响应异常。', 'Unexpected listing response.'));
    let data;
    try { data = JSON.parse(body); } catch { throw new Error(message('市集需要验证，请先在页面完成验证。', 'Complete market verification first.')); }
    const item = array(data.result).find(entry => entry?.id === id)?.item;
    if (!item) throw new Error(message('商品已下架或暂时不可用。', 'The listing is no longer available.'));
    return item;
  }

  function manualCopy(text) {
    const dialog = document.createElement('dialog'); dialog.className = 'pobr-copy-dialog';
    const title = document.createElement('h3'); title.textContent = message('剪贴板不可用，请手动复制', 'Select and copy this item text');
    const textarea = document.createElement('textarea'); textarea.value = text; textarea.readOnly = true;
    textarea.setAttribute('aria-label', message('完整物品文本', 'Complete item text'));
    const close = document.createElement('button'); close.textContent = message('关闭', 'Close'); close.onclick = () => dialog.close();
    dialog.append(title, textarea, close); dialog.addEventListener('close', () => dialog.remove(), {once: true});
    document.body.append(dialog); dialog.showModal(); textarea.focus(); textarea.select();
  }

  const style = document.createElement('style'); style.id = 'pobr-market-copy-style';
  style.textContent = `
    .pobr-copy-tools {display:flex;flex-wrap:wrap;gap:6px;align-items:center;margin-top:8px;max-width:100%}
    .pobr-copy-tools button {padding:6px 10px;border:1px solid #b69755;border-radius:5px;background:#292821;color:#e6c77f;font:inherit;cursor:pointer}
    .pobr-copy-tools button:disabled {opacity:.6;cursor:wait}
    .pobr-copy-status {font-size:12px;color:#dacba8;overflow-wrap:anywhere;flex-basis:100%}
    .pobr-copy-dialog {width:min(620px,85vw);padding:20px;border:1px solid #b69755;border-radius:8px;background:#191c22;color:#eee}
    .pobr-copy-dialog::backdrop {background:#0009}
    .pobr-copy-dialog textarea {box-sizing:border-box;width:100%;height:300px;background:#111318;color:#eee;padding:12px}
    .pobr-copy-dialog button {margin-top:10px;padding:6px 14px}
  `;
  document.head.append(style);
  function decorate() {
    for (const row of document.querySelectorAll('.resultset .row[data-id]')) {
      if (row.querySelector('.pobr-copy-tools') || !row.querySelector('.item-popup, .itemBoxContent')) continue;
      const controls = document.createElement('div'); controls.className = 'pobr-copy-tools';
      const button = document.createElement('button'); button.type = 'button'; button.textContent = label;
      const status = document.createElement('span'); status.className = 'pobr-copy-status'; status.setAttribute('role', 'status');
      button.onclick = async event => {
        event.preventDefault(); event.stopPropagation();
        button.disabled = true; status.textContent = message('正在读取这件商品…', 'Reading this item…');
        try {
          const id = row.dataset.id, search = location.href;
          const isCurrent = () => {
            if (!row.isConnected) return false;
            if (row.dataset.id !== id || location.href !== search) throw new Error(message('搜索结果已变化，请重新点击。', 'Search results changed. Click again.'));
            return true;
          };
          const text = itemText(await fetchItem(id));
          if (!isCurrent()) return;
          try {
            if (typeof GM !== 'undefined' && GM.setClipboard) await GM.setClipboard(text, 'text');
            else await navigator.clipboard.writeText(text);
            status.textContent = message('已复制，回到 PoBR「提升」页粘贴。', 'Copied. Paste into PoBR → Upgrades.');
          } catch {
            if (!isCurrent()) return;
            manualCopy(text); status.textContent = message('已打开手动复制窗口。', 'Manual copy opened.');
          }
        } catch (error) { status.textContent = error instanceof Error ? error.message : message('复制失败。', 'Copy failed.'); }
        finally { button.disabled = false; }
      };
      controls.append(button, status); (row.querySelector('.right .details') || row).append(controls);
    }
  }
  let timer;
  const observer = new MutationObserver(records => {
    if (!records.some(record => [...record.addedNodes].some(node => node.nodeType === 1 && !node.closest?.('.pobr-copy-tools, .pobr-copy-dialog')))) return;
    clearTimeout(timer); timer = setTimeout(decorate, 100);
  });
  observer.observe(document.body, {childList: true, subtree: true});
  decorate();
})();
