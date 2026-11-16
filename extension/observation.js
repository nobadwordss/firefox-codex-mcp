/* Bounded, rendered-page observation. No page HTML or form/editing buffers. */
(() => {
  "use strict";
  const LIMITS = { nodes: 10000, sourceChars: 60000, ranges: 2048, text: 12000, headings: 30, statuses: 30, elements: 200, candidates: 2000 };
  const BLOCKED = 'script,style,noscript,template,input,textarea,select,option,[contenteditable]:not([contenteditable="false"]),[role="textbox"],[role="combobox"]';
  const REGIONS = '[role="alert"],[role="status"],[role="log"],[role="alertdialog"],[aria-live]:not([aria-live="off"]),dialog[open],[aria-modal="true"]';
  const MODALS = 'dialog[open],[aria-modal="true"],[role="alertdialog"]';
  function normalize(text) { return String(text || "").replace(/\s+/g, " ").trim(); }
  function clip(rect, box) {
    return { left: Math.max(rect.left, box.left), top: Math.max(rect.top, box.top), right: Math.min(rect.right, box.right), bottom: Math.min(rect.bottom, box.bottom) };
  }
  function area(rect) { return rect.right - rect.left > 1 && rect.bottom - rect.top > 1; }
  function hidden(element) {
    for (let parent = element; parent; parent = parent.parentElement) {
      const style = getComputedStyle(parent);
      if (parent.hidden || parent.hasAttribute('inert') || parent.getAttribute('aria-hidden') === 'true' || style.display === 'none' || style.visibility !== 'visible' || Number(style.opacity) === 0 || style.contentVisibility === 'hidden' || (style.clip && style.clip !== 'auto') || (style.clipPath && style.clipPath !== 'none')) return true;
    }
    return false;
  }
  function clippedRect(element, rect = element.getBoundingClientRect()) {
    let result = clip(rect, {left: 0, top: 0, right: innerWidth, bottom: innerHeight});
    for (let parent = element; parent && area(result); parent = parent.parentElement) {
      const style = getComputedStyle(parent), box = parent.getBoundingClientRect();
      if (/(auto|scroll|hidden|clip)/.test(style.overflowX)) result = {...result, left: Math.max(result.left, box.left), right: Math.min(result.right, box.right)};
      if (/(auto|scroll|hidden|clip)/.test(style.overflowY)) result = {...result, top: Math.max(result.top, box.top), bottom: Math.min(result.bottom, box.bottom)};
    }
    return result;
  }
  function visible(element) { return !hidden(element) && area(clippedRect(element)); }
  function hitVisible(element, rect) {
    // Inline links can wrap: their union-box center may fall between lines.
    const fragments = rect ? [rect] : Array.from(element.getClientRects()).slice(0, 100).map(rect => clippedRect(element, rect));
    return fragments.some(fragment => {
      if (!area(fragment)) return false;
      const hit = document.elementFromPoint((fragment.left + fragment.right) / 2, (fragment.top + fragment.bottom) / 2);
      return hit === element || !!hit && element.contains(hit);
    });
  }
  function excluded(element) { return !!element.closest(BLOCKED) || hidden(element); }
  function renderedText(node, end, budget) {
    const range = document.createRange(), segments = [];
    function visit(start, end) {
      if (start >= end) return;
      if (--budget.ranges < 0) {budget.truncated = true; return;}
      range.setStart(node, start); range.setEnd(node, end);
      let any = false, all = true;
      for (const rect of range.getClientRects()) {
        if (!area(rect)) continue;
        const clipped = clippedRect(node.parentElement, rect);
        const rendered = area(clipped) && hitVisible(node.parentElement, clipped);
        any ||= rendered;
        all &&= rendered && ['left','top','right','bottom'].every(key => Math.abs(clipped[key] - rect[key]) < 0.5);
      }
      if (!any) return;
      if (all || end - start <= 1) {
        const previous = segments.at(-1);
        if (previous?.end === start) previous.end = end;
        else segments.push({start, end});
        return;
      }
      const middle = start + Math.floor((end - start) / 2);
      visit(start, middle); visit(middle, end);
    }
    visit(0, end);
    return segments.map(({start, end}) => node.nodeValue.slice(start, end)).join('\n');
  }
  function text(root, max = LIMITS.text, viewport = true) {
    if (!root || excluded(root)) return {text: '', truncated: false};
    let node = root, visited = 0, used = 0, parts = [], truncated = false;
    const budget = {chars: LIMITS.sourceChars, ranges: LIMITS.ranges, truncated: false};
    while (node) {
      if (++visited > LIMITS.nodes) {truncated = true; break;}
      let descend = node.nodeType === 1 && !excluded(node);
      if (node.nodeType === 3) {
        const end = Math.min(node.nodeValue.length, budget.chars);
        budget.chars -= end;
        budget.truncated ||= end < node.nodeValue.length;
        // Split boundary ranges so one visible line cannot expose an offscreen tail.
        const value = normalize(viewport ? renderedText(node, end, budget) : node.nodeValue.slice(0, end));
        if (value) {
          const remaining = max - used;
          parts.push(value.slice(0, remaining)); used += Math.min(value.length, remaining) + 1;
          if (value.length >= remaining) {truncated = true; break;}
        }
        if (budget.truncated) {truncated = true; break;}
      }
      if (descend && node.firstChild) {node = node.firstChild; continue;}
      while (node !== root && !node.nextSibling) node = node.parentNode;
      if (node === root) break;
      node = node.nextSibling;
    }
    return {text: parts.join('\n'), truncated: truncated || budget.truncated};
  }
  function disabled(element) { return element.matches(':disabled') || !!element.closest('[aria-disabled="true"],[inert]'); }
  function busy(element) { return !!element.closest('[aria-busy="true"]'); }
  function state(element) {
    const result = {disabled: disabled(element), readOnly: !!element.readOnly, busy: busy(element)};
    for (const key of ['checked','expanded','selected','pressed','invalid','required']) {
      const value = element.getAttribute(`aria-${key}`);
      if (value !== null) result[key] = value;
    }
    if (element instanceof HTMLInputElement && ['checkbox','radio'].includes(element.type)) result.checked = element.indeterminate ? 'mixed' : element.checked;
    return result;
  }
  function label(element) {
    const labelled = (element.getAttribute('aria-labelledby') || '').split(/\s+/).filter(Boolean);
    const named = labelled.map(id => text(document.getElementById(id), 2000, false).text).join(' ');
    return normalize(named || element.getAttribute('aria-label') || (element.labels ? Array.from(element.labels).map(el => text(el, 2000, false).text).join(' ') : '') || element.getAttribute('title') || element.getAttribute('placeholder') || text(element, 2000, false).text);
  }
  function describe(element, id) {
    const status = state(element), actions = [];
    if (!status.disabled && !status.busy && element.tagName !== 'SELECT') {
      actions.push('click');
      if (!status.readOnly && (element instanceof HTMLTextAreaElement || element instanceof HTMLInputElement && ['text','search','email','url','tel','number'].includes(element.type))) actions.push('type');
    }
    return {id, tag: element.tagName.toLowerCase(), role: element.getAttribute('role'), type: element instanceof HTMLInputElement ? element.type : undefined, label: label(element).slice(0,160), ...status, actions};
  }
  function regions(selector = REGIONS) {
    const result = [], nodes = document.querySelectorAll(selector);
    let truncated = false, scanned = 0;
    for (const node of nodes) {
      if (++scanned > LIMITS.candidates) {truncated = true; break;}
      if (!visible(node) || excluded(node)) continue;
      if (result.length === LIMITS.statuses) {truncated = true; break;}
      const content = text(node, 1000);
      result.push({node, text: content.text, role: node.getAttribute('role'), live: node.getAttribute('aria-live'), busy: busy(node), truncated: content.truncated});
      truncated ||= content.truncated;
    }
    return {items: result, truncated};
  }
  function regionKey(regions) { return JSON.stringify(regions.items.map(({node, ...item}) => item)); }
  function fingerprint(element, contextCache) {
    const anchor = element.closest('a[href]'), form = element.form || element.closest('form');
    const context = element.closest('form,dialog,[role="dialog"],[role="alertdialog"],[aria-modal="true"],fieldset,section,article,[role="group"],[role="region"]') || element.parentElement;
    const nearby = contextCache?.get(context) || text(context, 4000);
    contextCache?.set(context, nearby);
    const description = {tag: element.tagName, role: element.getAttribute('role'), type: element.type, label: label(element), ...state(element), href: anchor?.href, download: anchor?.getAttribute('download'), target: anchor?.target, formAction: form?.action, formMethod: form?.method, formTarget: form?.target};
    for (const name of ['formaction','formmethod','formtarget','name','aria-describedby']) description[name] = element.getAttribute(name);
    // Values stay inside this content script, solely to detect edits since observation.
    if (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement) description.buffer = element.value;
    if (element.tagName === 'SELECT') description.selection = element.selectedIndex;
    description.description = (element.getAttribute('aria-describedby') || '').split(/\s+/).filter(Boolean).map(id => text(document.getElementById(id), 2000, false).text);
    return {key: JSON.stringify(description), context, nearby, rect: element.getBoundingClientRect()};
  }
  function sameTarget(previous, element) {
    const next = fingerprint(element);
    return previous.key === next.key && previous.context === next.context && !previous.nearby.truncated && !next.nearby.truncated && previous.nearby.text === next.nearby.text && ['left','top','width','height'].every(key => Math.abs(previous.rect[key] - next.rect[key]) <= 2);
  }
  globalThis.FirefoxCodexObservation = {LIMITS, MODALS, normalize, visible, hitVisible, text, state, label, describe, regions, regionKey, fingerprint, sameTarget, disabled, busy};
})();
